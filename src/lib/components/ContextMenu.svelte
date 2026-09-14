<script lang="ts">
  /**
   * Menu de contexto global — montado uma vez em `App.svelte`. Escuta a
   * store `contextMenu`; qualquer outro componente só chama
   * `contextMenu.open(event, items)`.
   */
  import { tick, onMount } from "svelte";
  import { contextMenu } from "../stores/contextMenu.svelte";

  let menuEl = $state<HTMLDivElement | null>(null);
  let x = $state(0);
  let y = $state(0);
  /** Só fica visível depois de medido e reposicionado — evita o flash no canto errado. */
  let ready = $state(false);

  $effect(() => {
    if (!contextMenu.isOpen) {
      ready = false;
      return;
    }
    x = contextMenu.x;
    y = contextMenu.y;
    ready = false;

    // Espera o menu existir no DOM para medir a largura/altura reais e
    // encaixar dentro da janela.
    void tick().then(() => {
      if (!menuEl || !contextMenu.isOpen) return;
      const rect = menuEl.getBoundingClientRect();
      const margin = 8;
      x = Math.min(x, Math.max(margin, window.innerWidth - rect.width - margin));
      y = Math.min(y, Math.max(margin, window.innerHeight - rect.height - margin));
      ready = true;
    });
  });

  function pick(item: { action: () => void; disabled?: boolean }) {
    if (item.disabled) return;
    item.action();
    contextMenu.close();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") contextMenu.close();
  }

  // Fecha em qualquer clique fora do menu. Checar `contains` em vez de dar
  // `stopPropagation` no próprio menu evita pôr um handler de clique num
  // `<div>` que não é, semanticamente, um elemento interativo.
  function onWindowClick(e: MouseEvent) {
    if (menuEl && e.target instanceof Node && menuEl.contains(e.target)) return;
    contextMenu.close();
  }

  onMount(() => {
    // Scroll não borbulha: só a fase de captura no `window` pega a rolagem
    // de um painel interno (ex.: a VirtualList por trás do menu).
    const onScrollCapture = () => contextMenu.close();
    window.addEventListener("scroll", onScrollCapture, true);
    return () => window.removeEventListener("scroll", onScrollCapture, true);
  });
</script>

<svelte:window onkeydown={onKey} onclick={onWindowClick} onblur={() => contextMenu.close()} />

{#if contextMenu.isOpen}
  <div class="menu" class:ready bind:this={menuEl} style:left="{x}px" style:top="{y}px">
    {#each contextMenu.items as item, i (i)}
      <button class="item" disabled={item.disabled} onclick={() => pick(item)}>
        {item.label}
      </button>
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 10000;
    min-width: 180px;
    padding: var(--space-1);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.4);
    opacity: 0;
  }
  .menu.ready {
    opacity: 1;
  }

  .item {
    display: block;
    width: 100%;
    text-align: left;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    font-size: 13px;
    color: var(--text);
  }
  .item:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .item:disabled {
    color: var(--text-faint);
    cursor: default;
  }
</style>
