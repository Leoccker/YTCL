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

    let faixas: Vec<_> = page
        .items
        .iter()
        .filter_map(|i| match i {
            SearchItem::Track(t) => Some(t),
            _ => None,
        })
        .collect();
    let artistas = page.items.iter().filter(|i| matches!(i, SearchItem::Artist(_))).count();

    assert!(!faixas.is_empty(), "nenhuma faixa nos resultados");
    assert!(artistas > 0, "nenhum artista nos resultados");

    for t in &faixas {
        assert!(!t.id.is_empty(), "faixa sem videoId");
        assert!(!t.title.is_empty(), "faixa sem titulo");

        // As tres proximas checagens existem porque o `music_search_main` do
        // rustypipe passava nas anteriores devolvendo lixo: duracao ausente e
        // o rotulo da categoria ("Song") no lugar do nome do artista. Foi o
        // que motivou trocar a busca "tudo" por tres buscas filtradas.
        assert!(
            t.duration_secs.is_some(),
            "faixa \"{}\" sem duracao",
            t.title
        );
        assert!(!t.artists.is_empty(), "faixa \"{}\" sem artista", t.title);
        assert!(
            !t.artists.iter().any(|a| a.name == "Song" || a.name == "Video"),
            "faixa \"{}\" com rotulo de categoria no lugar do artista: {:?}",
            t.title,
            t.artists
        );
    }

    // Capa e o que a UI mostra em toda linha; sem ela a lista fica cega.
    let com_capa = page.items.iter().filter(|i| art_of(i).is_some()).count();
    assert_eq!(com_capa, page.items.len(), "ha itens sem capa");
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

fn art_of(item: &SearchItem) -> Option<&ytm_core::model::ArtRef> {
    match item {
        SearchItem::Track(t) => t.art.as_ref(),
        SearchItem::Album(a) => a.art.as_ref(),
        SearchItem::Artist(a) => a.art.as_ref(),
        SearchItem::Playlist(p) => p.art.as_ref(),
    }
}
