//! Comandos de reprodução. Todos delegam ao `Player`; se o libmpv não subiu,
//! devolvem um erro claro em vez de silêncio.

use std::sync::Arc;

use tauri::State;
use ytcl_core::error::{CoreError, ErrorPayload};
use ytcl_core::model::Track;
use ytcl_player::{PlayerSnapshot, RepeatMode};

use crate::commands::browse::remember_art;
use crate::state::AppState;

type St<'a> = State<'a, Arc<AppState>>;
type R<T> = std::result::Result<T, ErrorPayload>;

fn player(state: &Arc<AppState>) -> R<Arc<ytcl_player::Player>> {
    state
        .player()
        .ok_or_else(|| CoreError::Other("reprodução indisponível (libmpv não carregou)".into()).into())
}

/// Registra as capas das faixas antes de tocar — o `ytmart://` precisa do
/// mapa hash→url, igual aos comandos de navegação.
fn remember_tracks(state: &Arc<AppState>, tracks: &[Track]) {
    remember_art(state, tracks.iter().filter_map(|t| t.art.clone()));
}

#[tauri::command]
pub async fn player_play_tracks(state: St<'_>, tracks: Vec<Track>, start: usize) -> R<()> {
    remember_tracks(&state, &tracks);
    player(&state)?
        .play_tracks(tracks, start)
        .await
        .map_err(|e| CoreError::Other(e.to_string()).into())
}

#[tauri::command]
pub async fn player_toggle(state: St<'_>) -> R<()> {
    player(&state)?.toggle_pause().await.map_err(wrap)
}

#[tauri::command]
pub async fn player_next(state: St<'_>) -> R<()> {
    player(&state)?.next().await.map_err(wrap)
}

#[tauri::command]
pub async fn player_prev(state: St<'_>) -> R<()> {
    player(&state)?.prev().await.map_err(wrap)
}

#[tauri::command]
pub async fn player_seek(state: St<'_>, secs: f64) -> R<()> {
    player(&state)?.seek(secs).await.map_err(wrap)
}

#[tauri::command]
pub async fn player_set_volume(state: St<'_>, level: f64) -> R<()> {
    player(&state)?.set_volume(level).await.map_err(wrap)
}

#[tauri::command]
pub fn player_set_repeat(state: St<'_>, mode: RepeatMode) -> R<()> {
    player(&state)?.set_repeat(mode);
    Ok(())
}

#[tauri::command]
pub fn player_set_shuffle(state: St<'_>, on: bool) -> R<()> {
    player(&state)?.set_shuffle(on);
    Ok(())
}

#[tauri::command]
pub fn player_play_next(state: St<'_>, track: Track) -> R<()> {
    remember_tracks(&state, std::slice::from_ref(&track));
    player(&state)?.play_next(track);
    Ok(())
}

#[tauri::command]
pub fn player_enqueue(state: St<'_>, track: Track) -> R<()> {
    remember_tracks(&state, std::slice::from_ref(&track));
    player(&state)?.enqueue(track);
    Ok(())
}

#[tauri::command]
pub fn player_move_queue(state: St<'_>, from: usize, to: usize) -> R<()> {
    player(&state)?.move_in_queue(from, to);
    Ok(())
}

#[tauri::command]
pub async fn player_jump_queue(state: St<'_>, order_index: usize) -> R<()> {
    player(&state)?.jump_in_queue(order_index).await.map_err(wrap)
}

#[tauri::command]
pub fn player_snapshot(state: St<'_>) -> R<Option<PlayerSnapshot>> {
    Ok(state.player().map(|p| p.snapshot()))
}

/// Estado inicial mínimo para a UI antes de o player existir.
#[tauri::command]
pub fn player_available(state: St<'_>) -> bool {
    state.player().is_some()
}

fn wrap(e: anyhow::Error) -> ErrorPayload {
    CoreError::Other(e.to_string()).into()
}

