//! Nucleo do cliente: metadados, resolucao de stream, auth, cache e config.
//!
//! Nao depende do Tauri nem de nenhuma camada de UI — da para exercitar tudo
//! aqui por testes e por um binario de linha de comando.

pub mod artwork;
pub mod auth;
pub mod cache;
pub mod config;
pub mod error;
pub mod innertube;
pub mod metadata;
pub mod model;
pub mod stream;

pub use error::{CoreError, ErrorKind, ErrorPayload, Result};

/// Segundos desde a época Unix. Ponto único para não espalhar `SystemTime`
/// pelos módulos.
pub fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
