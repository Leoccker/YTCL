use parking_lot::RwLock;
use ytm_core::config::{Config, Paths};

/// Estado compartilhado por todos os comandos.
///
/// Nao guardamos um runtime tokio proprio: o `tauri::async_runtime` ja e um
/// tokio multi-thread, e todo comando `async` roda nele. Dois runtimes no
/// mesmo processo so gerariam threads ociosas e confusao.
pub struct AppState {
    pub paths: Paths,
    config: RwLock<Config>,
}

impl AppState {
    pub fn new(paths: Paths) -> Self {
        let config = Config::load(&paths.config_file);
        Self { paths, config: RwLock::new(config) }
    }

    pub fn config(&self) -> Config {
        self.config.read().clone()
    }

    /// Atualiza em memoria e persiste. Falha ao gravar nao derruba o app —
    /// o usuario perde a preferencia, nao a sessao.
    pub fn update_config(&self, next: Config) {
        *self.config.write() = next;
        let cfg = self.config.read().clone();
        if let Err(e) = cfg.save(&self.paths.config_file) {
            tracing::warn!("nao consegui salvar a config: {e}");
        }
    }
}
