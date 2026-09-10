//! Cache em disco de metadados e do mapa hash -> URL das capas.
//!
//! O que faz a navegacao parecer instantanea nao e a rede ser rapida: e a UI
//! desenhar do cache imediatamente e revalidar em segundo plano. Por isso o
//! `get` distingue Fresh de Stale em vez de so devolver Option.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{CoreError, Result};

/// Resultado de uma leitura do cache.
pub enum Entry<T> {
    /// Dentro do TTL: pode usar e nao precisa buscar de novo.
    Fresh(T),
    /// Vencido: desenhe agora e revalide em segundo plano.
    Stale(T),
}

impl<T> Entry<T> {
    pub fn into_inner(self) -> T {
        match self {
            Entry::Fresh(v) | Entry::Stale(v) => v,
        }
    }

    pub fn is_stale(&self) -> bool {
        matches!(self, Entry::Stale(_))
    }
}

pub struct Cache {
    conn: Mutex<Connection>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| CoreError::Other(format!("abrindo o cache {}: {e}", path.display())))?;
        Self::from_conn(conn)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| CoreError::Other(format!("cache em memoria: {e}")))?;
        Self::from_conn(conn)
    }

    fn from_conn(conn: Connection) -> Result<Self> {
        // WAL: leituras nao bloqueiam a escrita da revalidacao em background.
        // `synchronous = NORMAL` e seguro com WAL e evita um fsync por commit,
        // que num cache descartavel seria custo sem beneficio.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS kv (
                 key        TEXT PRIMARY KEY,
                 value      TEXT NOT NULL,
                 fetched_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS art (
                 hash TEXT PRIMARY KEY,
                 url  TEXT NOT NULL
             );",
        )
        .map_err(|e| CoreError::Other(format!("criando o schema do cache: {e}")))?;

        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Le uma entrada. Devolve `None` so quando nao ha nada gravado — uma
    /// entrada vencida volta como `Stale` para a UI poder desenhar na hora.
    pub fn get<T: DeserializeOwned>(&self, key: &str, ttl: Duration) -> Option<Entry<T>> {
        let row: Option<(String, i64)> = {
            let conn = self.conn.lock();
            conn.query_row(
                "SELECT value, fetched_at FROM kv WHERE key = ?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten()
        };

        let (json, fetched_at) = row?;
        let value: T = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(e) => {
                // Schema mudou entre versoes do app: trata como ausente em vez
                // de propagar um erro que o usuario nao pode resolver.
                tracing::debug!("entrada de cache ilegivel em {key}: {e}");
                self.forget(key);
                return None;
            }
        };

        let idade = now_secs().saturating_sub(fetched_at);
        Some(if idade >= 0 && (idade as u64) < ttl.as_secs() {
            Entry::Fresh(value)
        } else {
            Entry::Stale(value)
        })
    }

    pub fn put<T: Serialize>(&self, key: &str, value: &T) {
        let Ok(json) = serde_json::to_string(value) else {
            tracing::warn!("nao consegui serializar a entrada {key}");
            return;
        };
        let conn = self.conn.lock();
        if let Err(e) = conn.execute(
            "INSERT INTO kv (key, value, fetched_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, fetched_at = ?3",
            rusqlite::params![key, json, now_secs()],
        ) {
            // Cache e otimizacao: falhar ao gravar nao pode derrubar a acao.
            tracing::warn!("nao consegui gravar {key} no cache: {e}");
        }
    }

    pub fn forget(&self, key: &str) {
        let conn = self.conn.lock();
        let _ = conn.execute("DELETE FROM kv WHERE key = ?1", [key]);
    }

    /// Guarda de onde veio uma capa.
    ///
    /// O handler do `ytmart://` so recebe o hash na URL; e este mapa que
    /// permite baixar a imagem sob demanda no primeiro acesso, sem que o
    /// frontend precise orquestrar download nenhum.
    pub fn remember_art(&self, hash: &str, url: &str) {
        let conn = self.conn.lock();
        let _ = conn.execute(
            "INSERT INTO art (hash, url) VALUES (?1, ?2) ON CONFLICT(hash) DO NOTHING",
            [hash, url],
        );
    }

    pub fn art_url(&self, hash: &str) -> Option<String> {
        let conn = self.conn.lock();
        conn.query_row("SELECT url FROM art WHERE hash = ?1", [hash], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Dado {
        n: u32,
    }

    #[test]
    fn ausente_e_none() {
        let c = Cache::in_memory().unwrap();
        assert!(c.get::<Dado>("nada", Duration::from_secs(60)).is_none());
    }

    #[test]
    fn grava_e_le_como_fresh() {
        let c = Cache::in_memory().unwrap();
        c.put("k", &Dado { n: 7 });
        let e = c.get::<Dado>("k", Duration::from_secs(60)).expect("gravado");
        assert!(!e.is_stale());
        assert_eq!(e.into_inner(), Dado { n: 7 });
    }

    #[test]
    fn ttl_zero_vence_na_hora_mas_devolve_o_valor() {
        let c = Cache::in_memory().unwrap();
        c.put("k", &Dado { n: 7 });
        let e = c.get::<Dado>("k", Duration::ZERO).expect("gravado");
        assert!(e.is_stale(), "com TTL zero a entrada tem que vir stale");
        // O ponto do stale-while-revalidate: o valor continua utilizavel.
        assert_eq!(e.into_inner(), Dado { n: 7 });
    }

    #[test]
    fn valor_ilegivel_e_descartado_em_vez_de_estourar() {
        let c = Cache::in_memory().unwrap();
        c.put("k", &"uma string");
        // Tipo incompativel com o que foi gravado.
        assert!(c.get::<Dado>("k", Duration::from_secs(60)).is_none());
        // E a entrada ruim saiu do caminho.
        assert!(c.get::<String>("k", Duration::from_secs(60)).is_none());
    }

    #[test]
    fn mapa_de_capas_ida_e_volta() {
        let c = Cache::in_memory().unwrap();
        assert!(c.art_url("abc").is_none());
        c.remember_art("abc", "https://exemplo/capa.jpg");
        assert_eq!(c.art_url("abc").as_deref(), Some("https://exemplo/capa.jpg"));
    }
}
