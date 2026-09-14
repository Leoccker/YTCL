<script lang="ts">
  import { onMount } from "svelte";
  import VirtualList from "../components/VirtualList.svelte";
  import ResultRow from "../components/ResultRow.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import ErrorState from "../components/ErrorState.svelte";
  import { player } from "../stores/player.svelte";
  import { router } from "../router.svelte";
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
  /** Erro da página seguinte — fica no rodapé, sem sumir com o que carregou. */
  let moreError = $state<string | null>(null);
  let searched = $state(false);
  /** Termo da última busca que respondeu; o campo pode já ter outro texto. */
  let lastTerm = $state("");
  // Só o método que usamos: descreve o contrato de verdade, em vez de um
  // ReturnType que o TypeScript aceitaria como qualquer coisa.
  let list = $state<{ scrollToTop: () => void } | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);

  // Ctrl+F (atalho global, em App.svelte) foca este campo — ele já trocou a
  // rota para "search" antes de disparar o evento.
  onMount(() => {
    function onFocusSearch() {
      inputEl?.focus();
      inputEl?.select();
    }
    window.addEventListener("ytcl:focus-search", onFocusSearch);
    return () => window.removeEventListener("ytcl:focus-search", onFocusSearch);
  });

  /**
   * Cada busca recebe um número; só a mais recente pode escrever o resultado.
   * Sem isto, digitar rápido faz uma resposta antiga chegar depois da nova e
   * sobrescrever a tela com o resultado errado.
   */
  let epoch = 0;

  /**
   * `silent` é a revalidação de um resultado vencido do cache: troca os dados
   * por baixo sem skeleton, sem arrancar a rolagem e sem apagar a tela se
   * falhar. Termo e filtro vêm por parâmetro porque o campo pode ter mudado
   * enquanto a primeira resposta chegava.
   */
  async function run(refresh = false, silent = false, termo = query.trim(), f = filter) {
    if (!termo) {
      items = [];
      continuation = null;
      searched = false;
      error = null;
      return;
    }

    const meu = ++epoch;
    if (!silent) {
      loading = true;
      error = null;
      moreError = null;
    }

    try {
      const res = await search(termo, f, refresh);
      if (meu !== epoch) return;

      items = res.data.items;
      continuation = res.data.continuation;
      searched = true;
      lastTerm = termo;
      if (!silent) list?.scrollToTop();
      loading = false;

      // Veio do cache e está vencido: mostramos na hora e revalidamos por
      // baixo. É o que faz a busca repetida parecer instantânea.
      if (res.stale && !refresh) void run(true, true, termo, f);
    } catch (e) {
      if (meu !== epoch) return;
      loading = false;
      // Dado velho na tela é melhor que um erro no lugar dele.
      if (silent) return;
      error = errorMessage(e);
      items = [];
      continuation = null;
    }
  }

  async function more() {
    if (!continuation || loadingMore) return;
    const meu = epoch;
    loadingMore = true;
    moreError = null;
    try {
      const page = await searchMore(continuation);
      if (meu !== epoch) return;
      items = [...items, ...page.items];
      continuation = page.continuation;
    } catch (e) {
      if (meu === epoch) moreError = errorMessage(e);
    } finally {
      // Incondicional: se uma busca nova começou no meio, a trava ainda
      // precisa soltar para ela conseguir paginar.
      loadingMore = false;
    }
  }

  function activate(item: SearchItem) {
    if (item.type === "track") {
      const tracks = items.filter((i) => i.type === "track");
      const idx = tracks.findIndex((t) => t.id === item.id);
      player.play(tracks, Math.max(0, idx));
      return;
    }
    if (item.type === "album") router.push({ name: "album", id: item.id });
    else if (item.type === "artist") router.push({ name: "artist", id: item.id });
    else if (item.type === "playlist") router.push({ name: "playlist", id: item.id });
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
      bind:this={inputEl}
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
      <ErrorState message={error} onRetry={() => void run()} />
    {:else if loading && items.length === 0}
      <Skeleton variant="rows" count={10} />
    {:else if searched && items.length === 0}
      <EmptyState title="Nada encontrado" message={`Nenhum resultado para “${lastTerm}”.`} />
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
      <EmptyState
        title="Busque músicas, álbuns, artistas e playlists"
        message="Digite algo e pressione Enter. Ctrl+F volta para cá de qualquer tela."
      />
    {/if}
  </div>

  {#if loadingMore}
    <div class="more">carregando mais…</div>
  {:else if moreError}
    <div class="more error">
      <span>{moreError}</span>
      <button onclick={() => void more()}>Tentar de novo</button>
    </div>
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

  .more {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-6);
    color: var(--text-faint);
    font-size: 12px;
  }
  .more.error {
    color: #f87171;
  }
  .more button {
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
  }
</style>
