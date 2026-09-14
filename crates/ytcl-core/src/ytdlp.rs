//! Resolução de stream via `yt-dlp`.
//!
//! Por que não o rustypipe: a decifragem de assinatura dele quebrou contra o
//! `base.js` atual do YouTube (todos os clientes que precisam dela falham), e
//! as URLs do cliente iOS — o único que dispensa decifragem — vêm truncadas
//! em ~800 KB. O `yt-dlp` é mantido de perto e resolve tudo. Ver
//! `docs/plano.md > Problemas conhecidos`.
//!
//! Custo: ~1–2 s por chamada (startup do yt-dlp + rede). A pré-resolução da
//! próxima faixa (~20 s antes do fim) esconde isso no gapless.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::{CoreError, Result};
use crate::stream::{AudioCodec, ResolvedStream, StreamResolver};

pub struct YtDlp {
    /// Caminho do binário. `yt-dlp` (do PATH) por padrão.
    binary: String,
}

impl Default for YtDlp {
    fn default() -> Self {
        Self {
            binary: "yt-dlp".into(),
        }
    }
}

impl YtDlp {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    /// Confere que o binário responde. Chamado na inicialização para o app
    /// avisar cedo se falta o yt-dlp.
    pub async fn check(&self) -> Result<String> {
        let out = tokio::process::Command::new(&self.binary)
            .arg("--version")
            .output()
            .await
            .map_err(|e| {
                CoreError::Other(format!("yt-dlp não encontrado ({}): {e}", self.binary))
            })?;
        if !out.status.success() {
            return Err(CoreError::Other("yt-dlp --version falhou".into()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

/// Um item de `formats` do JSON do yt-dlp — ou o próprio topo quando `-f`
/// seleciona um formato único.
#[derive(Deserialize)]
struct YtFormat {
    url: String,
    #[serde(default)]
    acodec: Option<String>,
    #[serde(default)]
    abr: Option<f64>,
    #[serde(default)]
    tbr: Option<f64>,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    http_headers: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize)]
struct YtJson {
    #[serde(flatten)]
    fmt: YtFormat,
    #[serde(default)]
    duration: Option<f64>,
}

fn parse_expire(url: &str) -> u64 {
    url.split(['?', '&'])
        .find_map(|p| p.strip_prefix("expire="))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| crate::epoch_secs() + 6 * 3600)
}

/// Extrai uma resposta `Content-Range` completa. O tamanho que o mpv recebe
/// precisa ser exato: um valor aproximado faz o EOF e os seeks caírem no
/// offset errado.
fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim().strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.parse().ok()?;
    let end = end.parse().ok()?;
    let total = total.parse().ok()?;
    (start <= end && end < total).then_some((start, end, total))
}

/// Descobre o tamanho quando o yt-dlp não o colocou no JSON. Um Range
/// fechado evita baixar a faixa inteira e o `206` impede aceitar servidores
/// que ignoram Range e respondem o arquivo começando no byte zero.
async fn probe_size(url: &str, user_agent: Option<&str>) -> Result<u64> {
    let mut request = reqwest::Client::new()
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0");
    if let Some(user_agent) = user_agent.filter(|ua| !ua.is_empty()) {
        request = request.header(reqwest::header::USER_AGENT, user_agent);
    }

    let response = request
        .send()
        .await
        .map_err(|e| CoreError::Network(format!("sondando tamanho do stream: {e}")))?;
    size_from_probe_response(
        response.status(),
        response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok()),
    )
}

fn size_from_probe_response(
    status: reqwest::StatusCode,
    content_range: Option<&str>,
) -> Result<u64> {
    if status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(CoreError::Parse(format!(
            "stream não aceitou Range fechado para descobrir tamanho (HTTP {})",
            status
        )));
    }
    let content_range = content_range
        .and_then(parse_content_range)
        .filter(|&(start, end, _)| start == 0 && end == 0)
        .ok_or_else(|| {
            CoreError::Parse("Content-Range inválido ao descobrir tamanho do stream".into())
        })?;

    Ok(content_range.2)
}

#[async_trait]
impl StreamResolver for YtDlp {
    async fn resolve(&self, video_id: &str) -> Result<ResolvedStream> {
        // videoId puro é aceito pelo yt-dlp. `--` encerra as flags.
        let out = tokio::process::Command::new(&self.binary)
            .args([
                "-J",
                "--no-warnings",
                "--no-playlist",
                "-f",
                "bestaudio[acodec=opus]/bestaudio",
                "--",
                video_id,
            ])
            .output()
            .await
            .map_err(|e| CoreError::Other(format!("chamando o yt-dlp: {e}")))?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let msg = err.lines().last().unwrap_or("erro desconhecido").trim();
            // yt-dlp devolve texto humano; classificamos o que dá.
            if msg.contains("Private") || msg.contains("sign in") || msg.contains("members-only") {
                return Err(CoreError::SessionExpired);
            }
            if msg.contains("not available") || msg.contains("unavailable") {
                return Err(CoreError::NotFound);
            }
            return Err(CoreError::Parse(format!("yt-dlp: {msg}")));
        }

        let json: YtJson = serde_json::from_slice(&out.stdout)
            .map_err(|e| CoreError::Parse(format!("JSON do yt-dlp: {e}")))?;
        let f = json.fmt;

        let codec = match f.acodec.as_deref() {
            Some(c) if c.contains("opus") => AudioCodec::Opus,
            _ => AudioCodec::Aac,
        };
        let bitrate = f
            .abr
            .or(f.tbr)
            .map(|k| (k * 1000.0) as u32)
            .unwrap_or(128_000);
        let user_agent = f
            .http_headers
            .as_ref()
            .and_then(|h| h.get("User-Agent").or_else(|| h.get("user-agent")))
            .cloned();
        // `filesize_approx` não serve aqui: o callback `size` do libmpv e o
        // último Range exigem o tamanho real. Quando o yt-dlp não sabe,
        // consultamos a própria URL antes de entregar o stream ao player.
        let size = match f.filesize.filter(|size| *size > 0) {
            Some(size) => size,
            None => probe_size(&f.url, user_agent.as_deref()).await?,
        };

        Ok(ResolvedStream {
            expires_at: parse_expire(&f.url),
            url: f.url,
            codec,
            bitrate,
            // yt-dlp não expõe o loudnessDb do YouTube; sem normalização por ora.
            loudness_db: None,
            duration_secs: json.duration.map(|d| d.round() as u32).filter(|d| *d > 0),
            user_agent,
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expire_vem_da_url() {
        assert_eq!(
            parse_expire("https://x/videoplayback?foo=1&expire=1789088346&bar=2"),
            1789088346
        );
    }

    #[test]
    fn sem_expire_usa_fallback() {
        let e = parse_expire("https://x/videoplayback?foo=1");
        assert!(e > crate::epoch_secs());
    }

    #[test]
    fn expire_como_primeiro_parametro() {
        assert_eq!(
            parse_expire("https://x/videoplayback?expire=1789088346&bar=2"),
            1789088346
        );
    }

    #[test]
    fn content_range_exige_total_e_intervalo_validos() {
        assert_eq!(parse_content_range("bytes 0-0/123"), Some((0, 0, 123)));
        assert_eq!(parse_content_range("bytes 10-9/123"), None);
        assert_eq!(parse_content_range("bytes 0-0/*"), None);
        assert_eq!(parse_content_range("items 0-0/123"), None);
    }

    #[test]
    fn sonda_so_aceita_range_fechado_com_total_exato() {
        assert_eq!(
            size_from_probe_response(
                reqwest::StatusCode::PARTIAL_CONTENT,
                Some("bytes 0-0/12345")
            )
            .unwrap(),
            12_345
        );
        assert!(size_from_probe_response(reqwest::StatusCode::OK, None).is_err());
        assert!(size_from_probe_response(
            reqwest::StatusCode::PARTIAL_CONTENT,
            Some("bytes 1-1/12345")
        )
        .is_err());
    }
}
