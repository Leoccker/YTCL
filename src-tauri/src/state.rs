use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use ytcl_core::artwork::Fetcher;
use ytcl_core::cache::Cache;
use ytcl_core::config::{Config, Paths};
use ytcl_core::innertube::InnerTube;
use ytcl_core::metadata::MetadataSource;

/// Quantas capas baixamos ao mesmo tempo. Uma grade cheia pede dezenas de
/// uma vez; sem teto, abriria dezenas de conexoes e nenhuma terminaria antes.
const ART_CONCURRENCY: usize = 8;

/// TTLs por tipo de conteudo. Busca envelhece rapido porque o ranking do
/// YouTube muda; album e artista sao praticamente estaticos.
pub const TTL_SEARCH: Duration = Duration::from_secs(15 * 60);
pub const TTL_DETAIL: Duration = Duration::from_secs(24 * 60 * 60);

/// Estado compartilhado por todos os comandos.
///
/// Nao guardamos um runtime tokio proprio: o `tauri::async_runtime` ja e um
/// tokio multi-thread, e todo comando `async` roda nele. Dois runtimes no
/// mesmo processo so gerariam threads ociosas e confusao.
pub struct AppState {
    pub paths: Paths,
    pub cache: Cache,
    pub artwork: Fetcher,
    pub innertube: Arc<dyn MetadataSource>,
    config: RwLock<Config>,
}

impl AppState {
    pub fn new(paths: Paths) -> anyhow::Result<Self> {
        let config = Config::load(&paths.config_file);
        let cache = Cache::open(&paths.db_file)?;
        let artwork = Fetcher::new(paths.art_dir.clone(), ART_CONCURRENCY);
        let innertube = InnerTube::new(&paths.cache_dir)?;

        Ok(Self {
            innertube: Arc::new(innertube),
            cache,
            artwork,
            config: RwLock::new(config),
            paths,
        })
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
