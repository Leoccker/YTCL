<script lang="ts" generics="T">
  /**
   * Prateleira horizontal (usada na Início): título + fileira que rola na
   * horizontal com scroll-snap, e setas ‹ › que avançam "uma página" da
   * fileira. Genérico em T para servir qualquer SearchItem sem a Shelf
   * conhecer os quatro tipos — quem chama decide como desenhar o card.
   */
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    items: T[];
    card: Snippet<[T, number]>;
  }

  let { title, items, card }: Props = $props();

  let rowEl = $state<HTMLDivElement | null>(null);
  let atStart = $state(true);
  let atEnd = $state(true);

  function updateEdges() {
    const el = rowEl;
    if (!el) return;
    atStart = el.scrollLeft <= 1;
    atEnd = el.scrollLeft + el.clientWidth >= el.scrollWidth - 1;
  }

  // Listener manual (em vez de `onscroll` no markup) para garantir
  // {passive:true} — regra do projeto. O MutationObserver reavalia as
  // pontas quando os cards trocam (ex.: skeleton -> dados reais), e o
  // ResizeObserver quando a janela muda de tamanho.
  $effect(() => {
    const el = rowEl;
    if (!el) return;

    updateEdges();
    el.addEventListener("scroll", updateEdges, { passive: true });
    const ro = new ResizeObserver(updateEdges);
    ro.observe(el);
    const mo = new MutationObserver(updateEdges);
    mo.observe(el, { childList: true, subtree: true });

    return () => {
      el.removeEventListener("scroll", updateEdges);
      ro.disconnect();
      mo.disconnect();
    };
  });

  function page(dir: 1 | -1) {
    const el = rowEl;
    if (!el) return;
    el.scrollBy({ left: dir * el.clientWidth * 0.9, behavior: "smooth" });
  }
</script>

<section class="shelf">
  <div class="head">
    <h2>{title}</h2>
    <div class="nav">
      <button
        class="nav-btn"
        disabled={atStart}
        onclick={() => page(-1)}
        aria-label="Anterior"
      >
        ‹
      </button>
      <button
        class="nav-btn"
        disabled={atEnd}
        onclick={() => page(1)}
        aria-label="Próximo"
      >
        ›
      </button>
    </div>
  </div>

  <div class="row" bind:this={rowEl}>
    {#each items as item, i (i)}
      <div class="item">
        {@render card(item, i)}
      </div>
    {/each}
  </div>
</section>

<style>
  .shelf {
    padding-bottom: var(--space-2);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--space-6) var(--space-2);
  }

  .head h2 {
    margin: 0;
    font-size: 16px;
  }

  .nav {
    display: flex;
    gap: var(--space-1);
  }

  .nav-btn {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-dim);
    font-size: 16px;
    line-height: 1;
  }
  .nav-btn:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .nav-btn:disabled {
    color: var(--text-faint);
    opacity: 0.4;
  }

  .row {
    display: flex;
    gap: var(--space-3);
    overflow-x: auto;
    overflow-y: hidden;
    scroll-snap-type: x proximity;
    padding: 0 var(--space-6) var(--space-1);
    /* Sem barra de rolagem visível: a fileira já tem as setas para isso. */
    scrollbar-width: none;
  }
  .row::-webkit-scrollbar {
    display: none;
  }

  .item {
    flex: none;
    width: 160px;
    scroll-snap-align: start;
    /* Fora da tela não custa layout/paint — a fileira pode ter muitos cards. */
    content-visibility: auto;
    contain-intrinsic-size: 160px 220px;
  }
</style>
