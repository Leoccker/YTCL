<script lang="ts">
  import VirtualList from "../components/VirtualList.svelte";
  import ResultRow from "../components/ResultRow.svelte";
  import { player } from "../stores/player.svelte";
  import {
    search,
    searchMore,
    errorMessage,
    type SearchFilter,
    type SearchItem,
  } from "../api";

  const ROW_HEIGHT = 56;

  const filters: { id: SearchFilter; label: string }[] = [
    { id: "all", label: "Tudo" },
    { id: "songs", label: "Músicas" },
    { id: "albums", label: "Álbuns" },
    { id: "artists", label: "Artistas" },
    { id: "playlists", label: "Playlists" },
  ];

  let query = $state("");
  let filter = $state<SearchFilter>("all");
  let items = $state<SearchItem[]>([]);
  let continuation = $state<string | null>(null);
  let loading = $state(false);
  let loadingMore = $state(false);
  let error = $state<string | null>(null);
  let searched = $state(false);
  // Só o método que usamos: descreve o contrato de verdade, em vez de um
  // ReturnType que o TypeScript aceitaria como qualquer coisa.
  let list = $state<{ scrollToTop: () => void } | null>(null);

  /**
   * Cada busca recebe um número; só a mais recente pode escrever o resultado.
   * Sem isto, digitar rápido faz uma resposta antiga chegar depois da nova e
   * sobrescrever a tela com o resultado errado.
   */
  let epoch = 0;

  async function run(refresh = false) {
    const termo = query.trim();
    if (!termo) {
      items = [];
      continuation = null;
      searched = false;
      return;
    }

    const meu = ++epoch;
    loading = true;
    error = null;

    try {
      const res = await search(termo, filter, refresh);
      if (meu !== epoch) return;

      items = res.data.items;
      continuation = res.data.continuation;
      searched = true;
      list?.scrollToTop();

      // Veio do cache e está vencido: mostramos na hora e revalidamos por
      // baixo. É o que faz a busca repetida parecer instantânea.
      if (res.stale && !refresh) void run(true);
    } catch (e) {
      if (meu !== epoch) return;
      error = errorMessage(e);
      items = [];
      continuation = null;
    } finally {
      if (meu === epoch) loading = false;
    }
  }

  async function more() {
    if (!continuation || loadingMore) return;
    const meu = epoch;
    loadingMore = true;
    try {
      const page = await searchMore(continuation);
      if (meu !== epoch) return;
      items = [...items, ...page.items];
      continuation = page.continuation;
    } catch (e) {
      if (meu === epoch) error = errorMessage(e);
    } finally {
      if (meu === epoch) loadingMore = false;
    }
  }

  function activate(item: SearchItem) {
    if (item.type !== "track") return; // abrir álbum/artista/playlist é fase 4
    const tracks = items.filter((i) => i.type === "track");
    const idx = tracks.findIndex((t) => t.id === item.id);
    player.play(tracks, Math.max(0, idx));
  }

  function pick(f: SearchFilter) {
    if (f === filter) return;
    filter = f;
    if (query.trim()) void run();
  }

  function onSubmit(e: SubmitEvent) {
    e.preventDefault();
    void run();
  }
</script>

<div class="search">
  <form onsubmit={onSubmit}>
    <input
      type="search"
      bind:value={query}
      placeholder="Buscar músicas, álbuns, artistas…"
      autocomplete="off"
      spellcheck="false"
    />
  </form>

  <div class="filters">
    {#each filters as f (f.id)}
      <button
        class="chip"
        class:active={filter === f.id}
        onclick={() => pick(f.id)}
      >
        {f.label}
      </button>
    {/each}
  </div>

  <div class="results">
    {#if error}
      <p class="msg error">{error}</p>
    {:else if loading && items.length === 0}
      <p class="msg">buscando…</p>
    {:else if searched && items.length === 0}
      <p class="msg">Nada encontrado para “{query}”.</p>
    {:else if items.length > 0}
      <VirtualList
        bind:this={list}
        {items}
        itemHeight={ROW_HEIGHT}
        onEnd={more}
      >
        {#snippet row(item: SearchItem)}
          <ResultRow {item} onActivate={activate} />
        {/snippet}
      </VirtualList>
    {:else}
      <p class="msg">Digite algo e pressione Enter.</p>
    {/if}
  </div>

  {#if loadingMore}
    <div class="more">carregando mais…</div>
  {/if}
</div>

<style>
  .search {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  form {
    padding: var(--space-4) var(--space-6) var(--space-3);
  }

  input {
    width: 100%;
    padding: var(--space-3) var(--space-4);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    font: inherit;
  }
  input::placeholder {
    color: var(--text-faint);
  }
  input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .filters {
    display: flex;
    gap: var(--space-2);
    padding: 0 var(--space-6) var(--space-3);
  }
  .chip {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .chip:hover {
    color: var(--text);
    border-color: var(--text-faint);
  }
  .chip.active {
    background: var(--text);
    border-color: var(--text);
    color: var(--bg);
  }

  .results {
    flex: 1;
    min-height: 0;
    padding: 0 var(--space-4);
  }

  .msg {
    padding: var(--space-6) var(--space-2);
    color: var(--text-dim);
  }
  .msg.error {
    color: #f87171;
  }

  .more {
    padding: var(--space-2) var(--space-6);
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
