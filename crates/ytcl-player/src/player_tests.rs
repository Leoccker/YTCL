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

impl ResolveCall {
    /// Responde à resolução, tolerando que ela já tenha sido cancelada —
    /// é justamente o que vários testes provocam de propósito.
    fn answer(self, result: Result<ResolvedStream>) {
        let _ = self.reply.send(result);
    }

    /// O player largou o futuro da resolução (com yt-dlp de verdade, o
    /// subprocesso teria morrido junto).
    fn abandoned(&self) -> bool {
        self.reply.is_closed()
    }
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
        self.call(ids[0]).await.answer(Ok(stream(ids[0])));
        task.await.unwrap().unwrap();
    }

    async fn append(&mut self, id: &str) {
        let task = self.prefetch();
        self.call(id).await.answer(Ok(stream(id)));
        task.await.unwrap();
    }
}

#[tokio::test]
async fn ultima_selecao_vence_mesmo_se_a_anterior_termina_depois() {
    let mut rig = Rig::new();
    let first = rig.play(&["A"]);
    let a = rig.call("A").await;
    rig.start(&["B"]).await;
    a.answer(Ok(stream("A")));
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
    a.answer(Err(CoreError::NotFound));
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
    rig.call("A").await.answer(Ok(stream("URL-nova")));
    second.await.unwrap().unwrap();
    old.answer(Ok(stream("URL-antiga")));
    first.await.unwrap().unwrap();
    assert_eq!(*rig.backend.loaded.lock(), ["URL-nova"]);
}

#[tokio::test]
async fn proxima_no_fim_cancela_carregamento_pendente() {
    let mut rig = Rig::new();
    let task = rig.play(&["A"]);
    let old = rig.call("A").await;
    rig.player.next().await.unwrap();
    old.answer(Ok(stream("A")));
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
    b.answer(Ok(stream("B")));
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
    b.answer(Ok(stream("B")));
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
    pending.answer(Ok(stream("B")));
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
    rig.call("A").await.answer(Ok(stream("A-renovada")));
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
    old.answer(Ok(stream("A-renovada")));
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
    pending.answer(Ok(stream("B")));
}

// --- prazo, cancelamento e espera entre tentativas (item 3) --------------

#[tokio::test]
async fn mudanca_de_fila_abandona_a_pre_resolucao_em_voo() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    let task = rig.prefetch();
    let pendente = rig.call("B").await;
    rig.player.play_next(track("C")).await.unwrap();
    task.await.unwrap();
    assert!(
        pendente.abandoned(),
        "a resolução de B continuou rodando depois da fila mudar"
    );
}

#[tokio::test]
async fn nova_selecao_abandona_a_resolucao_da_anterior() {
    let mut rig = Rig::new();
    let primeira = rig.play(&["A"]);
    let pendente = rig.call("A").await;
    rig.start(&["B"]).await;
    primeira.await.unwrap().unwrap();
    assert!(
        pendente.abandoned(),
        "a resolução de A continuou rodando depois de trocar para B"
    );
}

#[tokio::test]
async fn enfileirar_nao_derruba_a_faixa_que_esta_carregando() {
    let mut rig = Rig::new();
    let task = rig.play(&["A"]);
    let pendente = rig.call("A").await;
    // Mexer na fila cancela a pré-resolução, nunca o que o usuário mandou
    // tocar e ainda está resolvendo.
    rig.player.enqueue(track("B")).await.unwrap();
    assert!(
        !pendente.abandoned(),
        "enfileirar cancelou o carregamento da faixa atual"
    );
    pendente.answer(Ok(stream("A")));
    task.await.unwrap().unwrap();
    assert_eq!(*rig.backend.loaded.lock(), ["A"]);
    assert_eq!(rig.player.snapshot().current.unwrap().id, "A");
    assert_eq!(rig.player.snapshot().queue.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn pre_resolucao_pendurada_e_abandonada_no_prazo() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;
    let task = rig.prefetch();
    let pendurada = rig.call("B").await;
    // Ninguém responde: só o prazo encerra a tentativa.
    task.await.unwrap();
    assert!(pendurada.abandoned(), "o prazo não encerrou a resolução");
    assert_eq!(*rig.backend.playlist.lock(), ["A"]);
    assert!(rig.player.inner.lock().prefetching.is_none());
}

#[tokio::test(start_paused = true)]
async fn resolucao_pendurada_da_faixa_atual_avisa_o_usuario() {
    let mut rig = Rig::new();
    let task = rig.play(&["A"]);
    let pendurada = rig.call("A").await;
    task.await.unwrap().unwrap();
    assert!(pendurada.abandoned(), "o prazo não encerrou a resolução");
    assert_eq!(rig.player.snapshot().state, PlaybackState::Idle);
    assert!(!rig.player.inner.lock().loading);
    let erro = std::iter::from_fn(|| rig.events.try_recv().ok())
        .any(|event| matches!(event, PlayerEvent::Error { .. }));
    assert!(erro, "o usuário ficou sem explicação para a faixa parada");
}

#[tokio::test(start_paused = true)]
async fn falha_na_pre_resolucao_espera_antes_de_tentar_de_novo() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;

    let task = rig.prefetch();
    rig.call("B").await.answer(Err(CoreError::NotFound));
    task.await.unwrap();

    // As posições seguintes (~4 Hz) não podem gerar um yt-dlp cada.
    for _ in 0..3 {
        rig.prefetch().await.unwrap();
    }
    assert!(
        rig.calls.try_recv().is_err(),
        "tentou de novo sem esperar o intervalo"
    );

    tokio::time::advance(PREFETCH_RETRY_BASE).await;
    rig.append("B").await;
    assert_eq!(*rig.backend.playlist.lock(), ["A", "B"]);
    assert!(
        rig.player.inner.lock().prefetch_retry.is_none(),
        "o sucesso deveria zerar a espera"
    );
}

#[tokio::test(start_paused = true)]
async fn a_espera_cresce_a_cada_falha_da_mesma_faixa() {
    let mut rig = Rig::new();
    rig.start(&["A", "B"]).await;

    for esperado in [PREFETCH_RETRY_BASE, PREFETCH_RETRY_BASE * 2] {
        let task = rig.prefetch();
        rig.call("B").await.answer(Err(CoreError::NotFound));
        task.await.unwrap();

        // Um instante antes do prazo ainda não tenta...
        tokio::time::advance(esperado - std::time::Duration::from_millis(1)).await;
        rig.prefetch().await.unwrap();
        assert!(rig.calls.try_recv().is_err(), "tentou cedo demais");
        // ...e logo depois, sim.
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
    }

    rig.append("B").await;
    assert_eq!(*rig.backend.playlist.lock(), ["A", "B"]);
}

#[test]
fn a_espera_dobra_ate_o_teto() {
    assert_eq!(retry_delay(1), PREFETCH_RETRY_BASE);
    assert_eq!(retry_delay(2), PREFETCH_RETRY_BASE * 2);
    assert_eq!(retry_delay(3), PREFETCH_RETRY_BASE * 4);
    assert_eq!(retry_delay(50), PREFETCH_RETRY_MAX);
    // `failures` nunca é zero, mas a conta não pode estourar se for.
    assert_eq!(retry_delay(0), PREFETCH_RETRY_BASE);
}
