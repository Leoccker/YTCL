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
        Self { binary: "yt-dlp".into() }
    }
}

impl YtDlp {
    pub fn new(binary: impl Into<String>) -> Self {
        Self { binary: binary.into() }
    }

    /// Confere que o binário responde. Chamado na inicialização para o app
    /// avisar cedo se falta o yt-dlp.
    pub async fn check(&self) -> Result<String> {
        let out = tokio::process::Command::new(&self.binary)
            .arg("--version")
            .output()
            .await
            .map_err(|e| CoreError::Other(format!("yt-dlp não encontrado ({}): {e}", self.binary)))?;
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
    filesize_approx: Option<u64>,
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
    url.split('&')
        .find_map(|p| p.strip_prefix("expire="))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| crate::epoch_secs() + 6 * 3600)
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
        let size = f.filesize.or(f.filesize_approx).unwrap_or(0);
        let user_agent = f
            .http_headers
            .as_ref()
            .and_then(|h| h.get("User-Agent").or_else(|| h.get("user-agent")))
            .cloned();

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
}
