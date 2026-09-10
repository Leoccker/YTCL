//! Orquestrador: junta a fila, o backend do mpv e o resolvedor de stream.
//!
//! Regras que ele implementa:
//! - **gapless por pré-resolução:** faltando ~20 s, resolve a próxima faixa e
//!   faz `append` no mpv, que emenda sem silêncio.
//! - **URL expirada:** o mpv reporta erro de rede → re-resolve a faixa atual
//!   e retoma da mesma posição, sem o usuário perceber.
//! - **avanço automático:** faixa termina → fila avança → toca a próxima.
//!
//! Disciplina de lock: o `parking_lot::Mutex` de `Inner` nunca é segurado
//! através de um `.await`. Cada método lê o que precisa, solta o lock, e só
//! então chama a rede ou o mpv.

use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use ytcl_core::model::Track;
use ytcl_core::stream::{ResolvedStream, StreamResolver};

use crate::queue::Queue;
use crate::state::{PlaybackState, PlayerEvent, RepeatMode};
use crate::{Backend, BackendEvent};

/// Faltando isto (em segundos) para o fim, pré-resolve a próxima.
const PREFETCH_LEAD_SECS: f64 = 20.0;

struct Inner {
    queue: Queue,
    state: PlaybackState,
    volume: f64,
    normalize: bool,
    /// Stream da faixa que está tocando (para re-resolver em caso de 403).
    current_stream: Option<ResolvedStream>,
    /// `(videoId, stream)` da próxima faixa, já resolvida e `append`ada no mpv.
    prefetched: Option<(String, ResolvedStream)>,
    last_pos: f64,
    last_duration: f64,
}

pub struct Player {
    backend: Arc<dyn Backend>,
    resolver: Arc<dyn StreamResolver>,
    inner: Arc<Mutex<Inner>>,
    to_ui: mpsc::UnboundedSender<PlayerEvent>,
}

impl Player {
    /// Cria o player e sobe a task que consome os eventos do backend.
    pub fn new(
        backend: Arc<dyn Backend>,
        resolver: Arc<dyn StreamResolver>,
        backend_events: mpsc::UnboundedReceiver<BackendEvent>,
        to_ui: mpsc::UnboundedSender<PlayerEvent>,
    ) -> Arc<Self> {
        let player = Arc::new(Self {
            backend,
            resolver,
            inner: Arc::new(Mutex::new(Inner {
                queue: Queue::default(),
                state: PlaybackState::Idle,
                volume: 0.8,
                normalize: true,
                current_stream: None,
                prefetched: None,
                last_pos: 0.0,
                last_duration: 0.0,
            })),
            to_ui,
        });

        let p = player.clone();
        tokio::spawn(async move { p.consume_backend_events(backend_events).await });
        player
    }

    fn emit(&self, ev: PlayerEvent) {
        let _ = self.to_ui.send(ev);
    }

    fn gain_for(&self, stream: &ResolvedStream) -> Option<f64> {
        let normalize = self.inner.lock().normalize;
        normalize.then_some(stream.loudness_db).flatten()
    }

    // --- controle vindo dos comandos --------------------------------------

    /// Toca uma lista de faixas a partir de `start`.
    pub async fn play_tracks(&self, tracks: Vec<Track>, start: usize) -> anyhow::Result<()> {
        tracing::info!("play_tracks: {} faixas, start={start}", tracks.len());
        {
            let mut inner = self.inner.lock();
            inner.queue.set(tracks, start);
            inner.prefetched = None;
        }
        // `loadfile replace` no load_current já substitui o que estava tocando;
        // um `stop` antes só criava uma janela de corrida.
        self.load_current(true).await
    }

    pub async fn toggle_pause(&self) -> anyhow::Result<()> {
        let playing = matches!(self.inner.lock().state, PlaybackState::Playing);
        if playing {
            self.backend.pause().await
        } else {
            self.backend.play().await
        }
    }

    pub async fn next(&self) -> anyhow::Result<()> {
        let advanced = {
            let mut inner = self.inner.lock();
            inner.prefetched = None;
            inner.queue.advance().map(|t| t.id.clone())
        };
        match advanced {
            Some(_) => self.load_current(true).await,
            None => {
                self.backend.stop().await.ok();
                self.set_state(PlaybackState::Idle);
                Ok(())
            }
        }
    }

    pub async fn prev(&self) -> anyhow::Result<()> {
        // Convenção comum: nos primeiros 3 s volta a faixa, senão reinicia.
        let restart = self.inner.lock().last_pos > 3.0;
        if restart {
            return self.backend.seek(0.0).await;
        }
        {
            let mut inner = self.inner.lock();
            inner.prefetched = None;
            inner.queue.go_back();
        }
        self.load_current(true).await
    }

    pub async fn seek(&self, secs: f64) -> anyhow::Result<()> {
        self.backend.seek(secs.max(0.0)).await
    }

    pub async fn set_volume(&self, level: f64) -> anyhow::Result<()> {
        self.inner.lock().volume = level.clamp(0.0, 1.0);
        self.backend.set_volume(level).await?;
        self.emit(PlayerEvent::Volume { level });
        Ok(())
    }

    pub fn set_repeat(&self, mode: RepeatMode) {
        self.inner.lock().queue.set_repeat(mode);
        self.emit(PlayerEvent::QueueChanged);
    }

    pub fn set_shuffle(&self, on: bool) {
        {
            let mut inner = self.inner.lock();
            inner.queue.set_shuffle(on);
            inner.prefetched = None;
        }
        self.emit(PlayerEvent::QueueChanged);
    }

    pub fn set_normalize(&self, on: bool) {
        self.inner.lock().normalize = on;
    }

    pub fn play_next(&self, track: Track) {
        self.inner.lock().queue.play_next(track);
        self.emit(PlayerEvent::QueueChanged);
    }

    pub fn enqueue(&self, track: Track) {
        self.inner.lock().queue.enqueue(track);
        self.emit(PlayerEvent::QueueChanged);
    }

    pub fn move_in_queue(&self, from: usize, to: usize) {
        self.inner.lock().queue.move_item(from, to);
        self.emit(PlayerEvent::QueueChanged);
    }

    pub async fn jump_in_queue(&self, order_index: usize) -> anyhow::Result<()> {
        {
            let mut inner = self.inner.lock();
            inner.prefetched = None;
            inner.queue.jump_to(order_index);
        }
        self.load_current(true).await
    }

    /// Snapshot para os comandos de leitura.
    pub fn snapshot(&self) -> PlayerSnapshot {
        let inner = self.inner.lock();
        PlayerSnapshot {
            state: inner.state,
            volume: inner.volume,
            position: inner.last_pos,
            duration: inner.last_duration,
            repeat: inner.queue.repeat(),
            shuffled: inner.queue.shuffled(),
            current: inner.queue.current().cloned(),
            queue: inner
                .queue
                .view()
                .into_iter()
                .map(|(t, is_current)| QueueEntry { track: t.clone(), is_current })
                .collect(),
        }
    }

    // --- interno --------------------------------------------------------

    fn set_state(&self, state: PlaybackState) {
        {
            let mut inner = self.inner.lock();
            if inner.state == state {
                return;
            }
            inner.state = state;
        }
        self.emit(PlayerEvent::State { state });
    }

    /// Resolve a faixa atual e manda o mpv tocar.
    async fn load_current(&self, autoplay: bool) -> anyhow::Result<()> {
        let track = match self.inner.lock().queue.current().cloned() {
            Some(t) => t,
            None => return Ok(()),
        };

        self.set_state(PlaybackState::Buffering);
        self.emit(PlayerEvent::TrackChanged { track: Box::new(track.clone()) });

        tracing::info!("resolvendo stream de {}", track.id);
        let stream = match self.resolver.resolve(&track.id).await {
            Ok(s) => {
                tracing::info!("stream ok: {:?} {}kbps, expira em {}", s.codec, s.bitrate / 1000, s.expires_at);
                s
            }
            Err(e) => {
                tracing::error!("resolve falhou: {e}");
                self.emit(PlayerEvent::Error { message: format!("não consegui tocar: {e}") });
                self.set_state(PlaybackState::Idle);
                return Ok(());
            }
        };

        let gain = self.gain_for(&stream);
        self.backend.load(&stream, gain).await?;
        if autoplay {
            self.backend.play().await?;
        }
        self.inner.lock().current_stream = Some(stream);
        Ok(())
    }

    async fn consume_backend_events(
        self: Arc<Self>,
        mut rx: mpsc::UnboundedReceiver<BackendEvent>,
    ) {
        while let Some(ev) = rx.recv().await {
            tracing::debug!("backend event: {ev:?}");
            match ev {
                BackendEvent::Playing => self.set_state(PlaybackState::Playing),
                BackendEvent::Paused => self.set_state(PlaybackState::Paused),
                BackendEvent::Buffering => self.set_state(PlaybackState::Buffering),

                BackendEvent::Position { secs, duration_secs } => {
                    {
                        let mut inner = self.inner.lock();
                        inner.last_pos = secs;
                        inner.last_duration = duration_secs;
                    }
                    self.emit(PlayerEvent::Position { secs, duration_secs });
                    self.maybe_prefetch(secs, duration_secs).await;
                }

                BackendEvent::Ended { natural } => {
                    if natural {
                        self.on_track_finished().await;
                    }
                }

                BackendEvent::Error { message } => {
                    self.recover_from_error(&message).await;
                }
            }
        }
    }

    /// Faltando pouco para o fim, resolve a próxima e faz `append` no mpv.
    async fn maybe_prefetch(&self, pos: f64, duration: f64) {
        if duration <= 0.0 || duration - pos > PREFETCH_LEAD_SECS {
            return;
        }
        let next = {
            let inner = self.inner.lock();
            match inner.queue.peek_next() {
                Some(t) if inner.prefetched.as_ref().map(|(id, _)| id) != Some(&t.id) => {
                    t.id.clone()
                }
                _ => return,
            }
        };

        match self.resolver.resolve(&next).await {
            Ok(stream) => {
                let gain = self.gain_for(&stream);
                if self.backend.append(&stream, gain).await.is_ok() {
                    self.inner.lock().prefetched = Some((next, stream));
                }
            }
            Err(e) => tracing::debug!("pré-resolução da próxima falhou: {e}"),
        }
    }

    /// A faixa terminou sozinha: avança a fila.
    async fn on_track_finished(&self) {
        let (advanced, prefetched) = {
            let mut inner = self.inner.lock();
            let advanced = inner.queue.advance().map(|t| t.id.clone());
            let prefetched = inner.prefetched.take();
            (advanced, prefetched)
        };

        let Some(next_id) = advanced else {
            self.set_state(PlaybackState::Idle);
            self.emit(PlayerEvent::QueueChanged);
            return;
        };

        let track = self.inner.lock().queue.current().cloned();
        if let Some(track) = &track {
            self.emit(PlayerEvent::TrackChanged { track: Box::new(track.clone()) });
        }

        match prefetched {
            // O mpv já emendou nessa faixa via `append` — só sincronizar.
            Some((id, stream)) if id == next_id => {
                self.inner.lock().current_stream = Some(stream);
            }
            // Não deu tempo de pré-resolver: carrega agora.
            _ => {
                if let Err(e) = self.load_current(true).await {
                    self.emit(PlayerEvent::Error { message: e.to_string() });
                }
            }
        }
    }

    /// mpv reportou erro — quase sempre URL de stream expirada (403).
    async fn recover_from_error(&self, message: &str) {
        let (track, pos) = {
            let inner = self.inner.lock();
            (inner.queue.current().cloned(), inner.last_pos)
        };
        let Some(track) = track else {
            self.emit(PlayerEvent::Error { message: message.to_string() });
            return;
        };

        tracing::info!("re-resolvendo {} após erro do mpv: {message}", track.id);
        match self.resolver.resolve(&track.id).await {
            Ok(stream) => {
                let gain = self.gain_for(&stream);
                if self.backend.load(&stream, gain).await.is_ok() {
                    let _ = self.backend.seek(pos).await;
                    let _ = self.backend.play().await;
                    self.inner.lock().current_stream = Some(stream);
                    return;
                }
                self.emit(PlayerEvent::Error { message: "falha ao retomar a reprodução".into() });
            }
            Err(e) => {
                self.emit(PlayerEvent::Error {
                    message: format!("stream expirou e não consegui renovar: {e}"),
                });
                self.set_state(PlaybackState::Idle);
            }
        }
    }
}

// --- tipos para o IPC ----------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntry {
    pub track: Track,
    pub is_current: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub state: PlaybackState,
    pub volume: f64,
    pub position: f64,
    pub duration: f64,
    pub repeat: RepeatMode,
    pub shuffled: bool,
    pub current: Option<Track>,
    pub queue: Vec<QueueEntry>,
}
