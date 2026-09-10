//! Testes contra a API real do YouTube.
//!
//! Marcados `#[ignore]` porque dependem de rede e de o YouTube nao ter mudado
//! nada. Sao o canario do projeto: rodam num cron diario no CI e avisam que a
//! API interna mudou antes que o usuario descubra sozinho.
//!
//! Rodar com: `cargo test -p ytm-core --test innertube_live -- --ignored`

use ytm_core::innertube::InnerTube;
use ytm_core::metadata::MetadataSource;
use ytm_core::model::{SearchFilter, SearchItem};

fn client() -> InnerTube {
    let dir = std::env::temp_dir().join("ytmc-test-cache");
    std::fs::create_dir_all(&dir).expect("criar cache de teste");
    InnerTube::new(&dir).expect("iniciar cliente")
}

#[tokio::test]
#[ignore = "precisa de rede"]
async fn busca_generica_devolve_itens_de_varios_tipos() {
    let page = client()
        .search("radiohead", SearchFilter::All)
        .await
        .expect("busca falhou");

    assert!(!page.items.is_empty(), "busca nao devolveu nada");

    let tem_faixa = page.items.iter().any(|i| matches!(i, SearchItem::Track(_)));
    let tem_artista = page.items.iter().any(|i| matches!(i, SearchItem::Artist(_)));
    assert!(tem_faixa || tem_artista, "nenhum resultado reconhecivel");

    // Toda faixa precisa de id e titulo; sem isso nao da nem para tocar.
    for item in &page.items {
        if let SearchItem::Track(t) = item {
            assert!(!t.id.is_empty(), "faixa sem videoId");
            assert!(!t.title.is_empty(), "faixa sem titulo");
        }
    }
}

#[tokio::test]
#[ignore = "precisa de rede"]
async fn busca_de_faixas_pagina() {
    let it = client();
    let first = it
        .search("lo-fi", SearchFilter::Songs)
        .await
        .expect("busca falhou");

    assert!(!first.items.is_empty());
    let cursor = first.continuation.expect("primeira pagina sem continuacao");

    let second = it.search_more(&cursor).await.expect("continuacao falhou");
    assert!(!second.items.is_empty(), "segunda pagina veio vazia");

    // As paginas nao podem se repetir — seria sinal de cursor ignorado.
    let ids_1: Vec<_> = first.items.iter().filter_map(id_of).collect();
    let ids_2: Vec<_> = second.items.iter().filter_map(id_of).collect();
    assert!(
        ids_2.iter().any(|id| !ids_1.contains(id)),
        "segunda pagina repetiu a primeira"
    );
}

#[tokio::test]
#[ignore = "precisa de rede"]
async fn album_traz_as_faixas_e_a_capa() {
    let it = client();
    let page = it
        .search("in rainbows", SearchFilter::Albums)
        .await
        .expect("busca falhou");

    let SearchItem::Album(album) = page.items.first().expect("sem albuns") else {
        panic!("primeiro item nao e album");
    };

    let (detail, tracks) = it.album(&album.id).await.expect("album falhou");
    assert!(!tracks.is_empty(), "album sem faixas");
    assert!(detail.art.is_some(), "album sem capa");
}

fn id_of(item: &SearchItem) -> Option<&str> {
    match item {
        SearchItem::Track(t) => Some(&t.id),
        SearchItem::Album(a) => Some(&a.id),
        SearchItem::Artist(a) => Some(&a.id),
        SearchItem::Playlist(p) => Some(&p.id),
    }
}
