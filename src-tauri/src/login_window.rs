//! Janela de login do Google.
//!
//! Não temos webview na UI principal (é egui? não — é Tauri, mas a janela
//! principal roda o nosso frontend). Aqui abrimos uma **segunda** janela,
//! apontada para o login do Google, e quando ela chega em `music.youtube.com`
//! lemos os cookies de sessão e fechamos.
//!
//! Detalhe importante do YouTube: ele rotaciona os cookies a cada poucos
//! minutos enquanto a aba do site fica aberta. Por isso capturamos e fechamos
//! na hora — deixar a janela viva invalidaria o cookie que acabamos de pegar.

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri::webview::WebviewWindow;
use url::Url;

const LOGIN_LABEL: &str = "ytcl-login";

/// UA de Chrome desktop. Sem isto o Google barra o webview com "este navegador
/// não é seguro". Não é garantia — se o Google apertar, resta o caminho de
/// colar o cookie manualmente.
const DESKTOP_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

const LOGIN_URL: &str =
    "https://accounts.google.com/ServiceLogin?continue=https%3A%2F%2Fmusic.youtube.com%2F";

/// Abre a janela de login e resolve com o cabeçalho `Cookie` da sessão do
/// YouTube. Erro se o usuário fechar a janela antes de terminar.
pub async fn capture_cookie(app: &AppHandle) -> Result<String, String> {
    // Uma janela de login de cada vez.
    if let Some(existing) = app.get_webview_window(LOGIN_LABEL) {
        let _ = existing.close();
    }

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    let tx = Arc::new(Mutex::new(Some(tx)));

    let url: Url = LOGIN_URL.parse().map_err(|e| format!("URL de login inválida: {e}"))?;

    let nav_tx = tx.clone();
    let nav_app = app.clone();
    let done = Arc::new(Mutex::new(false));

    let window = WebviewWindowBuilder::new(app, LOGIN_LABEL, WebviewUrl::External(url))
        .title("Entrar com o Google")
        .inner_size(480.0, 660.0)
        .min_inner_size(360.0, 480.0)
        .user_agent(DESKTOP_UA)
        .center()
        .focused(true)
        .on_navigation(move |url| {
            if url.host_str() == Some("music.youtube.com") {
                let mut d = done.lock();
                if !*d {
                    *d = true;
                    // A navegação dispara antes de o cookie assentar; um
                    // pequeno atraso e então lemos.
                    let app = nav_app.clone();
                    let tx = nav_tx.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                        let result = match app.get_webview_window(LOGIN_LABEL) {
                            Some(w) => {
                                let r = read_cookie_header(&w);
                                let _ = w.close();
                                r
                            }
                            None => Err("janela de login sumiu".into()),
                        };
                        if let Some(tx) = tx.lock().take() {
                            let _ = tx.send(result);
                        }
                    });
                }
            }
            true
        })
        .build()
        .map_err(|e| format!("não consegui abrir a janela de login: {e}"))?;

    // Usuário fechou a janela sem concluir.
    let close_tx = tx.clone();
    window.on_window_event(move |ev| {
        if matches!(ev, WindowEvent::Destroyed) {
            if let Some(tx) = close_tx.lock().take() {
                let _ = tx.send(Err("login cancelado".into()));
            }
        }
    });

    rx.await.unwrap_or_else(|_| Err("login interrompido".into()))
}

/// Monta o cabeçalho `Cookie` a partir do que o webview tem para o YouTube.
fn read_cookie_header(w: &WebviewWindow) -> Result<String, String> {
    let url: Url = "https://music.youtube.com/"
        .parse()
        .map_err(|e| format!("{e}"))?;

    let cookies = w
        .cookies_for_url(url)
        .map_err(|e| format!("não consegui ler os cookies: {e}"))?;

    if cookies.is_empty() {
        return Err("nenhum cookie de sessão encontrado".into());
    }

    // O rustypipe precisa do SAPISID (ou __Secure-3PAPISID) para o hash de
    // autenticação. Se não vier, o login não valeu.
    let tem_auth = cookies
        .iter()
        .any(|c| c.name() == "SAPISID" || c.name() == "__Secure-3PAPISID");
    if !tem_auth {
        return Err("login incompleto — cookies de autenticação não vieram".into());
    }

    Ok(cookies
        .iter()
        .map(|c| format!("{}={}", c.name(), c.value()))
        .collect::<Vec<_>>()
        .join("; "))
}
