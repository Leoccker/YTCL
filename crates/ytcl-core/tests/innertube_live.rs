//! Testes contra a API real do YouTube.
//!
//! Marcados `#[ignore]` porque dependem de rede e de o YouTube nao ter mudado
//! nada. Sao o canario do projeto: rodam num cron diario no CI e avisam que a
//! API interna mudou antes que o usuario descubra sozinho.
//!
//! Rodar com: `cargo test -p ytcl-core --test innertube_live -- --ignored`

use ytcl_core::innertube::InnerTube;
use ytcl_core::metadata::MetadataSource;
use ytcl_core::model::{SearchFilter, SearchItem};

fn client() -> InnerTube {
    let dir = std::env::temp_dir().join("ytcl-test-cache");
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

fn art_of(item: &SearchItem) -> Option<&ytcl_core::model::ArtRef> {
    match item {
        SearchItem::Track(t) => t.art.as_ref(),
        SearchItem::Album(a) => a.art.as_ref(),
        SearchItem::Artist(a) => a.art.as_ref(),
        SearchItem::Playlist(p) => p.art.as_ref(),
    }
}

// --- autenticação --------------------------------------------------------

#[tokio::test]
#[ignore = "precisa de rede"]
async fn cookie_invalido_vira_sessao_expirada_sem_travar() {
    let it = client();

    // Um cookie sintático mas sem valer nada. O rustypipe vai buscar o
    // youtube.com com ele e não vai achar os cabeçalhos de sessão.
    let res = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        it.set_cookie("SID=nope; HSID=nope; SAPISID=nope"),
    )
    .await
    .expect("set_cookie não pode pendurar");

    assert!(
        matches!(res, Err(ytcl_core::CoreError::SessionExpired)),
        "cookie ruim deveria dar SessionExpired, deu {res:?}"
    );
}

#[tokio::test]
#[ignore = "precisa de rede"]
async fn biblioteca_sem_login_reclama_de_autenticacao() {
    use ytcl_core::metadata::MetadataSource;

    let it = client();
    let res = it.library_playlists().await;
    assert!(
        matches!(
            res,
            Err(ytcl_core::CoreError::SessionExpired | ytcl_core::CoreError::Unauthenticated)
        ),
        "sem cookie, a biblioteca deveria pedir login, deu {res:?}"
    );
}

#[tokio::test]
#[ignore = "precisa de conta conectada no cofre do SO"]
async fn library_playlists_pagina_ate_o_fim_sem_quebrar() {
    use ytcl_core::auth::{AuthStore, KeyringStore};

    let store = KeyringStore;
    let auth = AuthStore::new(&store);
    let Some(acc) = auth.accounts().unwrap_or_default().into_iter().next() else {
        eprintln!("sem conta no cofre — pulando");
        return;
    };
    let cookie = auth
        .cookie(&acc.id)
        .expect("ler cookie")
        .expect("conta sem cookie");

    let it = client();
    it.set_cookie(&cookie).await.expect("cookie deveria estar válido");

    // Antes do patch no rustypipe (`#[serde(default)]` em `GridRenderer.items`)
    // isto dava `missing field \`items\`` ao paginar até a página-marcador de
    // fim — um `gridContinuation` que o YouTube manda sem o campo `items`.
    let playlists = it
        .library_playlists()
        .await
        .expect("library_playlists não deveria falhar na paginação");

    assert!(!playlists.is_empty(), "biblioteca sem playlists?");
    for p in &playlists {
        assert!(!p.id.is_empty(), "playlist sem id");
        assert!(!p.title.is_empty(), "playlist sem título");
    }
}
