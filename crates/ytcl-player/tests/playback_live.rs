//! Toca áudio de verdade por alguns segundos. `#[ignore]`: precisa de rede,
//! do libmpv e de uma saída de áudio.
//!
//! `cargo test -p ytcl-player --test playback_live -- --ignored`

use std::time::Duration;

use tokio::sync::mpsc;
use ytcl_core::stream::StreamResolver;
use ytcl_core::ytdlp::YtDlp;
use ytcl_player::{Backend, BackendEvent, MpvBackend};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "precisa de rede + libmpv + áudio"]
async fn resolve_e_toca_avancando_a_posicao() {
    let stream = YtDlp::default().resolve(&std::env::var("VID").unwrap_or_else(|_| "dQw4w9WgXcQ".into())).await.expect("resolver");
    assert!(stream.url.starts_with("https://"));

    let (tx, mut rx) = mpsc::unbounded_channel::<BackendEvent>();
    let mpv = MpvBackend::new(tx).expect("mpv");

    mpv.set_volume(0.2).await.unwrap();
    mpv.load(&stream, stream.loudness_db).await.unwrap();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "precisa de rede + libmpv + áudio"]
async fn seek_para_frente_nao_pula_a_faixa() {
    let vid = std::env::var("VID").unwrap_or_else(|_| "dQw4w9WgXcQ".into());
    let stream = YtDlp::default().resolve(&vid).await.unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel::<BackendEvent>();
    let mpv = MpvBackend::new(tx).unwrap();
    mpv.set_volume(0.05).await.unwrap();
    mpv.load(&stream, None).await.unwrap();
    mpv.play().await.unwrap();

    // espera começar
    let dl = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < dl {
        if let Ok(Some(BackendEvent::Position { secs, .. })) =
            tokio::time::timeout(Duration::from_secs(2), rx.recv()).await
        {
            if secs > 1.0 { break; }
        }
    }

    // seek para 90s e confirma que a posição fica lá (não volta a 0 nem
    // dispara um Ended)
    mpv.seek(90.0).await.unwrap();
    let mut ended = false;
    let mut pos_after = 0.0;
    let dl = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < dl {
        match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
            Ok(Some(BackendEvent::Position { secs, .. })) => {
                pos_after = secs;
                if secs > 92.0 { break; }
            }
            Ok(Some(BackendEvent::Ended { .. })) => { ended = true; break; }
            Ok(Some(BackendEvent::Error { message })) => panic!("erro no seek: {message}"),
            _ => {}
        }
    }
    assert!(!ended, "seek disparou Ended (a faixa 'pulou')");
    assert!(pos_after > 85.0, "seek não levou a 90s (ficou em {pos_after:.1}s)");
    mpv.stop().await.unwrap();
}
