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
    /// User-Agent que o player HTTP precisa mandar. As URLs do cliente iOS
    /// são travadas por UA.
    pub user_agent: Option<String>,
    /// Tamanho total em bytes, do `clen` do YouTube. O proxy precisa dele
    /// para responder `size` ao mpv e limitar o último Range.
    pub size: u64,
}

impl ResolvedStream {
    /// Margem de seguranca: tratamos como expirado antes da hora para nunca
    /// entregar ao mpv uma URL que vai morrer no meio da faixa.
    pub fn is_fresh(&self, now_epoch: u64, margin_secs: u64) -> bool {
        self.expires_at > now_epoch.saturating_add(margin_secs)
    }
}

/// Uma trilha de áudio candidata — o mínimo que a escolha precisa saber.
/// O adaptador do rustypipe converte `AudioStream` para isto.
#[derive(Debug, Clone)]
pub struct AudioTrack {
    pub url: String,
    pub itag: u32,
    pub codec: AudioCodec,
    pub bitrate: u32,
    pub size: u64,
    pub loudness_db: Option<f64>,
    /// Trilha com DRM não toca — o resolvedor descarta antes de chegar aqui,
    /// mas o campo existe para o teste cobrir o caso.
    pub has_drm: bool,
}

/// Escolhe a melhor trilha só de áudio.
///
/// Regra: descarta DRM; se `prefer_opus`, tenta Opus e só cai para AAC se não
/// houver Opus; dentro do codec escolhido, pega o maior bitrate. Sem
/// `prefer_opus`, escolhe o maior bitrate entre todos.
///
/// Por que Opus primeiro: o itag 251 do YouTube Music é a melhor qualidade
/// disponível e o libmpv decodifica nativamente. AAC (itag 140) é o piso.
pub fn pick_track(tracks: &[AudioTrack], prefer_opus: bool) -> Option<&AudioTrack> {
    let playable = || tracks.iter().filter(|t| !t.has_drm);

    if prefer_opus {
        if let Some(best_opus) = playable()
            .filter(|t| t.codec == AudioCodec::Opus)
            .max_by_key(|t| t.bitrate)
        {
            return Some(best_opus);
        }
    }
    playable().max_by_key(|t| t.bitrate)
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
            user_agent: None,
            size: 4_000_000,
        }
    }

fn track(itag: u32, codec: AudioCodec, bitrate: u32, drm: bool) -> AudioTrack {
        AudioTrack {
            url: format!("https://example.com/{itag}"),
            itag,
            codec,
            bitrate,
            size: 1_000_000,
            loudness_db: None,
            has_drm: drm,
        }
    }

    #[test]
    fn prefere_opus_mesmo_com_aac_de_bitrate_maior() {
        let tracks = [
            track(140, AudioCodec::Aac, 256_000, false),
            track(251, AudioCodec::Opus, 160_000, false),
        ];
        let chosen = pick_track(&tracks, true).unwrap();
        assert_eq!(chosen.itag, 251);
    }

    #[test]
    fn sem_opus_cai_para_aac() {
        let tracks = [track(140, AudioCodec::Aac, 128_000, false)];
        assert_eq!(pick_track(&tracks, true).unwrap().itag, 140);
    }

    #[test]
    fn sem_prefer_opus_pega_o_maior_bitrate() {
        let tracks = [
            track(140, AudioCodec::Aac, 256_000, false),
            track(251, AudioCodec::Opus, 160_000, false),
        ];
        assert_eq!(pick_track(&tracks, false).unwrap().itag, 140);
    }

    #[test]
    fn descarta_drm() {
        let tracks = [
            track(251, AudioCodec::Opus, 320_000, true),
            track(140, AudioCodec::Aac, 128_000, false),
        ];
        assert_eq!(pick_track(&tracks, true).unwrap().itag, 140);
    }

    #[test]
    fn tudo_drm_nao_devolve_nada() {
        let tracks = [track(251, AudioCodec::Opus, 320_000, true)];
        assert!(pick_track(&tracks, true).is_none());
    }

    #[test]
    fn lista_vazia_nao_devolve_nada() {
        assert!(pick_track(&[], true).is_none());
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
