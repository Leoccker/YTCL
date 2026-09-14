<script lang="ts">
  /**
   * Tela da fila: faixa atual, "a seguir" (reordenável por arrastar) e
   * "já tocadas" (recolhida por padrão). Ocupa 100% da altura do contêiner
   * `.view`/`.page` do App e rola por conta própria (nunca a página toda —
   * ver `theme.css`).
   */
  import Art from "../components/Art.svelte";
  import TrackRow from "../components/TrackRow.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import VirtualList from "../components/VirtualList.svelte";
  import { player } from "../stores/player.svelte";
  import type { QueueEntry } from "../api";

  /** Acima disto a lista "A seguir" vira virtualizada. */
  const VIRTUALIZE_THRESHOLD = 200;
  const ROW_H = 56;

  const currentIndex = $derived(player.queue.findIndex((e) => e.isCurrent));
  const currentEntry = $derived(currentIndex >= 0 ? player.queue[currentIndex]! : null);
  const past = $derived(currentIndex > 0 ? player.queue.slice(0, currentIndex) : []);

  // "A seguir" vive numa cópia local para suportar reordenação otimista: o
  // arraste atualiza `upcoming` na hora, sem esperar a volta do backend. Este
  // efeito ressincroniza a partir de `player.queue` sempre que ela mudar de
  // verdade (evento queue_changed já recarregado pelo store) — o que também
  // é o caminho que confirma (ou corrige) o que o arraste presumiu.
  let upcoming = $state<QueueEntry[]>([]);
  $effect(() => {
    const idx = player.queue.findIndex((e) => e.isCurrent);
    upcoming = idx >= 0 ? player.queue.slice(idx + 1) : player.queue.slice();
  });

  let pastOpen = $state(false);

  // --- arrastar para reordenar ("a seguir") ---------------------------------
  //
  // `dragFrom` é o índice local (dentro de `upcoming`) da linha sendo
  // arrastada; `dropIndex` é o índice local "inserir antes de" indicado pela
  // linha de soltura — pode ir até `upcoming.length` (soltar depois da
  // última linha).
  let dragFrom = $state<number | null>(null);
  let dropIndex = $state<number | null>(null);

  function onRowDragStart(e: DragEvent, i: number) {
    dragFrom = i;
    dropIndex = i;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      // Firefox exige algum dado setado para o arraste funcionar.
      e.dataTransfer.setData("text/plain", String(i));
    }
  }

  function onRowDragOver(e: DragEvent, i: number) {
    if (dragFrom === null) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const before = e.clientY < rect.top + rect.height / 2;
    dropIndex = before ? i : i + 1;
  }

  /** Soltar no espaço vazio abaixo da última linha: manda para o fim. */
  function onContainerDragOver(e: DragEvent) {
    if (dragFrom === null) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    dropIndex = upcoming.length;
  }

  function resetDrag() {
    dragFrom = null;
    dropIndex = null;
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    const from = dragFrom;
    const to = dropIndex;
    resetDrag();
    if (from === null || to === null) return;
    // Soltar em cima de si mesma (antes ou depois, sem mudar nada) não move.
    if (to === from || to === from + 1) return;
    applyMove(from, to);
  }

  /**
   * Aplica o arraste: `localFrom` é a posição de origem em `upcoming` e
   * `insertBefore` é "inserir antes desta posição", ambos medidos no array
   * ANTES da remoção — é assim que a UI calcula a linha de soltura.
   *
   * O backend (`queue.rs::move_item`) remove em `from` e só then insere em
   * `to`, e `to` já é o índice DEPOIS da remoção — não antes. Por isso a
   * conversão abaixo: se o destino vem depois da origem, ele desloca uma
   * posição pra trás porque o item de origem já saiu do meio do caminho.
   *
   * Exemplo: fila com 5 faixas, tocando o índice 1 (real). "A seguir" =
   * índices reais [2, 3, 4], ou seja, locais [0, 1, 2]. Arrastando o local 0
   * para depois do local 2 (soltar no fim da lista): localFrom=0,
   * insertBefore=3 (upcoming.length). base = currentIndex+1 = 2.
   * realFrom = 2+0 = 2. realInsertBefore = 2+3 = 5.
   * Como 5 > 2, realTo = 5-1 = 4. Chama moveInQueue(2, 4): o backend remove o
   * item do índice 2 (os que estavam em 3 e 4 descem para 2 e 3) e insere no
   * índice 4 do array já sem ele — ou seja, no fim. Resultado correto.
   */
  function applyMove(localFrom: number, insertBefore: number) {
    // Otimista: já reordena a cópia local, sem esperar o backend confirmar.
    const copy = upcoming.slice();
    const [item] = copy.splice(localFrom, 1);
    const localTo = insertBefore > localFrom ? insertBefore - 1 : insertBefore;
    copy.splice(localTo, 0, item!);
    upcoming = copy;

    const base = currentIndex + 1;
    const realFrom = base + localFrom;
    const realInsertBefore = base + insertBefore;
    const realTo = realInsertBefore > realFrom ? realInsertBefore - 1 : realInsertBefore;
    player.moveInQueue(realFrom, realTo);
  }

  function artistNames(entry: QueueEntry): string {
    return entry.track.artists.map((a) => a.name).join(", ");
  }
</script>

{#snippet upcomingRow(entry: QueueEntry, i: number)}
  <div
    class="qrow"
    class:drop-before={dropIndex === i}
    class:drop-after={dropIndex === upcoming.length && i === upcoming.length - 1}
    class:dragging={dragFrom === i}
    draggable="true"
    role="listitem"
    ondragstart={(e) => onRowDragStart(e, i)}
    ondragover={(e) => onRowDragOver(e, i)}
    ondrop={onDrop}
    ondragend={resetDrag}
  >
    <span class="handle" title="Arrastar para reordenar" aria-hidden="true">⠿</span>
    <button class="qrow-main" onclick={() => player.jumpTo(currentIndex + 1 + i)}>
      <Art art={entry.track.art} size={40} alt="" />
      <span class="text">
        <span class="title">{entry.track.title}</span>
        <span class="subtitle">{artistNames(entry)}</span>
      </span>
    </button>
  </div>
{/snippet}

<div class="queue-view">
  <header class="head">
    <h1>Fila</h1>
    <span class="count">{player.queue.length} faixa{player.queue.length === 1 ? "" : "s"}</span>
  </header>

  {#if player.queue.length === 0}
    <EmptyState title="A fila está vazia" message="Toque uma música para começar." />
  {:else}
    <div class="sections">
      {#if currentEntry}
        <section class="current-section">
          <h2>Tocando agora</h2>
          <TrackRow track={currentEntry.track} active onPlay={() => {}} />
        </section>
      {/if}

      {#if past.length > 0}
        <section class="past-section">
          <button class="section-toggle" onclick={() => (pastOpen = !pastOpen)}>
            <span class="chevron" class:open={pastOpen}>▸</span>
            Já tocadas ({past.length})
          </button>
          {#if pastOpen}
            <div class="past-list">
              {#each past as entry, i (entry.track.id + "-" + i)}
                <TrackRow track={entry.track} onPlay={() => player.jumpTo(i)} />
              {/each}
            </div>
          {/if}
        </section>
      {/if}

      <section class="upcoming-section">
        <h2>A seguir ({upcoming.length})</h2>
        {#if upcoming.length === 0}
          <p class="hint">Nada depois desta faixa.</p>
        {:else if upcoming.length > VIRTUALIZE_THRESHOLD}
          <div class="upcoming-wrap virtual" role="list" ondragover={onContainerDragOver} ondrop={onDrop}>
            <VirtualList items={upcoming} itemHeight={ROW_H} row={upcomingRow} />
          </div>
        {:else}
          <div class="upcoming-wrap plain" role="list" ondragover={onContainerDragOver} ondrop={onDrop}>
            {#each upcoming as entry, i (entry.track.id + "-" + i)}
              {@render upcomingRow(entry, i)}
            {/each}
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .queue-view {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .head {
    flex: none;
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-6) var(--space-2);
  }
  .head h1 {
    margin: 0;
    font-size: 20px;
  }
  .head .count {
    color: var(--text-dim);
    font-size: 13px;
  }

  .sections {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 0 var(--space-4) var(--space-4);
    gap: var(--space-4);
  }

  section h2 {
    margin: 0 0 var(--space-1);
    padding: 0 var(--space-2);
    font-size: 13px;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .current-section,
  .past-section {
    flex: none;
  }

  .section-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    font-size: 13px;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .section-toggle:hover {
    color: var(--text);
  }
  .chevron {
    display: inline-block;
    transition: transform 0.15s ease;
    font-size: 10px;
  }
  .chevron.open {
    transform: rotate(90deg);
  }

  .past-list {
    max-height: 300px;
    overflow-y: auto;
  }

  .hint {
    padding: var(--space-2);
    color: var(--text-faint);
    font-size: 13px;
  }

  .upcoming-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .upcoming-wrap {
    flex: 1;
    min-height: 0;
  }
  .upcoming-wrap.plain {
    overflow-y: auto;
  }
  .upcoming-wrap.virtual {
    overflow: hidden;
  }

  .qrow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 56px;
    box-sizing: border-box;
    border-top: 2px solid transparent;
    border-bottom: 2px solid transparent;
  }
  .qrow.dragging {
    opacity: 0.4;
  }
  .qrow.drop-before {
    border-top-color: var(--accent);
  }
  .qrow.drop-after {
    border-bottom-color: var(--accent);
  }

  .handle {
    flex: none;
    width: var(--space-6);
    text-align: center;
    color: var(--text-faint);
    cursor: grab;
    font-size: 14px;
  }
  .handle:active {
    cursor: grabbing;
  }

  .qrow-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 100%;
    padding: 0 var(--space-3) 0 0;
    border-radius: var(--radius-sm);
  }
  .qrow-main:hover {
    background: var(--bg-hover);
  }

  .qrow-main .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    text-align: left;
  }
  .qrow-main .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .qrow-main .subtitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
    font-size: 12px;
  }
</style>
