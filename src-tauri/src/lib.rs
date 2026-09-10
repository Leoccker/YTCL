mod commands;
mod protocol;
mod state;

use std::sync::Arc;

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

    let state = Arc::new(AppState::new(paths).expect("nao consegui iniciar o estado do app"));

    // Poda do cache de capas na abertura: barato, e evita que o diretorio
    // cresca sem limite entre execucoes.
    {
        let art_dir = state.paths.art_dir.clone();
        let limit = state.config().art_cache_mb;
        std::thread::spawn(move || match ytm_core::artwork::prune(&art_dir, limit) {
            Ok(bytes) => tracing::debug!("cache de capas: {:.1} MB", bytes as f64 / 1e6),
            Err(e) => tracing::warn!("poda do cache de capas falhou: {e}"),
        });
    }

    let protocol_state = state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        // Capas saem por aqui, nunca pelo IPC. O handler e assincrono porque
        // baixa a imagem no primeiro acesso. Ver protocol.rs.
        .register_asynchronous_uri_scheme_protocol("ytmart", move |_ctx, request, responder| {
            let state = protocol_state.clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(protocol::serve(state, request).await);
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_config,
            commands::set_config,
            commands::mem_info,
            commands::search,
            commands::search_more,
            commands::album,
            commands::artist,
            commands::playlist,
            commands::playlist_tracks,
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o app");
}
