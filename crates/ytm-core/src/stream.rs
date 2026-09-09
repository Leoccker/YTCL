//! Resolucao de URL de audio.
//!
//! Esta e a camada mais fragil do projeto: depende da decifragem de
//! assinatura e do parametro `n`, que o YouTube muda algumas vezes por ano.
//! Fica atras de uma trait justamente para que a quebra tenha um lugar so.

use async_trait::async_trait;

use crate::error::Result;

/// Codec da trilha escolhida. Preferimos Opus; AAC e o fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Opus,
    Aac,
}

#[derive(Debug, Clone)]
pub struct ResolvedStream {
    pub url: String,
    pub codec: AudioCodec,
    pub bitrate: u32,
    /// Momento em que a URL deixa de valer (epoch secs). Passando disso,
    /// o mpv recebe 403 e precisamos re-resolver.
    pub expires_at: u64,
    /// `loudnessDb` do proprio YouTube, para alimentar o ReplayGain do mpv.
    pub loudness_db: Option<f64>,
    pub duration_secs: Option<u32>,
}

impl ResolvedStream {
    /// Margem de seguranca: tratamos como expirado antes da hora para nunca
    /// entregar ao mpv uma URL que vai morrer no meio da faixa.
    pub fn is_fresh(&self, now_epoch: u64, margin_secs: u64) -> bool {
        self.expires_at > now_epoch.saturating_add(margin_secs)
    }
}

#[async_trait]
pub trait StreamResolver: Send + Sync + 'static {
    async fn resolve(&self, video_id: &str) -> Result<ResolvedStream>;
}

/// Gancho para o dia em que o YouTube passar a exigir PO token no cliente
/// YTM. Hoje o cliente YTM envia mas nao exige, e o cliente TV nao usa —
/// por isso o v1 nao implementa isso.
#[async_trait]
pub trait PoTokenProvider: Send + Sync + 'static {
    async fn token(&self, video_id: &str) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(expires_at: u64) -> ResolvedStream {
        ResolvedStream {
            url: "https://example.com/a".into(),
            codec: AudioCodec::Opus,
            bitrate: 128_000,
            expires_at,
            loudness_db: None,
            duration_secs: Some(200),
        }
    }

    #[test]
    fn frescor_respeita_a_margem() {
        let s = stream(1000);
        assert!(s.is_fresh(800, 60));
        // dentro da margem: ja tratamos como velho
        assert!(!s.is_fresh(960, 60));
        assert!(!s.is_fresh(1200, 60));
    }

    #[test]
    fn frescor_nao_estoura_no_overflow() {
        assert!(!stream(10).is_fresh(u64::MAX, 60));
    }
}
