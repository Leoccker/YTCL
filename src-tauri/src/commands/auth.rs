//! Comandos de conta: entrar, sair, trocar, renomear.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};
use ytcl_core::auth::Account;
use ytcl_core::error::{CoreError, ErrorPayload};

use crate::login_window;
use crate::state::{AppState, SessionState};

type St<'a> = State<'a, Arc<AppState>>;
type CmdResult<T> = std::result::Result<T, ErrorPayload>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub session: SessionState,
    pub accounts: Vec<Account>,
}

#[tauri::command]
pub fn auth_status(state: St<'_>) -> AuthStatus {
    AuthStatus {
        session: state.session(),
        accounts: state.auth().accounts().unwrap_or_default(),
    }
}

/// Fluxo completo: abre o webview do Google, captura o cookie, valida contra
/// o YouTube, guarda no cofre e deixa a conta ativa.
#[tauri::command]
pub async fn auth_login_google(app: AppHandle, state: St<'_>) -> CmdResult<Account> {
    let cookie = login_window::capture_cookie(&app)
        .await
        .map_err(CoreError::Other)?;
    finish_login(&state, &cookie).await
}

/// Alternativa sem webview: o usuário cola o cabeçalho `Cookie` ou o conteúdo
/// de um `cookies.txt`. Útil quando o Google barra o webview.
#[tauri::command]
pub async fn auth_login_cookie(state: St<'_>, cookie: String) -> CmdResult<Account> {
    let cookie = cookie.trim();
    if cookie.is_empty() {
        return Err(CoreError::Other("cookie vazio".into()).into());
    }
    finish_login(&state, cookie).await
}

async fn finish_login(state: &Arc<AppState>, cookie: &str) -> CmdResult<Account> {
    // set_cookie valida: se o YouTube rejeitar, nada é gravado.
    state.innertube.set_cookie(cookie).await?;

    let account = state.auth().add(cookie)?;
    state.set_active_account(Some(account.id.clone()));
    state.set_session(SessionState::Active { account: account.clone() });
    Ok(account)
}

/// Troca a conta ativa. Re-hidrata a sessão com o cookie da conta escolhida.
#[tauri::command]
pub async fn auth_switch(state: St<'_>, id: String) -> CmdResult<SessionState> {
    let exists = state.auth().accounts()?.iter().any(|a| a.id == id);
    if !exists {
        return Err(CoreError::NotFound.into());
    }
    state.set_active_account(Some(id));
    state.hydrate_session().await;
    Ok(state.session())
}

/// Reconecta a conta atual — o usuário refaz o login e trocamos o cookie sem
/// criar conta nova.
#[tauri::command]
pub async fn auth_reconnect(app: AppHandle, state: St<'_>) -> CmdResult<SessionState> {
    let id = match state.session() {
        SessionState::Active { account } | SessionState::Expired { account } => account.id,
        SessionState::LoggedOut => return Err(CoreError::Unauthenticated.into()),
    };

    let cookie = login_window::capture_cookie(&app)
        .await
        .map_err(CoreError::Other)?;

    state.innertube.set_cookie(&cookie).await?;
    state.auth().update_cookie(&id, &cookie)?;
    state.hydrate_session().await;
    Ok(state.session())
}

#[tauri::command]
pub async fn auth_logout(state: St<'_>, id: String) -> CmdResult<SessionState> {
    state.auth().remove(&id)?;

    // Se era a conta ativa, cai para a próxima que sobrar (ou desloga).
    if state.config().last_account_id.as_deref() == Some(id.as_str()) {
        let proxima = state.auth().accounts()?.into_iter().next();
        state.innertube.clear_cookie().await;
        match proxima {
            Some(a) => {
                state.set_active_account(Some(a.id));
                state.hydrate_session().await;
            }
            None => {
                state.set_active_account(None);
                state.set_session(SessionState::LoggedOut);
            }
        }
    }
    Ok(state.session())
}

#[tauri::command]
pub fn auth_rename(state: St<'_>, id: String, label: String) -> CmdResult<Vec<Account>> {
    state.auth().rename(&id, label.trim())?;
    Ok(state.auth().accounts().unwrap_or_default())
}
