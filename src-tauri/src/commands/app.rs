use serde::Serialize;
use tauri::State;
use ytm_core::config::Config;

use crate::state::AppState;

#[derive(Serialize)]
pub struct AppInfo {
    pub version: &'static str,
    pub config_dir: String,
    pub cache_dir: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        config_dir: state.paths.config_dir.display().to_string(),
        cache_dir: state.paths.cache_dir.display().to_string(),
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.config()
}

#[tauri::command]
pub fn set_config(state: State<'_, AppState>, config: Config) {
    state.update_config(config);
}
