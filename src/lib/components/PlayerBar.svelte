<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Art from "./Art.svelte";
  import { player } from "../stores/player.svelte";
  import { router } from "../router.svelte";

  function fmt(secs: number): string {
    if (!isFinite(secs) || secs < 0) return "0:00";
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${String(s).padStart(2, "0")}`;
  }

  // Barra de progresso: interpola entre os eventos de posição (~4 Hz) para
  // não andar aos saltos.
  let uiPos = $state(0);
  onMount(() => {
    let raf = 0;
    const tick = () => {
      uiPos = player.displayPosition(performance.now());
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  let scrubbing = $state(false);
  let scrubValue = $state(0);

  const shownPos = $derived(scrubbing ? scrubValue : uiPos);
  const pct = $derived(
    player.duration > 0 ? (shownPos / player.duration) * 100 : 0,
  );

  function onScrubInput(e: Event) {
    scrubbing = true;
    scrubValue = Number((e.target as HTMLInputElement).value);
  }
  function onScrubCommit() {
    player.seek(scrubValue);
    scrubbing = false;
  }

  const repeatIcon = $derived(
    { off: "↻", all: "🔁", one: "🔂" }[player.repeat],
  );

  // --- navegação a partir do título/artistas/capa --------------------------

  function goAlbum() {
    const id = player.current?.album?.id;
    if (id) router.push({ name: "album", id });
  }
  function goArtist(id: string | null) {
    if (id) router.push({ name: "artist", id });
  }

  /** Alterna a tela de fila: fecha se já estiver aberta, senão abre. */
  function toggleQueueView() {
    if (router.current.name === "queue") {
      if (router.canBack) router.back();
      else router.push({ name: "home" });
    } else {
      router.push({ name: "queue" });
    }
  }
  function openQueueView() {
    router.push({ name: "queue" });
  }

  // --- volume ---------------------------------------------------------------
  //
  // O valor exibido (`volValue`) muda a cada evento "input" para o slider
  // responder ao arraste sem atraso. A chamada ao backend, porém, é limitada
  // a ~10/s (throttle de 100ms) — arrastar dispara "input" bem mais rápido
  // que isso, e cada chamada é uma invocação IPC. O valor final ao soltar
  // ("change") sempre é enviado, sem esperar o throttle.

  const VOL_THROTTLE_MS = 100;
  let volDragging = $state(false);
  let volValue = $state(player.volume);
  let volTimer: ReturnType<typeof setTimeout> | null = null;
  let volLastSent = 0;
  let muted = $state(false);
  let volBeforeMute = $state(player.volume || 0.8);

  $effect(() => {
    if (volDragging) return;
    volValue = player.volume;
    // O volume pode subir por fora da barra (Ctrl+↑): aí o som já voltou e
    // a barra não pode continuar dizendo "mudo".
    if (player.volume > 0) muted = false;
  });

  onDestroy(() => {
    if (volTimer) clearTimeout(volTimer);
  });


  const sliderValue = $derived(muted ? 0 : volValue);
  const volumeIcon = $derived(
    muted || sliderValue === 0 ? "🔇" : sliderValue < 0.5 ? "🔉" : "🔊",
  );

  function sendVolumeThrottled(level: number) {
    const now = performance.now();
    if (now - volLastSent >= VOL_THROTTLE_MS) {
      volLastSent = now;
      player.setVolume(level);
    } else if (volTimer === null) {
      volTimer = setTimeout(
        () => {
          volTimer = null;
          volLastSent = performance.now();
          player.setVolume(volValue);
        },
        VOL_THROTTLE_MS - (now - volLastSent),
      );
    }
  }

  function onVolumeInput(e: Event) {
    volDragging = true;
    const level = Number((e.target as HTMLInputElement).value);
    volValue = level;
    if (muted && level > 0) muted = false;
    sendVolumeThrottled(level);
  }

  function onVolumeCommit(e: Event) {
    if (volTimer) {
      clearTimeout(volTimer);
      volTimer = null;
    }
    const level = Number((e.target as HTMLInputElement).value);
    volValue = level;
    if (muted && level > 0) muted = false;
    volLastSent = performance.now();
    player.setVolume(level);
    volDragging = false;
  }

  function toggleMute() {
    if (volTimer) {
      clearTimeout(volTimer);
      volTimer = null;
    }
    if (muted) {
      muted = false;
      player.setVolume(volBeforeMute);
    } else {
      volBeforeMute = volValue > 0 ? volValue : volBeforeMute || 0.8;
      muted = true;
      player.setVolume(0);
    }
  }
</script>

{#if player.error}
  <div class="bar err">
    <span>{player.error}</span>
    <button onclick={() => (player.error = null)}>✕</button>
  </div>
{:else if !player.available}
  <div class="bar unavailable">
    <span>Reprodução indisponível — o libmpv não carregou.</span>
  </div>
{:else if player.hasTrack}
  <div class="bar">
    <div class="meta">
      <button class="art-btn" onclick={openQueueView} title="Abrir fila">
        <Art art={player.current?.art} size={48} alt="" />
      </button>
      <div class="text">
        {#if player.current?.album?.id}
          <button class="title link" onclick={goAlbum}>{player.current.title}</button>
        {:else}
          <span class="title">{player.current?.title}</span>
        {/if}
        <span class="artist">
          {#each player.current?.artists ?? [] as a, i (a.id ?? `${a.name}-${i}`)}
            {#if i > 0}<span class="sep">, </span>{/if}
            {#if a.id}
              <button class="link" onclick={() => goArtist(a.id)}>{a.name}</button>
            {:else}
              <span>{a.name}</span>
            {/if}
          {/each}
        </span>
      </div>
    </div>

    <div class="center">
      <div class="controls">
        <button onclick={() => player.toggleShuffle()} class:on={player.shuffled} title="Aleatório">
          🔀
        </button>
        <button onclick={() => player.prev()} title="Anterior">⏮</button>
        <button class="play" onclick={() => player.toggle()} title="Play/Pause">
          {player.playing ? "⏸" : "▶"}
        </button>
        <button onclick={() => player.next()} title="Próxima">⏭</button>
        <button onclick={() => player.cycleRepeat()} class:on={player.repeat !== "off"} title="Repetir">
          {repeatIcon}
        </button>
      </div>

      <div class="seek">
        <span class="t">{fmt(shownPos)}</span>
        <input
          type="range"
          min="0"
          max={player.duration || 0}
          step="0.1"
          value={shownPos}
          oninput={onScrubInput}
          onchange={onScrubCommit}
          style:--pct="{pct}%"
        />
        <span class="t">{fmt(player.duration)}</span>
      </div>
    </div>

    <div class="right">
      <button
        class="queue-btn"
        class:on={router.current.name === "queue"}
        onclick={toggleQueueView}
        title="Fila"
      >
        ☰
      </button>
      <button class="vol-icon" onclick={toggleMute} title={muted ? "Reativar som" : "Mudo"}>
        {volumeIcon}
      </button>
      <input
        class="vol"
        type="range"
        min="0"
        max="1"
        step="0.01"
        value={sliderValue}
        oninput={onVolumeInput}
        onchange={onVolumeCommit}
        style:--v={sliderValue}
      />
    </div>
  </div>
{:else}
  <div class="bar idle">
    <span>Nada tocando.</span>
  </div>
{/if}

<style>
  .bar {
    display: grid;
    grid-template-columns: 1fr 2fr 1fr;
    align-items: center;
    gap: var(--space-4);
    width: 100%;
    height: 100%;
    padding: 0 var(--space-4);
  }
  .bar.err {
    display: flex;
    justify-content: space-between;
    color: #fca5a5;
    background: var(--accent-dim);
    font-size: 13px;
  }
  .bar.err button {
    color: #fff;
  }
  .bar.idle,
  .bar.unavailable {
    display: flex;
    color: var(--text-faint);
    font-size: 13px;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }
  .art-btn {
    flex: none;
    border-radius: var(--radius-sm);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title,
  .artist {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title {
    font-size: 13px;
    text-align: left;
    max-width: 100%;
  }
  button.title {
    color: var(--text);
  }
  button.title:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .artist {
    font-size: 12px;
    color: var(--text-dim);
  }
  .artist .sep {
    color: var(--text-faint);
  }
  .artist .link {
    color: var(--text-dim);
    font-size: 12px;
  }
  .artist .link:hover {
    color: var(--text);
    text-decoration: underline;
  }

  .center {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
  }
  .controls {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .controls button {
    color: var(--text-dim);
    font-size: 15px;
    line-height: 1;
    padding: var(--space-1);
  }
  .controls button:hover {
    color: var(--text);
  }
  .controls button.on {
    color: var(--accent);
  }
  .controls .play {
    font-size: 20px;
    color: var(--text);
  }

  .seek {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    max-width: 520px;
  }
  .seek .t {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    min-width: 34px;
    text-align: center;
  }

  input[type="range"] {
    -webkit-appearance: none;
    appearance: none;
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--accent) var(--pct, 0%),
      var(--bg-active) var(--pct, 0%)
    );
    cursor: pointer;
  }
  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text);
  }

  .right {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .queue-btn {
    color: var(--text-dim);
    font-size: 15px;
    padding: var(--space-1);
  }
  .queue-btn:hover {
    color: var(--text);
  }
  .queue-btn.on {
    color: var(--accent);
  }
  .vol-icon {
    color: var(--text-dim);
    font-size: 13px;
    padding: var(--space-1);
    line-height: 1;
  }
  .vol-icon:hover {
    color: var(--text);
  }
  .vol {
    width: 90px;
    /* --v é a fração 0..1 do volume atual, vinda do elemento via style: no
       markup — antes ficava sem nenhum valor real definido em lugar nenhum
       e o preenchimento não acompanhava o slider. */
    background: linear-gradient(
      to right,
      var(--text-dim) calc(var(--v, 0.8) * 100%),
      var(--bg-active) calc(var(--v, 0.8) * 100%)
    );
  }
</style>
