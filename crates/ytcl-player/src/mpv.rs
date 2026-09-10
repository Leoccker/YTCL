//! Backend de áudio sobre o libmpv.
//!
//! Modo somente-áudio (`vid=no`): o mpv cuida de streaming HTTP com Range,
//! buffer, seek e — o que mais importa — transição **gapless** entre faixas.
//! Reimplementar isso em Rust puro era o motivo de não usar `symphonia` aqui
//! (ver `docs/plano.md`).
//!
//! O `Mpv` é `Send + Sync` e a API de cliente do libmpv é thread-safe, então
//! o handle é compartilhado entre o lado de controle (comandos vindos de
//! `async fn`) e a thread dedicada do loop de eventos.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use libmpv2::events::{Event, PropertyData};
use libmpv2::{mpv_end_file_reason, Format, Mpv};
use tokio::sync::mpsc;

use crate::Backend;

/// Eventos crus do backend. O `Player` (camada de cima) traduz isto para
/// `PlayerEvent` e decide o que a fila faz.
#[derive(Debug, Clone)]
pub enum BackendEvent {
    Playing,
    Paused,
    /// mpv está ocioso esperando dados (buffer vazio).
    Buffering,
    /// Emitido a ~4 Hz. A UI interpola entre um evento e o próximo.
    Position { secs: f64, duration_secs: f64 },
    /// A faixa atual terminou. `natural` = chegou ao fim sozinha (avançar a
    /// fila); caso contrário foi erro/stop (não avançar).
    Ended { natural: bool },
    /// Erro de reprodução — quase sempre URL de stream expirada (403). O
    /// `Player` re-resolve e retoma da mesma posição.
    Error { message: String },
}

pub struct MpvBackend {
    mpv: Arc<Mpv>,
}

impl MpvBackend {
    pub fn new(events: mpsc::UnboundedSender<BackendEvent>) -> anyhow::Result<Self> {
        // libmpv se recusa a inicializar (mpv_create devolve NULL) se o
        // LC_NUMERIC do processo não for "C" — e o GTK, que o Tauri inicia
        // antes, adota o locale do usuário (pt_BR usa vírgula decimal). Fixar
        // só o LC_NUMERIC logo antes de criar o mpv é o remédio documentado;
        // não afeta formatação de data/moeda.
        #[cfg(unix)]
        unsafe {
            libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
        }

        let mpv = Mpv::with_initializer(|init| {
            // Sem vídeo: nada de decodificar frames nem abrir janela.
            init.set_property("vid", "no")?;
            init.set_property("audio-display", "no")?;
            init.set_property("vo", "null")?;
            // Buffer de rede generoso — evita engasgo em conexão irregular.
            init.set_property("cache", "yes")?;
            init.set_property("cache-secs", 120i64)?;
            init.set_property("demuxer-max-bytes", "64MiB")?;
            init.set_property("demuxer-readahead-secs", 20i64)?;
            // Gapless de verdade + pré-carrega a próxima da playlist do mpv.
            init.set_property("gapless-audio", "yes")?;
            init.set_property("prefetch-playlist", "yes")?;
            // Sem terminal, sem config do usuário interferindo.
            init.set_property("terminal", "no")?;
            init.set_property("config", "no")?;
            Ok(())
        })
        .map_err(|e| anyhow::anyhow!("iniciando o mpv: {e}"))?;

        let mpv = Arc::new(mpv);

        let loop_mpv = mpv.clone();
        std::thread::Builder::new()
            .name("ytcl-mpv-events".into())
            .spawn(move || event_loop(&loop_mpv, events))
            .map_err(|e| anyhow::anyhow!("thread do mpv: {e}"))?;

        Ok(Self { mpv })
    }

    fn set_gain(&self, gain_db: Option<f64>) {
        // YouTube devolve `loudnessDb`: quão mais alta a faixa está em relação
        // ao alvo de normalização. Para nivelar, atenua-se por esse valor.
        // TODO(fase 3): conferir o sinal contra faixas reais (uma alta e uma
        // baixa devem sair no mesmo volume percebido).
        match gain_db {
            Some(db) => {
                let _ = self
                    .mpv
                    .set_property("af", format!("volume=volume={:.2}dB", -db));
            }
            None => {
                let _ = self.mpv.set_property("af", "");
            }
        }
    }
}

/// Traduz `anyhow::Result` de um comando do mpv que não deve derrubar nada.
fn cmd(r: libmpv2::Result<()>, what: &str) -> anyhow::Result<()> {
    r.map_err(|e| anyhow::anyhow!("mpv {what}: {e}"))
}

#[async_trait]
impl Backend for MpvBackend {
    async fn load(&self, url: &str, gain_db: Option<f64>) -> anyhow::Result<()> {
        self.set_gain(gain_db);
        cmd(self.mpv.command("loadfile", &[url, "replace"]), "loadfile replace")
    }

    async fn append(&self, url: &str, gain_db: Option<f64>) -> anyhow::Result<()> {
        // O gain da próxima faixa é aplicado quando ela vira a atual (o mpv
        // não tem `af` por item de playlist). Por ora, `append` só enfileira.
        let _ = gain_db;
        cmd(self.mpv.command("loadfile", &[url, "append"]), "loadfile append")
    }

    async fn play(&self) -> anyhow::Result<()> {
        cmd(self.mpv.set_property("pause", false), "unpause")
    }

    async fn pause(&self) -> anyhow::Result<()> {
        cmd(self.mpv.set_property("pause", true), "pause")
    }

    async fn seek(&self, secs: f64) -> anyhow::Result<()> {
        cmd(
            self.mpv.command("seek", &[&format!("{secs:.3}"), "absolute"]),
            "seek",
        )
    }

    async fn set_volume(&self, level: f64) -> anyhow::Result<()> {
        let pct = (level.clamp(0.0, 1.0) * 100.0).round() as i64;
        cmd(self.mpv.set_property("volume", pct), "volume")
    }

    async fn stop(&self) -> anyhow::Result<()> {
        cmd(self.mpv.command("stop", &[]), "stop")
    }
}

/// Loop da thread de eventos. Sai quando o mpv desliga (o `MpvBackend` foi
/// dropado e o último handle sumiu).
fn event_loop(mpv: &Mpv, tx: mpsc::UnboundedSender<BackendEvent>) {
    let client = // create_client(Some(name)) tem um use-after-free no libmpv2 6.0.0
    // (CString temporária). O nome é cosmético e nem é lido — usamos None.
    match mpv.create_client(None) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("cliente de eventos do mpv: {e}");
            return;
        }
    };

    for (name, fmt, id) in [
        ("pause", Format::Flag, 1u64),
        ("core-idle", Format::Flag, 2),
        ("duration", Format::Double, 3),
    ] {
        if let Err(e) = client.observe_property(name, fmt, id) {
            tracing::warn!("observe {name}: {e}");
        }
    }

    let mut duration = 0.0f64;
    let mut last_pos = Instant::now();

    loop {
        match client.wait_event(0.2) {
            Some(Ok(event)) => match event {
                Event::Shutdown => break,

                Event::FileLoaded => {
                    if let Ok(d) = mpv.get_property::<f64>("duration") {
                        duration = d;
                    }
                }

                Event::EndFile(reason) => {
                    let natural = reason == mpv_end_file_reason::Eof;
                    if reason == mpv_end_file_reason::Error {
                        let msg = mpv
                            .get_property::<String>("error-string")
                            .unwrap_or_else(|_| "erro de reprodução".into());
                        let _ = tx.send(BackendEvent::Error { message: msg });
                    } else {
                        let _ = tx.send(BackendEvent::Ended { natural });
                    }
                }

                Event::PlaybackRestart => {
                    let _ = tx.send(BackendEvent::Playing);
                }

                Event::PropertyChange { name: "pause", change: PropertyData::Flag(p), .. } => {
                    let _ = tx.send(if p { BackendEvent::Paused } else { BackendEvent::Playing });
                }
                Event::PropertyChange { name: "core-idle", change: PropertyData::Flag(idle), .. } => {
                    if idle {
                        let _ = tx.send(BackendEvent::Buffering);
                    }
                }
                Event::PropertyChange { name: "duration", change: PropertyData::Double(d), .. } => {
                    duration = d;
                }
                _ => {}
            },
            Some(Err(e)) => tracing::trace!("evento do mpv com erro: {e}"),
            None => {} // timeout — cai para o tick de posição
        }

        if last_pos.elapsed() >= Duration::from_millis(250) {
            last_pos = Instant::now();
            if let Ok(pos) = mpv.get_property::<f64>("time-pos") {
                let _ = tx.send(BackendEvent::Position { secs: pos, duration_secs: duration });
            }
        }

        // Se o canal fechou (Player dropado), não há mais o que fazer.
        if tx.is_closed() {
            break;
        }
    }
}
