mod commands;
mod login_window;
mod protocol;
mod state;

use std::sync::Arc;

use ytcl_core::config::Paths;

use crate::state::AppState;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,ytcl=debug,ytcl_core=debug".into()),
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
        std::thread::spawn(move || match ytcl_core::artwork::prune(&art_dir, limit) {
            Ok(bytes) => tracing::debug!("cache de capas: {:.1} MB", bytes as f64 / 1e6),
            Err(e) => tracing::warn!("poda do cache de capas falhou: {e}"),
        });
    }

    let protocol_state = state.clone();
    let setup_state = state.clone();

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
            commands::home,
            commands::search,
            commands::search_more,
            commands::album,
            commands::artist,
            commands::playlist,
            commands::playlist_tracks,
            commands::auth_status,
            commands::auth_login_google,
            commands::auth_login_cookie,
            commands::auth_switch,
            commands::auth_reconnect,
            commands::auth_logout,
            commands::auth_rename,
            commands::library_playlists,
            commands::library_albums,
            commands::library_artists,
            commands::liked_songs,
            commands::player_play_tracks,
            commands::player_toggle,
            commands::player_next,
            commands::player_prev,
            commands::player_seek,
            commands::player_set_volume,
            commands::player_set_repeat,
            commands::player_set_shuffle,
            commands::player_play_next,
            commands::player_enqueue,
            commands::player_move_queue,
            commands::player_jump_queue,
            commands::player_snapshot,
            commands::player_available,
        ])
        .setup(move |app| {
            // A ponte de sessao nasce antes da hidratacao. Assim, se ela
            // terminar depois de a UI ouvir, a troca chega por evento; se
            // terminar antes, `auth_status` ainda devolve o snapshot atual.
            let app_handle = app.handle().clone();
            let mut updates = setup_state.subscribe_session();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;

                while updates.changed().await.is_ok() {
                    let _ = app_handle.emit("auth-session", updates.borrow().clone());
                }
            });

            let hydrate_state = setup_state.clone();
            tauri::async_runtime::spawn(async move {
                hydrate_state.hydrate_session().await;
            });

            // O player usa tokio::spawn, entao precisa nascer DENTRO de uma
            // task do runtime (o setup() roda fora do contexto do reactor).
            // A ponte reencaminha cada PlayerEvent como evento Tauri "player".
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::{Emitter, Manager};
                let (to_ui, mut from_player) = tokio::sync::mpsc::unbounded_channel();

                // yt-dlp: instala a copia gerenciada a partir do binario
                // embutido no bundle (se houver — pode faltar num build de
                // desenvolvimento sem o download do empacotamento, o que
                // nunca e erro). So quem resolve `resource_dir` e o
                // src-tauri; o ytcl-core so recebe o caminho.
                let embedded = app_handle
                    .path()
                    .resource_dir()
                    .ok()
                    .map(|dir| dir.join("yt-dlp").join(ytcl_core::ytdlp_manager::bin_name()));
                if let Err(e) = ytcl_core::ytdlp_manager::install_from_embedded(
                    &setup_state.paths.bin_dir,
                    embedded.as_deref(),
                ) {
                    tracing::warn!("yt-dlp: nao consegui instalar a copia gerenciada: {e}");
                }

                // Precisa rodar depois da instalacao acima: e ela que faz a
                // copia gerenciada existir a tempo da primeira escolha de
                // binario, dentro de init_player.
                setup_state.init_player(to_ui);

                // Atualizacao da copia gerenciada em segundo plano — no
                // maximo 1x/dia (o proprio modulo checa o carimbo), sem
                // atrasar a abertura nem trocar o binario que o YtDlp do
                // player ja esta usando (o caminho continua o mesmo; so o
                // conteudo do arquivo muda, entao a proxima resolucao ja
                // usa a versao nova).
                tokio::spawn(ytcl_core::ytdlp_manager::run_update_task(
                    setup_state.paths.bin_dir.clone(),
                ));

                // A UI assina "player" antes de ler o snapshot. Este sinal
                // cobre o caso em que o mpv so fica pronto depois da leitura.
                let _ = app_handle.emit("player", serde_json::json!({ "event": "ready" }));
                while let Some(ev) = from_player.recv().await {
                    let _ = app_handle.emit("player", ev);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o app");
}
