//! Nucleo do cliente: metadados, resolucao de stream, cache e config.
//!
//! Nao depende do Tauri nem de nenhuma camada de UI — da para exercitar tudo
//! aqui por testes e por um binario de linha de comando.

pub mod artwork;
pub mod cache;
pub mod config;
pub mod error;
pub mod innertube;
pub mod metadata;
pub mod model;
pub mod stream;

pub use error::{CoreError, ErrorKind, ErrorPayload, Result};
