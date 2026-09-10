<script lang="ts" generics="T">
  /**
   * Lista virtualizada por janela de rolagem.
   *
   * Renderiza só as linhas visíveis mais uma margem. Sem isto, uma busca com
   * 500 resultados cria 500 nós no DOM e a rolagem cai de 60 fps —
   * exatamente o critério de aceite que este componente existe para proteger.
   *
   * Assume altura de linha fixa. É o caso de todas as listas do app, e permite
   * calcular a janela por aritmética em vez de medir cada elemento.
   */
  import type { Snippet } from "svelte";

  interface Props {
    items: T[];
    itemHeight: number;
    /** Linhas extras acima e abaixo, para a rolagem não mostrar buraco. */
    overscan?: number;
    row: Snippet<[T, number]>;
    /** Chamado ao aproximar do fim — para carregar a próxima página. */
    onEnd?: () => void;
  }

  let { items, itemHeight, overscan = 6, row, onEnd }: Props = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  const start = $derived(
    Math.max(0, Math.floor(scrollTop / itemHeight) - overscan),
  );
  const visibleCount = $derived(
    Math.ceil(viewportHeight / itemHeight) + overscan * 2,
  );
  const end = $derived(Math.min(items.length, start + visibleCount));
  const visible = $derived(items.slice(start, end));
  const totalHeight = $derived(items.length * itemHeight);

  // Dispara uma vez por chegada ao fim: sem esta trava, cada evento de
  // rolagem dentro da zona final pediria a mesma página de novo.
  let endFired = false;

  function onScroll() {
    if (!viewport) return;
    scrollTop = viewport.scrollTop;

    const restante = totalHeight - (scrollTop + viewportHeight);
    if (restante < itemHeight * 10) {
      if (!endFired) {
        endFired = true;
        onEnd?.();
      }
    } else {
      endFired = false;
    }
  }

  $effect(() => {
    if (!viewport) return;
    const ro = new ResizeObserver(() => {
      viewportHeight = viewport?.clientHeight ?? 0;
    });
    ro.observe(viewport);
    viewportHeight = viewport.clientHeight;
    return () => ro.disconnect();
  });

  /** Volta ao topo — usado ao trocar de busca ou de filtro. */
  export function scrollToTop() {
    viewport?.scrollTo({ top: 0 });
    scrollTop = 0;
    endFired = false;
  }
</script>

<div class="viewport" bind:this={viewport} onscroll={onScroll}>
  <div class="spacer" style:height="{totalHeight}px">
    <div class="window" style:transform="translateY({start * itemHeight}px)">
      {#each visible as item, i (start + i)}
        <div class="row" style:height="{itemHeight}px">
          {@render row(item, start + i)}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .viewport {
    height: 100%;
    overflow-y: auto;
    /* Isola o cálculo de layout da lista do resto da página: sem isto, cada
       atualização da janela força o navegador a reavaliar a árvore inteira. */
    contain: strict;
  }
  .spacer {
    position: relative;
    width: 100%;
  }
  .window {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    /* Promove a janela a camada própria: o translate da rolagem passa a ser
       trabalho de composição, não de repaint. */
    will-change: transform;
  }
  .row {
    box-sizing: border-box;
  }
</style>
