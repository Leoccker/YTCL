use serde::Serialize;
use ytcl_core::model::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackState {
    Idle,
    Buffering,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    Off,
    All,
    One,
}

/// Eventos que sobem do player para a UI.
///
/// `Position` e emitido a ~4 Hz de proposito: a barra de progresso interpola
/// em CSS entre os eventos. Emitir a 60 Hz satura o canal sem ganho visual.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PlayerEvent {
    State { state: PlaybackState },
    Position { secs: f64, duration_secs: f64 },
    TrackChanged { track: Box<Track> },
    QueueChanged,
    Volume { level: f64 },
    /// Falha recuperavel: a UI mostra um aviso discreto, sem modal.
    Error { message: String },
}
