mod commands;
mod protocol;
mod state;

use ytm_core::config::Paths;

use crate::state::AppState;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,ytmc=debug,ytm_core=debug".into()),
        )
        .init();

    let paths = Paths::discover().expect("nao consegui determinar os diretorios do app");
    paths.ensure().expect("nao consegui criar os diretorios do app");
    tracing::info!("config: {}", paths.config_dir.display());
    tracing::info!("cache:  {}", paths.cache_dir.display());

    let art_dir = paths.art_dir.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(paths))
        // Capas saem por aqui, nunca pelo IPC. Ver protocol.rs.
        .register_uri_scheme_protocol("ytmart", move |_ctx, request| {
            protocol::serve(&art_dir, &request)
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_config,
            commands::set_config,
            commands::mem_info,
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o app");
}
