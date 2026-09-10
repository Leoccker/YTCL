//! Reproducao: fila, historico e a ponte com o libmpv.
//!
//! O backend fica atras da trait `Backend` para que a fila possa ser testada
//! sem audio nenhum, e para deixar a porta aberta a um backend alternativo
//! (`symphonia` + `cpal`) se um dia o libmpv virar um problema.

pub mod mpv;
pub mod player;
pub mod proxy;
pub mod queue;
pub mod state;

pub use mpv::{BackendEvent, MpvBackend};
pub use player::{Player, PlayerSnapshot, QueueEntry};
pub use queue::Queue;
pub use state::{PlaybackState, PlayerEvent, RepeatMode};

use async_trait::async_trait;

/// O minimo que um backend de audio precisa saber fazer.
#[async_trait]
pub trait Backend: Send + Sync + 'static {
    async fn load(
        &self,
        stream: &ytcl_core::stream::ResolvedStream,
        gain_db: Option<f64>,
    ) -> anyhow::Result<()>;
    /// Enfileira a proxima faixa sem interromper a atual — e isso que
    /// produz o gapless de verdade.
    async fn append(
        &self,
        stream: &ytcl_core::stream::ResolvedStream,
        gain_db: Option<f64>,
    ) -> anyhow::Result<()>;
    async fn play(&self) -> anyhow::Result<()>;
    async fn pause(&self) -> anyhow::Result<()>;
    async fn seek(&self, secs: f64) -> anyhow::Result<()>;
    async fn set_volume(&self, level: f64) -> anyhow::Result<()>;
    async fn stop(&self) -> anyhow::Result<()>;
}
