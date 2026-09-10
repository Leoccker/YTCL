use std::path::{Path, PathBuf};

use directories_next::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

const QUALIFIER: &str = "dev";
const ORG: &str = "ytcl";
const APP: &str = "ytcl";

/// Onde o app guarda config, cache e capas.
#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub art_dir: PathBuf,
    pub db_file: PathBuf,
    pub config_file: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from(QUALIFIER, ORG, APP)
            .ok_or_else(|| CoreError::Other("nao foi possivel achar o home do usuario".into()))?;

        let config_dir = dirs.config_dir().to_path_buf();
        let cache_dir = dirs.cache_dir().to_path_buf();

        Ok(Self {
            art_dir: cache_dir.join("art"),
            db_file: cache_dir.join("ytcl.db"),
            config_file: config_dir.join("config.toml"),
            config_dir,
            cache_dir,
        })
    }

    pub fn ensure(&self) -> Result<()> {
        for d in [&self.config_dir, &self.cache_dir, &self.art_dir] {
            std::fs::create_dir_all(d)
                .map_err(|e| CoreError::Other(format!("criando {}: {e}", d.display())))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub volume: f64,
    /// Teto do cache de capas em disco. Passando disso, o mais antigo sai.
    pub art_cache_mb: u64,
    /// Preferir Opus (itag 251) quando disponivel; senao cai para AAC.
    pub prefer_opus: bool,
    /// Normalizacao de volume a partir do `loudnessDb` do proprio YouTube.
    pub normalize_volume: bool,
    pub last_account_id: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 0.8,
            art_cache_mb: 256,
            prefer_opus: true,
            normalize_volume: true,
            last_account_id: None,
        }
    }
}

impl Config {
    /// Config corrompida nao pode impedir o app de abrir: loga e usa o padrao.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => match toml::from_str(&s) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("config invalida ({e}), usando padroes");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let s = toml::to_string_pretty(self)
            .map_err(|e| CoreError::Other(format!("serializando config: {e}")))?;
        std::fs::write(path, s)
            .map_err(|e| CoreError::Other(format!("gravando {}: {e}", path.display())))
    }
}
