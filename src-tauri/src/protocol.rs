//! Handler do protocolo `ytmart://`.
//!
//! Por que isto existe: mandar capa por `invoke` (base64 no JSON do IPC) e o
//! erro classico que engasga a rolagem de uma grade de albuns — serializa,
//! trafega, decodifica em JS, tudo na thread principal do webview. Servindo
//! por um protocolo custom, o `<img>` vira uma imagem normal: o webview
//! decodifica fora da thread principal, cacheia sozinho e o IPC fica livre
//! para o que importa.
//!
//! URL no Linux: `ytmart://localhost/<hash>`
//! URL no Windows: `http://ytmart.localhost/<hash>`
//! O frontend nao precisa saber disso — usa `convertFileSrc(hash, "ytmart")`.

use std::path::PathBuf;

use tauri::http::{Request, Response, StatusCode};
use ytm_core::artwork;

/// Extrai o hash da URI, seja qual for a forma que a plataforma usa.
fn hash_from_uri(uri: &str) -> Option<&str> {
    let path = uri
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(uri);
    // Depois do host vem `/<hash>`; sem host, o proprio caminho ja e o hash.
    let last = path.rsplit('/').next()?;
    let last = last.split(['?', '#']).next()?;
    (!last.is_empty()).then_some(last)
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Vec::new())
        .expect("resposta 404 e sempre valida")
}

/// Serve um arquivo do cache de capas.
///
/// A validacao do hash (32 chars hex, gerados por nos) e o que impede path
/// traversal: nenhum caminho vindo da URI e concatenado sem passar por ela.
pub fn serve(art_dir: &PathBuf, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let uri = request.uri().to_string();

    let Some(hash) = hash_from_uri(&uri) else {
        return not_found();
    };

    if !artwork::is_valid_hash(hash) {
        tracing::debug!("ytmart: hash rejeitado: {hash:?}");
        return not_found();
    }

    let path = artwork::path_for(art_dir, hash);

    match std::fs::read(&path) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "image/jpeg")
            // O conteudo e imutavel: a chave e o hash da URL de origem.
            .header("Cache-Control", "public, max-age=31536000, immutable")
            .header("Access-Control-Allow-Origin", "*")
            .body(bytes)
            .expect("resposta 200 e sempre valida"),
        Err(_) => not_found(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrai_hash_das_duas_formas_de_uri() {
        let h = "ab12345678901234567890123456789f";
        assert_eq!(hash_from_uri(&format!("ytmart://localhost/{h}")), Some(h));
        assert_eq!(hash_from_uri(&format!("http://ytmart.localhost/{h}")), Some(h));
        assert_eq!(hash_from_uri(&format!("ytmart://{h}")), Some(h));
    }

    #[test]
    fn ignora_query_e_fragmento() {
        let h = "ab12345678901234567890123456789f";
        assert_eq!(hash_from_uri(&format!("ytmart://localhost/{h}?v=2")), Some(h));
    }

    #[test]
    fn uri_sem_hash_nao_resolve() {
        assert_eq!(hash_from_uri("ytmart://localhost/"), None);
    }

    #[test]
    fn path_traversal_nao_passa_da_validacao() {
        // Mesmo que a URI traga algo assim, `is_valid_hash` barra antes de
        // qualquer acesso a disco.
        let malicioso = hash_from_uri("ytmart://localhost/..%2F..%2Fetc%2Fpasswd").unwrap();
        assert!(!artwork::is_valid_hash(malicioso));
    }
}
