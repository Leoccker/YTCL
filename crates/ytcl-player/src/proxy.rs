//! Protocolo custom `ytclstream://` para o libmpv.
//!
//! Por que existe: o ffmpeg do mpv faz a primeira requisição HTTP com
//! `Range: bytes=0-` (aberto), e o YouTube passou a devolver **403** para
//! algumas URLs nesse caso — só aceita Range limitado. Aqui a gente serve o
//! stream para o mpv usando `reqwest` (rustls) com Range em blocos, o que o
//! YouTube aceita.
//!
//! Os callbacks do libmpv são `fn` puros (não capturam), rodam numa thread do
//! mpv (não-tokio) e são síncronos. Então:
//! - o estado do runtime + cliente HTTP vai no `user_data` (`ProxyEnv`);
//! - a URL, o UA e o tamanho vão codificados na própria URI;
//! - cada `read` que precisa de dados faz um `block_on` de um GET com Range.

use base64::Engine;
use serde::{Deserialize, Serialize};

/// Blocos de 256 KiB. O YouTube devolve 403 para Range acima de ~800 KB em
/// algumas URLs (limite anti-abuso, varia por vídeo), então ficamos bem
/// abaixo disso. Um Opus de 4 min ≈ 4 MB = ~16 requisições, custo desprezível.
const CHUNK: u64 = 256 * 1024;

/// Passado a todo `open`. Clonável e barato.
pub struct ProxyEnv {
    pub rt: tokio::runtime::Handle,
    pub http: reqwest::Client,
}

// Os callbacks do libmpv rodam sob `catch_unwind`, que exige `RefUnwindSafe`.
// `reqwest::Client` e `Handle` têm interior mutability mas um panic num
// callback de leitura não deixa nada observável em estado ruim — no pior caso
// uma requisição é abortada. Afirmamos a segurança.
impl std::panic::RefUnwindSafe for ProxyEnv {}

/// O que a URI `ytclstream://<base64>` carrega.
#[derive(Serialize, Deserialize)]
pub struct StreamSpec {
    pub url: String,
    pub ua: String,
    pub size: u64,
}

impl StreamSpec {
    /// Monta a URI que vai para o `loadfile` do mpv.
    pub fn to_uri(&self) -> String {
        let json = serde_json::to_vec(self).expect("StreamSpec serializa");
        format!(
            "ytclstream://{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
        )
    }

    fn from_uri(uri: &str) -> Option<Self> {
        let b64 = uri.strip_prefix("ytclstream://")?;
        let json = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(b64).ok()?;
        serde_json::from_slice(&json).ok()
    }
}

/// Estado por stream aberto (o "cookie" do libmpv).
pub struct Stream {
    rt: tokio::runtime::Handle,
    http: reqwest::Client,
    url: String,
    ua: String,
    size: u64,
    /// Offset de leitura atual.
    pos: u64,
    /// Bloco baixado e o offset em que ele começa.
    buf: Vec<u8>,
    buf_start: u64,
    /// `true` depois de um erro de rede — `read` passa a devolver -1 e o mpv
    /// dispara EndFile(Error), que o `Player` trata re-resolvendo.
    failed: bool,
}

impl std::panic::RefUnwindSafe for Stream {}

pub fn open(env: &mut ProxyEnv, uri: &str) -> Stream {
    // `open` não pode falhar com Result; uma URI inválida vira um stream
    // "morto" que devolve EOF na hora.
    let spec = StreamSpec::from_uri(uri).unwrap_or(StreamSpec {
        url: String::new(),
        ua: String::new(),
        size: 0,
    });
    Stream {
        rt: env.rt.clone(),
        http: env.http.clone(),
        url: spec.url,
        ua: spec.ua,
        size: spec.size,
        pos: 0,
        buf: Vec::new(),
        buf_start: 0,
        failed: false,
    }
}

pub fn close(_: Box<Stream>) {}

pub fn size(s: &mut Stream) -> i64 {
    s.size as i64
}

pub fn seek(s: &mut Stream, offset: i64) -> i64 {
    if offset < 0 || offset as u64 > s.size {
        return -1;
    }
    s.pos = offset as u64;
    s.pos as i64
}

pub fn read(s: &mut Stream, out: &mut [std::os::raw::c_char]) -> i64 {
    if s.failed || s.url.is_empty() {
        return if s.failed { -1 } else { 0 };
    }
    if s.pos >= s.size {
        return 0;
    }

    // O offset pedido está no bloco atual?
    let in_buf = s.pos >= s.buf_start && s.pos < s.buf_start + s.buf.len() as u64;
    if !in_buf && !fetch_block(s, s.pos) {
        s.failed = true;
        return -1;
    }

    let start = (s.pos - s.buf_start) as usize;
    let avail = &s.buf[start..];
    let n = avail.len().min(out.len());
    // SAFETY: c_char e u8 têm o mesmo layout; copiamos n bytes válidos.
    let dst = unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr() as *mut u8, out.len()) };
    dst[..n].copy_from_slice(&avail[..n]);
    s.pos += n as u64;
    n as i64
}

/// Baixa o bloco que contém `at`. `false` em qualquer falha.
fn fetch_block(s: &mut Stream, at: u64) -> bool {
    let start = (at / CHUNK) * CHUNK;
    let end = (start + CHUNK - 1).min(s.size.saturating_sub(1));
    let range = format!("bytes={start}-{end}");

    let url = s.url.clone();
    let ua = s.ua.clone();
    let http = s.http.clone();
    let result = s.rt.block_on(async move {
        let resp = http
            .get(&url)
            .header(reqwest::header::USER_AGENT, ua)
            .header(reqwest::header::RANGE, range)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("HTTP {}", resp.status()));
        }
        Ok::<_, anyhow::Error>(resp.bytes().await?)
    });

    match result {
        Ok(bytes) => {
            s.buf = bytes.to_vec();
            s.buf_start = start;
            true
        }
        Err(e) => {
            tracing::warn!("proxy: bloco {start}-{end} falhou: {e}");
            false
        }
    }
}
