use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::Serialize;
use ytcl_core::artwork::Fetcher;
use ytcl_core::auth::{Account, AuthStore, KeyringStore};
use ytcl_core::cache::Cache;
use ytcl_core::config::{Config, Paths};
use ytcl_core::innertube::InnerTube;

/// Quantas capas baixamos ao mesmo tempo. Uma grade cheia pede dezenas de
/// uma vez; sem teto, abriria dezenas de conexoes e nenhuma terminaria antes.
const ART_CONCURRENCY: usize = 8;

/// TTLs por tipo de conteudo. Busca envelhece rapido porque o ranking do
/// YouTube muda; album e artista sao praticamente estaticos.
pub const TTL_SEARCH: Duration = Duration::from_secs(15 * 60);
pub const TTL_DETAIL: Duration = Duration::from_secs(24 * 60 * 60);

/// Situacao da sessao, do ponto de vista da UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionState {
    /// Nenhuma conta conectada.
    LoggedOut,
    /// Conta conectada e cookie valido.
    Active { account: Account },
    /// Ha uma conta, mas o YouTube rejeitou o cookie. A UI mostra o banner
    /// de reconexao sem interromper o que estiver tocando.
    Expired { account: Account },
}

/// Estado compartilhado por todos os comandos.
///
/// Nao guardamos um runtime tokio proprio: o `tauri::async_runtime` ja e um
/// tokio multi-thread, e todo comando `async` roda nele.
pub struct AppState {
    pub paths: Paths,
    pub cache: Cache,
    pub artwork: Fetcher,
    /// Tipo concreto, nao `dyn MetadataSource`: os comandos de biblioteca
    /// precisam de `set_cookie`, que nao esta na trait.
    pub innertube: Arc<InnerTube>,
    secrets: KeyringStore,
    session: RwLock<SessionState>,
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
            secrets: KeyringStore,
            session: RwLock::new(SessionState::LoggedOut),
            config: RwLock::new(config),
            paths,
        })
    }

    pub fn auth(&self) -> AuthStore<'_> {
        AuthStore::new(&self.secrets)
    }

    pub fn session(&self) -> SessionState {
        self.session.read().clone()
    }

    pub fn set_session(&self, s: SessionState) {
        *self.session.write() = s;
    }

    /// Marca a sessao atual como expirada — chamado quando um comando de
    /// biblioteca leva 401/403. Nao faz nada se ja nao havia conta ativa.
    pub fn mark_expired(&self) {
        let mut s = self.session.write();
        if let SessionState::Active { account } = &*s {
            *s = SessionState::Expired { account: account.clone() };
        }
    }

    /// Le o cookie da conta ativa (config) e o injeta no InnerTube. Chamado
    /// na abertura e ao trocar de conta.
    pub async fn hydrate_session(&self) {
        let Some(id) = self.config().last_account_id else {
            self.set_session(SessionState::LoggedOut);
            return;
        };

        let account = match self.auth().accounts() {
            Ok(list) => list.into_iter().find(|a| a.id == id),
            Err(e) => {
                tracing::warn!("nao consegui ler as contas: {e}");
                None
            }
        };

        let Some(account) = account else {
            // A conta sumiu do cofre; limpa o ponteiro.
            self.set_active_account(None);
            self.set_session(SessionState::LoggedOut);
            return;
        };

        let cookie = match self.auth().cookie(&id) {
            Ok(Some(c)) => c,
            _ => {
                self.set_session(SessionState::Expired { account });
                return;
            }
        };

        match self.innertube.set_cookie(&cookie).await {
            Ok(()) => self.set_session(SessionState::Active { account }),
            Err(e) => {
                tracing::info!("cookie da conta ativa nao vale mais: {e}");
                self.innertube.clear_cookie().await;
                self.set_session(SessionState::Expired { account });
            }
        }
    }

    pub fn config(&self) -> Config {
        self.config.read().clone()
    }

    pub fn set_active_account(&self, id: Option<String>) {
        let mut cfg = self.config.write();
        cfg.last_account_id = id;
        if let Err(e) = cfg.save(&self.paths.config_file) {
            tracing::warn!("nao consegui salvar a config: {e}");
        }
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
