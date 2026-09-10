//! Comandos que exigem conta conectada: biblioteca, playlists salvas,
//! curtidas.
//!
//! Todos passam por `authed`, que traduz "não logado / cookie rejeitado" em
//! `SessionExpired` e marca a sessão como expirada — é o que dispara o banner
//! de reconexão na UI.

use std::sync::Arc;

use tauri::State;
use ytcl_core::error::{CoreError, ErrorPayload};
use ytcl_core::metadata::MetadataSource;
use ytcl_core::model::{Album, Artist, Page, Playlist, Track};

use crate::commands::browse::remember_art;
use crate::state::AppState;

type St<'a> = State<'a, Arc<AppState>>;
type CmdResult<T> = std::result::Result<T, ErrorPayload>;

/// Executa uma chamada de biblioteca, convertendo falha de autenticação em
/// estado "expirado".
async fn authed<T, F, Fut>(state: &Arc<AppState>, call: F) -> CmdResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ytcl_core::Result<T>>,
{
    match call().await {
        Ok(v) => Ok(v),
        Err(e @ (CoreError::SessionExpired | CoreError::Unauthenticated)) => {
            state.mark_expired();
            Err(e.into())
        }
        Err(e) => Err(e.into()),
    }
}

#[tauri::command]
pub async fn library_playlists(state: St<'_>) -> CmdResult<Vec<Playlist>> {
    let it = state.innertube.clone();
    let list = authed(&state, || async move { it.library_playlists().await }).await?;
    remember_art(&state, list.iter().filter_map(|p| p.art.clone()));
    Ok(list)
}

#[tauri::command]
pub async fn library_albums(state: St<'_>) -> CmdResult<Vec<Album>> {
    let it = state.innertube.clone();
    let list = authed(&state, || async move { it.library_albums().await }).await?;
    remember_art(&state, list.iter().filter_map(|a| a.art.clone()));
    Ok(list)
}

#[tauri::command]
pub async fn library_artists(state: St<'_>) -> CmdResult<Vec<Artist>> {
    let it = state.innertube.clone();
    let list = authed(&state, || async move { it.library_artists().await }).await?;
    remember_art(&state, list.iter().filter_map(|a| a.art.clone()));
    Ok(list)
}

#[tauri::command]
pub async fn liked_songs(
    state: St<'_>,
    continuation: Option<String>,
) -> CmdResult<Page<Track>> {
    let it = state.innertube.clone();
    let cont = continuation.clone();
    let page = authed(&state, || async move {
        it.liked_songs(cont.as_deref()).await
    })
    .await?;
    remember_art(&state, page.items.iter().filter_map(|t| t.art.clone()));
    Ok(page)
}
