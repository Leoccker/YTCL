use super::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::oneshot;
use ytcl_core::error::{CoreError, Result};
use ytcl_core::stream::AudioCodec;

#[derive(Default)]
struct FakeBackend {
    playlist: Mutex<Vec<String>>,
    loaded: Mutex<Vec<String>>,
    seeks: Mutex<Vec<f64>>,
    fail_clear: AtomicBool,
}

#[async_trait]
impl Backend for FakeBackend {
    async fn load(&self, stream: &ResolvedStream, _: Option<f64>) -> anyhow::Result<()> {
        *self.playlist.lock() = vec![stream.url.clone()];
        self.loaded.lock().push(stream.url.clone());
        Ok(())
    }
    async fn append(&self, stream: &ResolvedStream, _: Option<f64>) -> anyhow::Result<()> {
        self.playlist.lock().push(stream.url.clone());
        Ok(())
    }
    async fn clear_next(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.fail_clear.load(Ordering::SeqCst),
            "falha de playlist-clear"
        );
        self.playlist.lock().truncate(1);
        Ok(())
    }
    async fn stop(&self) -> anyhow::Result<()> {
        self.playlist.lock().clear();
        Ok(())
    }
    async fn play(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn pause(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn seek(&self, secs: f64) -> anyhow::Result<()> {
        self.seeks.lock().push(secs);
        Ok(())
    }
    async fn set_volume(&self, _: f64) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct ResolveCall {
    id: String,
    reply: oneshot::Sender<Result<ResolvedStream>>,
}

struct ControlledResolver(mpsc::UnboundedSender<ResolveCall>);

#[async_trait]
impl StreamResolver for ControlledResolver {
    async fn resolve(&self, id: &str) -> Result<ResolvedStream> {
        let (reply, response) = oneshot::channel();
        self.0
            .send(ResolveCall {
                id: id.into(),
                reply,
            })
            .unwrap();
        response.await.unwrap()
    }
}

fn track(id: &str) -> Track {
    Track {
        id: id.into(),
        title: id.into(),
        artists: vec![],
        album: None,
        duration_secs: Some(180),
        art: None,
        is_explicit: false,
        set_video_id: None,
    }
}

fn stream(id: &str) -> ResolvedStream {
    ResolvedStream {
        url: id.into(),
        codec: AudioCodec::Opus,
        bitrate: 128_000,
        expires_at: u64::MAX,
        loudness_db: None,
        duration_secs: Some(180),
        user_agent: None,
        size: 1000,
    }
}

struct Rig {
    player: Arc<Player>,
    backend: Arc<FakeBackend>,
    calls: mpsc::UnboundedReceiver<ResolveCall>,
    events: mpsc::UnboundedReceiver<PlayerEvent>,
    backend_events: mpsc::UnboundedSender<BackendEvent>,
}

impl Rig {
    fn new() -> Self {
        let backend = Arc::new(FakeBackend::default());
        let (calls_tx, calls) = mpsc::unbounded_channel();
        let (backend_events, backend_rx) = mpsc::unbounded_channel();
        let (to_ui, events) = mpsc::unbounded_channel();
        let player = Player::new(
            backend.clone(),
            Arc::new(ControlledResolver(calls_tx)),
            backend_rx,
            to_ui,
        );
        Self {
            player,
            backend,
            calls,
            events,
            backend_events,
        }
    }

    async fn call(&mut self, expected: &str) -> ResolveCall {
        let call = tokio::time::timeout(std::time::Duration::from_secs(2), self.calls.recv())
            .await
            .expect("resolução não começou")
            .unwrap();
        assert_eq!(call.id, expected);
        call
    }

    fn play(&self, ids: &[&str]) -> tokio::task::JoinHandle<anyhow::Result<()>> {
        let tracks = ids.iter().map(|id| track(id)).collect();
        let player = self.player.clone();
        tokio::spawn(async move { player.play_tracks(tracks, 0).await })
    }

    fn prefetch(&self) -> tokio::task::JoinHandle<()> {
        let player = self.player.clone();
        let generation = player.inner.lock().generation;
        tokio::spawn(async move { player.maybe_prefetch(generation, 165.0, 180.0).await })
    }

    async fn start(&mut self, ids: &[&str]) {
        let task = self.play(ids);
        self.call(ids[0])
            .await
            .reply
            .send(Ok(stream(ids[0])))
            .unwrap();
        task.await.unwrap().unwrap();
    }

    async fn append(&mut self, id: &str) {
        let task = self.prefetch();
        self.call(id).await.reply.send(Ok(stream(id))).unwrap();
        task.await.unwrap();
    }
}

#[tokio::test]
async fn ultima_selecao_vence_mesmo_se_a_anterior_termina_depois() {
    let mut rig = Rig::new();
    let first = rig.play(&["A"]);
    let a = rig.call("A").await;
    rig.start(&["B"]).await;
    a.reply.send(Ok(stream("A"))).unwrap();
    first.await.unwrap().unwrap();
    assert_eq!(*rig.backend.loaded.lock(), ["B"]);
    assert_eq!(rig.player.snapshot().current.unwrap().id, "B");
}

#[tokio::test]
async fn erro_antigo_nao_muda_estado_nem_emite_erro_na_selecao_nova() {
    let mut rig = Rig::new();
    let first = rig.play(&["A"]);
    let a = rig.call("A").await;
    rig.start(&["B"]).await;
    rig.player.set_state(PlaybackState::Playing);
    while rig.events.try_recv().is_ok() {}
    a.reply.send(Err(CoreError::NotFound)).unwrap();
    first.await.unwrap().unwrap();
    assert_eq!(rig.player.snapshot().state, PlaybackState::Playing);
    assert!(rig.events.try_recv().is_err());
}

#[tokio::test]
async fn selecionar_o_mesmo_id_tambem_invalida_o_pedido_anterior() {
    let mut rig = Rig::new();
    let first = rig.play(&["A"]);
    let old = rig.call("A").await;
    let second = rig.play(&["A"]);
    rig.call("A")
        .await
        .reply
        .send(Ok(stream("URL-nova")))
        .unwrap();
    second.await.unwrap().unwrap();
    old.reply.send(Ok(stream("URL-antiga"))).unwrap();
    first.await.unwrap().unwrap();
    assert_eq!(*rig.backend.loaded.lock(), ["URL-nova"]);
}

#[tokio::test]
async fn proxima_no_fim_cancela_carregamento_pendente() {
    let mut rig = Rig::new();
    let task = rig.play(&["A"]);
    let old = rig.call("A").await;
    rig.player.next().await.unwrap();
    old.reply.send(Ok(stream("A"))).unwrap();
    task.await.unwrap().unwrap();
    assert!(rig.backend.loaded.lock().is_empty());
    assert_eq!(rig.player.snapshot().state, PlaybackState::Idle);
}

#[tokio::test]
async fn tocar_em_seguida_substitui_a_pre_carga_real() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    rig.append("B").await;
    rig.player.play_next(track("C")).await.unwrap();
    assert_eq!(*rig.backend.playlist.lock(), ["A"]);
    rig.append("C").await;
    assert_eq!(*rig.backend.playlist.lock(), ["A", "C"]);
    let generation = rig.player.inner.lock().generation;
    rig.backend.playlist.lock().remove(0); // EOF de A: mpv já emendou C.
    rig.player.on_track_finished(generation).await;
    assert_eq!(rig.player.snapshot().current.unwrap().id, "C");
    assert_eq!(*rig.backend.loaded.lock(), ["A"], "gapless não recarrega C");
}

#[tokio::test]
async fn mudar_fila_descarta_pre_resolucao_ainda_em_voo() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    let task = rig.prefetch();
    let b = rig.call("B").await;
    rig.player.play_next(track("C")).await.unwrap();
    b.reply.send(Ok(stream("B"))).unwrap();
    task.await.unwrap();
    assert_eq!(*rig.backend.playlist.lock(), ["A"]);
    rig.append("C").await;
    assert_eq!(*rig.backend.playlist.lock(), ["A", "C"]);
}

#[tokio::test]
async fn trocar_faixa_descarta_pre_resolucao_da_fila_antiga() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    let task = rig.prefetch();
    let b = rig.call("B").await;
    rig.start(&["D", "E"]).await;
    b.reply.send(Ok(stream("B"))).unwrap();
    task.await.unwrap();
    assert_eq!(*rig.backend.playlist.lock(), ["D"]);
}

#[tokio::test]
async fn posicoes_repetidas_nao_duplicam_a_pre_resolucao_pendente() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    let task = rig.prefetch();
    let pending = rig.call("B").await;
    rig.prefetch().await.unwrap();
    assert!(rig.calls.try_recv().is_err());
    pending.reply.send(Ok(stream("B"))).unwrap();
    task.await.unwrap();
    assert_eq!(*rig.backend.playlist.lock(), ["A", "B"]);
}

#[tokio::test]
async fn fim_de_faixa_de_geracao_antiga_nao_avanca_a_fila_nova() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    let generation = rig.player.inner.lock().generation;
    rig.start(&["C", "D"]).await;
    rig.player.on_track_finished(generation).await;
    assert_eq!(rig.player.snapshot().current.unwrap().id, "C");
    assert!(rig.calls.try_recv().is_err());
    assert_eq!(*rig.backend.playlist.lock(), ["C"]);
}

#[tokio::test]
async fn repeat_shuffle_reordenacao_e_enqueue_invalidam_pre_carga() {
    let mut rig = Rig::new();
    rig.start(&["A", "B", "C"]).await;
    rig.append("B").await;
    rig.player.set_repeat(RepeatMode::One).await.unwrap();
    assert_eq!(*rig.backend.playlist.lock(), ["A"]);
    rig.append("A").await;
    rig.player.set_repeat(RepeatMode::Off).await.unwrap();
    rig.append("B").await;
    rig.player.move_in_queue(2, 1).await.unwrap();
    rig.append("C").await;
    rig.player.set_shuffle(true).await.unwrap();
    assert_eq!(*rig.backend.playlist.lock(), ["A"]);
    assert!(rig.player.inner.lock().prefetched.is_none());
    rig.player.set_shuffle(false).await.unwrap();
    rig.append("B").await;
    rig.player.enqueue(track("D")).await.unwrap();
    assert_eq!(*rig.backend.playlist.lock(), ["A"]);
}

#[tokio::test]
async fn recuperacao_invalida_pre_carga_e_permite_refaze_la() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    rig.append("B").await;
    rig.player.inner.lock().last_pos = 150.0;
    let player = rig.player.clone();
    let generation = player.inner.lock().generation;
    let recovery = tokio::spawn(async move { player.recover_from_error(generation, "403").await });
    rig.call("A")
        .await
        .reply
        .send(Ok(stream("A-renovada")))
        .unwrap();
    recovery.await.unwrap();
    assert_eq!(*rig.backend.playlist.lock(), ["A-renovada"]);
    assert_eq!(*rig.backend.seeks.lock(), [150.0]);
    assert!(rig.player.inner.lock().prefetched.is_none());
    rig.append("B").await;
    assert_eq!(*rig.backend.playlist.lock(), ["A-renovada", "B"]);
}

#[tokio::test]
async fn recuperacao_atrasada_nao_sobrescreve_nova_selecao() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    let player = rig.player.clone();
    let generation = player.inner.lock().generation;
    let recovery = tokio::spawn(async move { player.recover_from_error(generation, "403").await });
    let old = rig.call("A").await;
    rig.start(&["C"]).await;
    old.reply.send(Ok(stream("A-renovada"))).unwrap();
    recovery.await.unwrap();
    assert_eq!(*rig.backend.loaded.lock(), ["A", "C"]);
    assert!(rig.backend.seeks.lock().is_empty());
}

#[tokio::test]
async fn falha_ao_limpar_backend_nao_aplica_nova_ordem() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    rig.append("B").await;
    rig.backend.fail_clear.store(true, Ordering::SeqCst);
    assert!(rig.player.play_next(track("C")).await.is_err());
    assert_eq!(*rig.backend.playlist.lock(), ["A", "B"]);
    assert_eq!(rig.player.snapshot().queue.len(), 2);
    assert_eq!(rig.player.inner.lock().prefetched.as_ref().unwrap().0, "B");
}

#[tokio::test]
async fn pausa_e_processada_enquanto_pre_resolucao_esta_pendente() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    rig.backend_events
        .send(BackendEvent::Position {
            secs: 165.0,
            duration_secs: 180.0,
        })
        .unwrap();
    let pending = rig.call("B").await;
    rig.backend_events.send(BackendEvent::Paused).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Some(event) = rig.events.recv().await {
            if matches!(
                event,
                PlayerEvent::State {
                    state: PlaybackState::Paused
                }
            ) {
                return;
            }
        }
        panic!("canal de eventos fechado");
    })
    .await
    .expect("pausa ficou bloqueada pela rede");
    pending.reply.send(Ok(stream("B"))).unwrap();
}
