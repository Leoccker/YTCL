//! Comandos de busca e navegacao.
//!
//! Padrao de todos eles: le do cache e devolve na hora, marcando se o dado
//! esta vencido. Se estiver, o frontend dispara a mesma chamada com
//! `refresh: true` — a tela aparece instantaneamente e se corrige sozinha,
//! sem spinner cobrindo nada.

use std::time::Duration;

use serde::Serialize;
use tauri::State;
use ytm_core::cache::Entry;
use ytm_core::error::ErrorPayload;
use ytm_core::model::{
    Album, ArtRef, Artist, Page, Playlist, SearchFilter, SearchItem, Track,
};

use crate::state::{AppState, TTL_DETAIL, TTL_SEARCH};

type CmdResult<T> = std::result::Result<Cached<T>, ErrorPayload>;

/// Resposta de um comando de leitura.
///
/// `stale` e o que permite o padrao stale-while-revalidate sem eventos: o
/// frontend desenha `data` de imediato e, se `stale`, repete a chamada com
/// `refresh: true`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cached<T> {
    pub data: T,
    pub stale: bool,
}

impl<T> Cached<T> {
    fn fresh(data: T) -> Self {
        Self { data, stale: false }
    }
}

/// Registra de onde veio cada capa.
///
/// Sem isto o handler do `ytmart://` recebe um hash que nao sabe resolver, e
/// a imagem nunca aparece. E o unico acoplamento entre buscar metadado e
/// exibir capa — e vale o preco de manter o IPC livre de imagens.
fn remember_art(state: &AppState, arts: impl Iterator<Item = ArtRef>) {
    for art in arts {
        state.cache.remember_art(&art.hash, &art.url);
    }
}

fn art_of_search(items: &[SearchItem]) -> impl Iterator<Item = ArtRef> + '_ {
    items.iter().filter_map(|i| match i {
        SearchItem::Track(t) => t.art.clone(),
        SearchItem::Album(a) => a.art.clone(),
        SearchItem::Artist(a) => a.art.clone(),
        SearchItem::Playlist(p) => p.art.clone(),
    })
}

fn art_of_tracks(tracks: &[Track]) -> impl Iterator<Item = ArtRef> + '_ {
    tracks.iter().filter_map(|t| t.art.clone())
}

/// Le do cache; se faltar ou `refresh` for pedido, busca e grava.
///
/// Um erro de rede com dado velho em maos devolve o dado velho: ficar offline
/// nao deveria esvaziar a tela do usuario.
async fn cached_or_fetch<T, F, Fut>(
    state: &AppState,
    key: &str,
    ttl: Duration,
    refresh: bool,
    fetch: F,
) -> std::result::Result<Cached<T>, ErrorPayload>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ytm_core::Result<T>>,
{
    if !refresh {
        if let Some(entry) = state.cache.get::<T>(key, ttl) {
            let stale = entry.is_stale();
            return Ok(Cached { data: entry.into_inner(), stale });
        }
    }

    match fetch().await {
        Ok(data) => {
            state.cache.put(key, &data);
            Ok(Cached::fresh(data))
        }
        Err(e) => {
            // Rede caiu, mas temos algo gravado: melhor mostrar velho do que
            // um erro. O `stale` avisa a UI que aquilo nao esta atualizado.
            if let Some(Entry::Fresh(v) | Entry::Stale(v)) = state.cache.get::<T>(key, ttl) {
                tracing::debug!("usando cache vencido para {key}: {e}");
                return Ok(Cached { data: v, stale: true });
            }
            Err(e.into())
        }
    }
}

#[tauri::command]
pub async fn search(
    state: State<'_, std::sync::Arc<AppState>>,
    query: String,
    filter: SearchFilter,
    refresh: bool,
) -> CmdResult<Page<SearchItem>> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Cached::fresh(Page::empty()));
    }

    let key = format!("search:{filter:?}:{}", query.to_lowercase());
    let it = state.innertube.clone();
    let q = query.clone();

    let result = cached_or_fetch(&state, &key, TTL_SEARCH, refresh, || async move {
        it.search(&q, filter).await
    })
    .await?;

    remember_art(&state, art_of_search(&result.data.items));
    Ok(result)
}

/// Continuacao de busca. Nao passa pelo cache: o token so vale uma vez e
/// guardar paginas soltas nao ajudaria em nada.
#[tauri::command]
pub async fn search_more(
    state: State<'_, std::sync::Arc<AppState>>,
    continuation: String,
) -> std::result::Result<Page<SearchItem>, ErrorPayload> {
    let page = state.innertube.search_more(&continuation).await?;
    remember_art(&state, art_of_search(&page.items));
    Ok(page)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumView {
    pub album: Album,
    pub tracks: Vec<Track>,
}

#[tauri::command]
pub async fn album(
    state: State<'_, std::sync::Arc<AppState>>,
    id: String,
    refresh: bool,
) -> CmdResult<AlbumView> {
    let key = format!("album:{id}");
    let it = state.innertube.clone();
    let aid = id.clone();

    let result = cached_or_fetch(&state, &key, TTL_DETAIL, refresh, || async move {
        let (album, tracks) = it.album(&aid).await?;
        Ok(AlbumViewOwned { album, tracks })
    })
    .await
    .map(|c| Cached { data: AlbumView { album: c.data.album, tracks: c.data.tracks }, stale: c.stale })?;

    remember_art(&state, art_of_tracks(&result.data.tracks));
    remember_art(&state, result.data.album.art.clone().into_iter());
    Ok(result)
}

/// Espelho serializavel de `AlbumView` para atravessar o cache.
#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlbumViewOwned {
    album: Album,
    tracks: Vec<Track>,
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistView {
    pub artist: Artist,
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
}

#[tauri::command]
pub async fn artist(
    state: State<'_, std::sync::Arc<AppState>>,
    id: String,
    refresh: bool,
) -> CmdResult<ArtistView> {
    let key = format!("artist:{id}");
    let it = state.innertube.clone();
    let aid = id.clone();

    let result = cached_or_fetch(&state, &key, TTL_DETAIL, refresh, || async move {
        let (artist, tracks, albums) = it.artist(&aid).await?;
        Ok(ArtistView { artist, tracks, albums })
    })
    .await?;

    remember_art(&state, art_of_tracks(&result.data.tracks));
    remember_art(&state, result.data.albums.iter().filter_map(|a| a.art.clone()));
    remember_art(&state, result.data.artist.art.clone().into_iter());
    Ok(result)
}

#[tauri::command]
pub async fn playlist(
    state: State<'_, std::sync::Arc<AppState>>,
    id: String,
    refresh: bool,
) -> CmdResult<Playlist> {
    let key = format!("playlist:{id}");
    let it = state.innertube.clone();
    let pid = id.clone();

    let result = cached_or_fetch(&state, &key, TTL_DETAIL, refresh, || async move {
        it.playlist(&pid).await
    })
    .await?;

    remember_art(&state, result.data.art.clone().into_iter());
    Ok(result)
}

/// Faixas de uma playlist, paginadas.
///
/// Paginado no comando e nao no frontend de proposito: uma playlist pode ter
/// milhares de faixas, e serializar isso de uma vez trava os dois lados do IPC.
#[tauri::command]
pub async fn playlist_tracks(
    state: State<'_, std::sync::Arc<AppState>>,
    id: String,
    continuation: Option<String>,
) -> std::result::Result<Page<Track>, ErrorPayload> {
    let page = state
        .innertube
        .playlist_tracks(&id, continuation.as_deref())
        .await?;
    remember_art(&state, art_of_tracks(&page.items));
    Ok(page)
}
