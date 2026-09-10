<script lang="ts">
  import { onMount } from "svelte";
  import Art from "./Art.svelte";
  import { player } from "../stores/player.svelte";

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
      <Art art={player.current?.art} size={48} alt="" />
      <div class="text">
        <span class="title">{player.current?.title}</span>
        <span class="artist">
          {player.current?.artists.map((a) => a.name).join(", ")}
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
      <span class="vol-icon">🔊</span>
      <input
        class="vol"
        type="range"
        min="0"
        max="1"
        step="0.01"
        value={player.volume}
        oninput={(e) => player.setVolume(Number((e.target as HTMLInputElement).value))}
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
  }
  .artist {
    font-size: 12px;
    color: var(--text-dim);
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
  .vol-icon {
    font-size: 13px;
  }
  .vol {
    width: 90px;
    background: linear-gradient(
      to right,
      var(--text-dim) calc(var(--v, 0.8) * 100%),
      var(--bg-active) 0
    );
  }
</style>
