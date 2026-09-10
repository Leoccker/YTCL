//! Toca áudio de verdade por alguns segundos. `#[ignore]`: precisa de rede,
//! do libmpv e de uma saída de áudio.
//!
//! `cargo test -p ytcl-player --test playback_live -- --ignored`

use std::time::Duration;

use tokio::sync::mpsc;
use ytcl_core::innertube::InnerTube;
use ytcl_core::stream::StreamResolver;
use ytcl_player::{Backend, BackendEvent, MpvBackend};

#[tokio::test]
#[ignore = "precisa de rede + libmpv + áudio"]
async fn resolve_e_toca_avancando_a_posicao() {
    let dir = std::env::temp_dir().join("ytcl-playback-test");
    std::fs::create_dir_all(&dir).unwrap();
    let it = InnerTube::new(&dir).expect("innertube");

    let stream = it.resolve("dQw4w9WgXcQ").await.expect("resolver");
    assert!(stream.url.starts_with("https://"));

    let (tx, mut rx) = mpsc::unbounded_channel::<BackendEvent>();
    let mpv = MpvBackend::new(tx).expect("mpv");

    mpv.set_volume(0.2).await.unwrap();
    mpv.load(&stream.url, stream.loudness_db).await.unwrap();
    mpv.play().await.unwrap();

    // Espera a posição passar de 1.5s dentro de 15s.
    let mut max_pos = 0.0f64;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
            Ok(Some(BackendEvent::Position { secs, .. })) => {
                max_pos = max_pos.max(secs);
                if max_pos > 1.5 {
                    break;
                }
            }
            Ok(Some(BackendEvent::Error { message })) => panic!("erro de reprodução: {message}"),
            Ok(Some(_)) => {}
            Ok(None) => panic!("canal fechou"),
            Err(_) => {}
        }
    }

    assert!(max_pos > 1.5, "posição não avançou (chegou só a {max_pos:.1}s)");

    // Seek e pause funcionam.
    mpv.seek(30.0).await.unwrap();
    mpv.pause().await.unwrap();
    mpv.stop().await.unwrap();
}
