//! Diagnóstico das rotas de biblioteca com a conta já conectada no app.
//!
//! Lê o cookie da conta ativa do cofre do SO e tenta cada chamada, dizendo
//! o que cada uma devolve. Uso: `cargo run -p ytcl-core --example library`

use ytcl_core::auth::{AuthStore, KeyringStore};
use ytcl_core::config::Paths;
use ytcl_core::innertube::InnerTube;
use ytcl_core::metadata::MetadataSource;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths = Paths::discover()?;
    let store = KeyringStore;
    let auth = AuthStore::new(&store);

    let accounts = auth.accounts()?;
    let Some(acc) = accounts.first() else {
        eprintln!("nenhuma conta no cofre — entre pelo app primeiro");
        return Ok(());
    };
    println!("conta: {} ({})", acc.label, acc.id);

    let cookie = auth.cookie(&acc.id)?.ok_or("conta sem cookie")?;
    let it = InnerTube::new(&paths.cache_dir)?;

    print!("set_cookie… ");
    match it.set_cookie(&cookie).await {
        Ok(()) => println!("ok, sessão válida"),
        Err(e) => {
            println!("FALHOU: {e}");
            return Ok(());
        }
    }

    macro_rules! probe {
        ($nome:expr, $call:expr) => {{
            let t = std::time::Instant::now();
            match $call.await {
                Ok(v) => println!("  {:<20} {} itens em {:.1?}", $nome, v.len(), t.elapsed()),
                Err(e) => println!("  {:<20} ERRO em {:.1?}: {e}", $nome, t.elapsed()),
            }
        }};
    }

    println!("\nrotas de biblioteca:");
    probe!("playlists salvas", it.library_playlists());
    probe!("álbuns salvos", it.library_albums());
    probe!("artistas salvos", it.library_artists());

    let t = std::time::Instant::now();
    match it.liked_songs(None).await {
        Ok(p) => println!(
            "  {:<20} {} itens (próx: {}) em {:.1?}",
            "curtidas",
            p.items.len(),
            p.continuation.is_some(),
            t.elapsed()
        ),
        Err(e) => println!("  {:<20} ERRO em {:.1?}: {e}", "curtidas", t.elapsed()),
    }

    Ok(())
}
