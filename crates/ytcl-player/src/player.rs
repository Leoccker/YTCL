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
use tokio::sync::{mpsc, Mutex as AsyncMutex};
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
    /// Identifica a seleção atual, inclusive ao selecionar novamente o mesmo vídeo.
    generation: u64,
    /// Invalida pré-resoluções quando a ordem/repetição da fila muda.
    queue_revision: u64,
    loading: bool,
    prefetching: Option<(u64, u64)>,
    last_pos: f64,
    last_duration: f64,
}

struct LoadRequest {
    generation: u64,
    track: Track,
    resume_at: Option<f64>,
}

pub struct Player {
    backend: Arc<dyn Backend>,
    resolver: Arc<dyn StreamResolver>,
    inner: Arc<Mutex<Inner>>,
    to_ui: mpsc::UnboundedSender<PlayerEvent>,
    /// Serializa mudanças da fila com comandos do backend, nunca com a rede.
    operations: AsyncMutex<()>,
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
                generation: 0,
                queue_revision: 0,
                loading: false,
                prefetching: None,
                last_pos: 0.0,
                last_duration: 0.0,
            })),
            to_ui,
            operations: AsyncMutex::new(()),
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
        self.change_current(|queue| queue.set(tracks, start)).await
    }

    pub async fn toggle_pause(&self) -> anyhow::Result<()> {
        let _operation = self.operations.lock().await;
        let playing = matches!(self.inner.lock().state, PlaybackState::Playing);
        if playing {
            self.backend.pause().await
        } else {
            self.backend.play().await
        }
    }

    pub async fn next(&self) -> anyhow::Result<()> {
        let request = {
            let _operation = self.operations.lock().await;
            let advanced = self.inner.lock().queue.advance().is_some();
            if !advanced {
                self.invalidate_current();
                self.backend.stop().await?;
                self.set_state(PlaybackState::Idle);
                self.emit(PlayerEvent::QueueChanged);
                return Ok(());
            }
            self.prepare_current(None).await?
        };
        self.resolve_current(request).await
    }

    pub async fn prev(&self) -> anyhow::Result<()> {
        let request = {
            let _operation = self.operations.lock().await;
            let restart = self.inner.lock().last_pos > 3.0;
            if restart {
                return self.backend.seek(0.0).await;
            }
            self.inner.lock().queue.go_back();
            self.prepare_current(None).await?
        };
        self.resolve_current(request).await
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

    pub async fn set_repeat(&self, mode: RepeatMode) -> anyhow::Result<()> {
        self.edit_queue(|queue| queue.set_repeat(mode)).await
    }

    pub async fn set_shuffle(&self, on: bool) -> anyhow::Result<()> {
        self.edit_queue(|queue| queue.set_shuffle(on)).await
    }

    pub fn set_normalize(&self, on: bool) {
        self.inner.lock().normalize = on;
    }

    pub async fn play_next(&self, track: Track) -> anyhow::Result<()> {
        self.edit_queue(|queue| queue.play_next(track)).await
    }

    pub async fn enqueue(&self, track: Track) -> anyhow::Result<()> {
        self.edit_queue(|queue| queue.enqueue(track)).await
    }

    pub async fn move_in_queue(&self, from: usize, to: usize) -> anyhow::Result<()> {
        self.edit_queue(|queue| queue.move_item(from, to)).await
    }

    pub async fn jump_in_queue(&self, order_index: usize) -> anyhow::Result<()> {
        self.change_current(|queue| {
            queue.jump_to(order_index);
        })
        .await
    }

    async fn edit_queue(&self, edit: impl FnOnce(&mut Queue)) -> anyhow::Result<()> {
        let _operation = self.operations.lock().await;
        // Primeiro remove a playlist real. Se falhar, não aplica uma ordem
        // que o backend não conseguiu acompanhar.
        self.backend.clear_next().await?;
        {
            let mut inner = self.inner.lock();
            inner.queue_revision = inner.queue_revision.wrapping_add(1);
            inner.prefetched = None;
            inner.prefetching = None;
            edit(&mut inner.queue);
        }
        self.emit(PlayerEvent::QueueChanged);
        Ok(())
    }

    async fn change_current(&self, edit: impl FnOnce(&mut Queue)) -> anyhow::Result<()> {
        let request = {
            let _operation = self.operations.lock().await;
            edit(&mut self.inner.lock().queue);
            self.prepare_current(None).await?
        };
        self.resolve_current(request).await
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
                .map(|(t, is_current)| QueueEntry {
                    track: t.clone(),
                    is_current,
                })
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

    /// Chamado com `operations` adquirido. Invalida também a pré-carga.
    fn invalidate_current(&self) {
        let mut inner = self.inner.lock();
        inner.generation = inner.generation.wrapping_add(1);
        inner.queue_revision = inner.queue_revision.wrapping_add(1);
        inner.prefetched = None;
        inner.prefetching = None;
        inner.current_stream = None;
        inner.loading = false;
    }

    /// Prepara uma seleção enquanto `operations` protege fila e backend.
    /// O pedido retorna antes da rede para que uma nova seleção possa vencê-lo.
    async fn prepare_current(&self, resume_at: Option<f64>) -> anyhow::Result<Option<LoadRequest>> {
        self.invalidate_current();
        let request = {
            let mut inner = self.inner.lock();
            inner.last_pos = resume_at.unwrap_or(0.0);
            inner.last_duration = 0.0;
            let request = inner.queue.current().cloned().map(|track| LoadRequest {
                generation: inner.generation,
                track,
                resume_at,
            });
            inner.loading = request.is_some();
            request
        };
        // Impede que a faixa antiga avance a playlist enquanto a nova resolve.
        if let Err(error) = self.backend.stop().await {
            self.inner.lock().loading = false;
            self.set_state(PlaybackState::Idle);
            return Err(error);
        }
        if let Some(request) = &request {
            self.set_state(PlaybackState::Buffering);
            self.emit(PlayerEvent::TrackChanged {
                track: Box::new(request.track.clone()),
            });
        } else {
            self.set_state(PlaybackState::Idle);
            self.emit(PlayerEvent::QueueChanged);
        }
        Ok(request)
    }

    async fn resolve_current(&self, request: Option<LoadRequest>) -> anyhow::Result<()> {
        let Some(request) = request else {
            return Ok(());
        };
        let resolved = self.resolver.resolve(&request.track.id).await;
        let _operation = self.operations.lock().await;
        if self.inner.lock().generation != request.generation {
            // Nem o sucesso nem o erro de um pedido antigo pode alterar a UI.
            return Ok(());
        }
        let stream = match resolved {
            Ok(stream) => stream,
            Err(error) => {
                self.inner.lock().loading = false;
                self.emit(PlayerEvent::Error {
                    message: format!("não consegui tocar: {error}"),
                });
                self.set_state(PlaybackState::Idle);
                return Ok(());
            }
        };
        let result = async {
            self.backend.load(&stream, self.gain_for(&stream)).await?;
            if let Some(pos) = request.resume_at {
                self.backend.seek(pos).await?;
            }
            self.backend.play().await
        }
        .await;
        self.inner.lock().loading = false;
        if result.is_ok() {
            self.inner.lock().current_stream = Some(stream);
        } else {
            self.set_state(PlaybackState::Idle);
        }
        result
    }

    async fn consume_backend_events(
        self: Arc<Self>,
        mut rx: mpsc::UnboundedReceiver<BackendEvent>,
    ) {
        while let Some(ev) = rx.recv().await {
            let operation = self.operations.lock().await;
            let generation = {
                let inner = self.inner.lock();
                if inner.loading || inner.current_stream.is_none() {
                    continue;
                }
                inner.generation
            };
            match ev {
                BackendEvent::Playing => self.set_state(PlaybackState::Playing),
                BackendEvent::Paused => self.set_state(PlaybackState::Paused),
                BackendEvent::Buffering => self.set_state(PlaybackState::Buffering),
                BackendEvent::Position {
                    secs,
                    duration_secs,
                } => {
                    {
                        let mut inner = self.inner.lock();
                        inner.last_pos = secs;
                        inner.last_duration = duration_secs;
                    }
                    self.emit(PlayerEvent::Position {
                        secs,
                        duration_secs,
                    });
                    drop(operation);
                    let player = self.clone();
                    // O consumidor continua recebendo eventos enquanto a URL
                    // resolve; o resultado leva a geração do evento original.
                    tokio::spawn(async move {
                        player.maybe_prefetch(generation, secs, duration_secs).await;
                    });
                }
                BackendEvent::Ended { natural: true } => {
                    drop(operation);
                    let player = self.clone();
                    tokio::spawn(async move { player.on_track_finished(generation).await });
                }
                BackendEvent::Ended { natural: false } => {}
                BackendEvent::Error { message } => {
                    drop(operation);
                    let player = self.clone();
                    tokio::spawn(
                        async move { player.recover_from_error(generation, &message).await },
                    );
                }
            }
        }
    }

    /// Faltando pouco para o fim, resolve a próxima e faz `append` no mpv.
    async fn maybe_prefetch(&self, expected_generation: u64, pos: f64, duration: f64) {
        if duration <= 0.0 || duration - pos > PREFETCH_LEAD_SECS {
            return;
        }
        let (generation, revision, next) = {
            let _operation = self.operations.lock().await;
            let mut inner = self.inner.lock();
            if inner.generation != expected_generation
                || inner.loading
                || inner.current_stream.is_none()
                || inner.prefetching.is_some()
            {
                return;
            }
            let next = match inner.queue.peek_next() {
                Some(track) if inner.prefetched.as_ref().map(|(id, _)| id) != Some(&track.id) => {
                    track.id.clone()
                }
                _ => return,
            };
            inner.prefetching = Some((inner.generation, inner.queue_revision));
            (inner.generation, inner.queue_revision, next)
        };

        let resolved = self.resolver.resolve(&next).await;
        let _operation = self.operations.lock().await;
        {
            let mut inner = self.inner.lock();
            if inner.prefetching == Some((generation, revision)) {
                inner.prefetching = None;
            }
            if inner.generation != generation
                || inner.queue_revision != revision
                || inner.loading
                || inner.queue.peek_next().map(|t| &t.id) != Some(&next)
                || inner.prefetched.is_some()
            {
                return;
            }
        }
        match resolved {
            Ok(stream) => {
                // Só existe uma próxima faixa no mpv, assim como em prefetched.
                let result = async {
                    self.backend.clear_next().await?;
                    self.backend.append(&stream, self.gain_for(&stream)).await
                }
                .await;
                match result {
                    Ok(()) => self.inner.lock().prefetched = Some((next, stream)),
                    Err(error) => tracing::debug!("pré-carga falhou: {error}"),
                }
            }
            Err(error) => tracing::debug!("pré-resolução da próxima falhou: {error}"),
        }
    }

    /// A faixa terminou sozinha: avança a fila.
    async fn on_track_finished(&self, generation: u64) {
        let request = {
            let _operation = self.operations.lock().await;
            let (advanced, prefetched) = {
                let mut inner = self.inner.lock();
                if inner.generation != generation || inner.loading || inner.current_stream.is_none()
                {
                    return;
                }
                let advanced = inner.queue.advance().cloned();
                let prefetched = inner.prefetched.take();
                (advanced, prefetched)
            };
            let Some(track) = advanced else {
                self.invalidate_current();
                let _ = self.backend.stop().await;
                self.set_state(PlaybackState::Idle);
                self.emit(PlayerEvent::QueueChanged);
                return;
            };
            if let Some((id, stream)) = prefetched {
                if id == track.id {
                    {
                        let mut inner = self.inner.lock();
                        inner.generation = inner.generation.wrapping_add(1);
                        inner.queue_revision = inner.queue_revision.wrapping_add(1);
                        inner.current_stream = Some(stream);
                        inner.prefetching = None;
                        inner.last_pos = 0.0;
                        inner.last_duration = 0.0;
                    }
                    self.emit(PlayerEvent::TrackChanged {
                        track: Box::new(track),
                    });
                    return;
                }
            }
            match self.prepare_current(None).await {
                Ok(request) => request,
                Err(error) => {
                    self.emit(PlayerEvent::Error {
                        message: error.to_string(),
                    });
                    return;
                }
            }
        };
        if let Err(error) = self.resolve_current(request).await {
            self.emit(PlayerEvent::Error {
                message: error.to_string(),
            });
        }
    }

    async fn recover_from_error(&self, generation: u64, message: &str) {
        let request = {
            let _operation = self.operations.lock().await;
            let pos = {
                let inner = self.inner.lock();
                if inner.generation != generation || inner.loading || inner.current_stream.is_none()
                {
                    return;
                }
                inner.last_pos
            };
            tracing::info!("renovando stream após erro do mpv: {message}");
            // A recuperação substitui a playlist; sua pré-carga também deve
            // ser invalidada para que seja resolvida e acrescentada novamente.
            match self.prepare_current(Some(pos)).await {
                Ok(request) => request,
                Err(error) => {
                    self.emit(PlayerEvent::Error {
                        message: error.to_string(),
                    });
                    return;
                }
            }
        };
        if let Err(error) = self.resolve_current(request).await {
            self.emit(PlayerEvent::Error {
                message: error.to_string(),
            });
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

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;
