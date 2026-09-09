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
}
