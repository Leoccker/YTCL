<script lang="ts">
  /**
   * Overlay de diagnostico (F3).
   *
   * Existe desde a fase 0 de proposito: os alvos de performance do projeto
   * (60 fps de rolagem, RSS < 200 MB) so sao perseguidos se estiverem
   * visiveis enquanto o codigo cresce.
   */
  import { onMount } from "svelte";
  import { memInfo, type MemInfo } from "../api";

  let visible = $state(false);
  let fps = $state(0);
  let worstFrameMs = $state(0);
  let mem = $state<MemInfo | null>(null);

  function onKey(e: KeyboardEvent) {
    if (e.key === "F3") {
      e.preventDefault();
      visible = !visible;
    }
  }

  onMount(() => {
    let frames = 0;
    let lastSample = performance.now();
    let lastFrame = lastSample;
    let raf = 0;

    const tick = (now: number) => {
      const delta = now - lastFrame;
      lastFrame = now;
      frames++;

      // Ignora o primeiro frame e saltos de aba em segundo plano.
      if (delta < 500) worstFrameMs = Math.max(worstFrameMs, delta);

      if (now - lastSample >= 1000) {
        fps = Math.round((frames * 1000) / (now - lastSample));
        frames = 0;
        lastSample = now;
        worstFrameMs = 0; // janela deslizante de 1s
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);

    // Memoria muda devagar; 2s basta e nao polui o IPC.
    const memTimer = setInterval(async () => {
      if (visible) mem = await memInfo();
    }, 2000);

    window.addEventListener("keydown", onKey);
    return () => {
      cancelAnimationFrame(raf);
      clearInterval(memTimer);
      window.removeEventListener("keydown", onKey);
    };
  });

  const fpsColor = $derived(fps >= 55 ? "ok" : fps >= 40 ? "warn" : "bad");
  // Alvo do projeto: RSS < 500 MB. RSS e o numero que aparece no monitor do
  // sistema, entao e o que vale como criterio; o PSS fica do lado para
  // diagnosticar (a diferenca e o que os 3 processos compartilham).
  const memColor = $derived(
    !mem?.rssMb ? "dim" : mem.rssMb < 500 ? "ok" : mem.rssMb < 650 ? "warn" : "bad",
  );
</script>

{#if visible}
  <div class="overlay">
    <div class="row">
      <span class="label">fps</span>
      <span class={fpsColor}>{fps}</span>
    </div>
    <div class="row">
      <span class="label">pior frame</span>
      <span class={worstFrameMs > 16.7 ? "warn" : "ok"}>{worstFrameMs.toFixed(1)}ms</span>
    </div>
    <div class="row">
      <span class="label">rss</span>
      <span class={memColor}>
        {mem?.rssMb ? `${mem.rssMb.toFixed(0)}MB` : "—"}
      </span>
    </div>
    <div class="row">
      <span class="label">pss</span>
      <span class="dim">{mem?.pssMb ? `${mem.pssMb.toFixed(0)}MB` : "—"}</span>
    </div>
    <div class="row">
      <span class="label">procs</span>
      <span class="dim">{mem?.processCount ?? "—"}</span>
    </div>
    <div class="hint">F3 fecha</div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    top: var(--space-3);
    right: var(--space-3);
    z-index: 9999;
    padding: var(--space-2) var(--space-3);
    background: rgb(0 0 0 / 0.82);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.6;
    pointer-events: none;
    min-width: 132px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .label {
    color: var(--text-faint);
  }
  .hint {
    margin-top: var(--space-1);
    padding-top: var(--space-1);
    border-top: 1px solid var(--border);
    color: var(--text-faint);
    font-size: 10px;
  }
  .ok {
    color: #4ade80;
  }
  .warn {
    color: #fbbf24;
  }
  .bad {
    color: #f87171;
  }
  .dim {
    color: var(--text-dim);
  }
</style>
