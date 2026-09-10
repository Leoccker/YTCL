//! Cache de capas em disco.
//!
//! Regra de ouro da performance: **capa nunca passa pelo IPC**. O Rust so
//! grava o arquivo e devolve um hash; o frontend usa `ytmart://<hash>` e o
//! webview decodifica fora da thread principal e cacheia sozinho.

use std::path::{Path, PathBuf};

/// Chave estavel para uma URL de capa.
///
/// As URLs do YouTube trazem o tamanho no proprio caminho (`=w544-h544-...`),
/// entao normalizamos para o tamanho pedido antes de hashear — assim a mesma
/// arte em dois tamanhos vira duas entradas, que e o que queremos.
pub fn hash_url(url: &str) -> String {
    blake3::hash(url.as_bytes()).to_hex()[..32].to_string()
}

/// Caminho no cache. Fica em subdiretorio de 2 chars para nao criar um
/// diretorio com dezenas de milhares de arquivos.
pub fn path_for(art_dir: &Path, hash: &str) -> PathBuf {
    art_dir.join(&hash[..2]).join(hash)
}

/// Aceita so o que a gente mesmo gera: 32 chars hex. Isso e a validacao que
/// impede path traversal no handler do protocolo `ytmart://`.
pub fn is_valid_hash(hash: &str) -> bool {
    hash.len() == 32 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Reescreve a URL de thumbnail do YouTube para o tamanho desejado.
/// As URLs terminam em `=w60-h60-l90-rj` ou similar; trocar isso evita
/// baixar 544x544 para exibir num quadrado de 48px.
pub fn sized_url(url: &str, size: u32) -> String {
    match url.rfind('=') {
        Some(i) if url[i..].starts_with("=w") => {
            format!("{}=w{size}-h{size}-l90-rj", &url[..i])
        }
        _ => url.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Download sob demanda
// ---------------------------------------------------------------------------

use tokio::sync::Semaphore;

use crate::error::{CoreError, Result};

/// Baixa capas e as guarda em disco.
///
/// Quem chama isto e o handler do `ytmart://`, no primeiro acesso a uma capa
/// que ainda nao esta em cache. O frontend nunca participa: ele so aponta um
/// `<img>` para o hash e o webview espera a resposta como esperaria de
/// qualquer imagem na rede.
pub struct Fetcher {
    art_dir: PathBuf,
    http: reqwest::Client,
    /// Teto de downloads simultaneos. Uma grade de albuns pede dezenas de
    /// capas ao mesmo tempo; sem limite, abriria dezenas de conexoes.
    permits: Semaphore,
}

impl Fetcher {
    pub fn new(art_dir: PathBuf, max_concurrent: usize) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self { art_dir, http, permits: Semaphore::new(max_concurrent) }
    }

    /// Devolve os bytes da capa, baixando-a se ainda nao estiver em disco.
    pub async fn ensure(&self, hash: &str, url: &str) -> Result<Vec<u8>> {
        let path = path_for(&self.art_dir, hash);

        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(bytes);
        }

        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| CoreError::Other("fila de download fechada".into()))?;

        // Outra requisicao pode ter baixado enquanto esperavamos a vaga.
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(bytes);
        }

        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(CoreError::Network(format!("capa: HTTP {}", resp.status())));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?
            .to_vec();

        self.write_atomic(&path, &bytes).await;
        Ok(bytes)
    }

    /// Grava em arquivo temporario e renomeia.
    ///
    /// Sem isso, um download interrompido deixaria um JPEG truncado no cache —
    /// e como a chave e o hash da URL, ele seria servido para sempre.
    async fn write_atomic(&self, path: &std::path::Path, bytes: &[u8]) {
        let Some(dir) = path.parent() else { return };
        if tokio::fs::create_dir_all(dir).await.is_err() {
            return;
        }
        let tmp = path.with_extension("part");
        if tokio::fs::write(&tmp, bytes).await.is_ok() {
            let _ = tokio::fs::rename(&tmp, path).await;
        }
    }
}

/// Apaga as capas mais antigas ate o cache caber no limite.
///
/// Roda no start. Usa o mtime como aproximacao de "menos usada": capas de
/// artistas que o usuario nao ouve mais nao sao reescritas e envelhecem
/// naturalmente.
pub fn prune(art_dir: &std::path::Path, limit_mb: u64) -> Result<u64> {
    let limit = limit_mb * 1024 * 1024;
    let mut arquivos: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;

    for sub in std::fs::read_dir(art_dir).into_iter().flatten().flatten() {
        for f in std::fs::read_dir(sub.path()).into_iter().flatten().flatten() {
            let Ok(md) = f.metadata() else { continue };
            if !md.is_file() {
                continue;
            }
            let mtime = md.modified().unwrap_or(std::time::UNIX_EPOCH);
            total += md.len();
            arquivos.push((mtime, md.len(), f.path()));
        }
    }

    if total <= limit {
        return Ok(total);
    }

    // Desce ate 80% do limite para nao rodar a poda a cada abertura.
    let alvo = limit * 8 / 10;
    arquivos.sort_by_key(|(mtime, _, _)| *mtime);

    for (_, len, path) in arquivos {
        if total <= alvo {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_e_estavel_e_valido() {
        let h = hash_url("https://lh3.googleusercontent.com/abc=w544-h544-l90-rj");
        assert_eq!(h.len(), 32);
        assert!(is_valid_hash(&h));
        assert_eq!(h, hash_url("https://lh3.googleusercontent.com/abc=w544-h544-l90-rj"));
    }

    #[test]
    fn hash_invalido_e_rejeitado() {
        assert!(!is_valid_hash("../../etc/passwd"));
        assert!(!is_valid_hash(""));
        assert!(!is_valid_hash(&"a".repeat(64)));
        assert!(!is_valid_hash("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }

    #[test]
    fn path_espalha_em_subdiretorios() {
        let p = path_for(Path::new("/cache/art"), "ab12345678901234567890123456789f");
        assert_eq!(p, Path::new("/cache/art/ab/ab12345678901234567890123456789f"));
    }

    #[test]
    fn sized_url_troca_a_dimensao() {
        assert_eq!(
            sized_url("https://lh3.googleusercontent.com/abc=w544-h544-l90-rj", 96),
            "https://lh3.googleusercontent.com/abc=w96-h96-l90-rj"
        );
    }

    #[test]
    fn sized_url_deixa_url_desconhecida_intacta() {
        let u = "https://example.com/capa.jpg";
        assert_eq!(sized_url(u, 96), u);
    }

    #[test]
    #[cfg(unix)]
    fn poda_respeita_o_limite_e_preserva_os_recentes() {
        let dir = std::env::temp_dir().join(format!("ytcl-poda-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 6 arquivos de 1 MB, gravados do mais antigo para o mais novo.
        let mut caminhos = Vec::new();
        for i in 0..6u32 {
            let hash = format!("{i:032x}");
            let p = path_for(&dir, &hash);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, vec![0u8; 1_000_000]).unwrap();
            // mtimes distintos para a ordenacao ser deterministica.
            let t = std::time::SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(1_000_000 + u64::from(i) * 60);
            filetime_set(&p, t);
            caminhos.push(p);
        }

        // Limite de 3 MB: desce ate 80% disso, ou seja ~2.4 MB -> sobram 2.
        let restante = prune(&dir, 3).unwrap();
        assert!(restante <= 3 * 1024 * 1024, "poda nao respeitou o limite");

        assert!(!caminhos[0].exists(), "o mais antigo deveria ter saido");
        assert!(caminhos[5].exists(), "o mais recente deveria ter ficado");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn poda_nao_mexe_em_cache_dentro_do_limite() {
        let dir = std::env::temp_dir().join(format!("ytcl-poda2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let p = path_for(&dir, &format!("{:032x}", 1));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, vec![0u8; 1000]).unwrap();

        prune(&dir, 256).unwrap();
        assert!(p.exists(), "nada deveria ser apagado abaixo do limite");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn poda_em_diretorio_inexistente_nao_estoura() {
        let dir = std::env::temp_dir().join("ytcl-nao-existe-mesmo");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(prune(&dir, 10).unwrap(), 0);
    }

    #[cfg(unix)]
    /// `std::fs` nao expoe ajuste de mtime; usamos a syscall diretamente para
    /// nao trazer uma dependencia so por causa de um teste.
    fn filetime_set(path: &std::path::Path, t: std::time::SystemTime) {
        use std::os::unix::ffi::OsStrExt;
        let secs = t
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let times = [
            libc::timespec { tv_sec: secs, tv_nsec: 0 },
            libc::timespec { tv_sec: secs, tv_nsec: 0 },
        ];
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: caminho valido terminado em NUL e array de dois timespec,
        // que e exatamente o que utimensat espera.
        let r = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0) };
        assert_eq!(r, 0, "utimensat falhou");
    }
}
