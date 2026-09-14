<script lang="ts">
  /**
   * Início: uma prateleira horizontal por seção que o backend mandar. A tela
   * é dona da própria rolagem (vertical, entre prateleiras); cada prateleira
   * rola na horizontal (Shelf.svelte).
   */
  import { onMount } from "svelte";
  import Shelf from "../components/Shelf.svelte";
  import MediaCard from "../components/MediaCard.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import ErrorState from "../components/ErrorState.svelte";
  import { router } from "../router.svelte";
  import { player } from "../stores/player.svelte";
  import {
    home,
    errorMessage,
    type HomeView as HomeViewData,
    type HomeSection,
    type SearchItem,
    type Track,
  } from "../api";

  let data = $state<HomeViewData | null>(null);
  let loading = $state(false);
  let refreshing = $state(false);
  let error = $state<string | null>(null);

  onMount(() => void load(false));

  /**
   * Mesmo padrão das outras telas: carrega do cache, revalida por baixo se
   * `stale` e troca quando chegar. Uma revalidação (ou o botão "Atualizar")
   * que falhar mantém o que já está na tela — só mostra erro se ainda não
   * há nada carregado.
   */
  let epoch = 0;
  async function load(refresh: boolean) {
    const meu = ++epoch;
    if (refresh) refreshing = true;
    else if (!data) loading = true;

    try {
      const res = await home(refresh);
      if (meu !== epoch) return;
      data = res.data;
      error = null;
      if (res.stale && !refresh) void load(true);
    } catch (e) {
      if (meu !== epoch) return;
      if (!data) error = errorMessage(e);
    } finally {
      if (meu !== epoch) return;
      loading = false;
      refreshing = false;
    }
  }

  function subtitle(item: SearchItem): string {
    switch (item.type) {
      case "track":
        return item.artists.map((a) => a.name).join(", ");
      case "album":
        return [item.artists.map((a) => a.name).join(", "), item.year?.toString()]
          .filter(Boolean)
          .join(" · ");
      case "artist":
        return item.subscribers ? `${item.subscribers} inscritos` : "Artista";
      case "playlist":
        return [item.author, item.trackCount ? `${item.trackCount} faixas` : null]
          .filter(Boolean)
          .join(" · ");
    }
  }

  /** Faixa de uma prateleira: toca as faixas daquela seção a partir dela. */
  function playFromSection(section: HomeSection, item: Track & { type: "track" }) {
    const tracks = section.items.filter(
      (x): x is Track & { type: "track" } => x.type === "track",
    );
    const idx = tracks.findIndex((t) => t.id === item.id);
    player.play(tracks, Math.max(0, idx));
  }

  function open(section: HomeSection, item: SearchItem) {
    if (item.type === "track") playFromSection(section, item);
    else if (item.type === "album") router.push({ name: "album", id: item.id });
    else if (item.type === "artist") router.push({ name: "artist", id: item.id });
    else if (item.type === "playlist") router.push({ name: "playlist", id: item.id });
  }
</script>

<div class="home">
  <div class="head">
    <h1>Início</h1>
    <button class="refresh" disabled={refreshing} onclick={() => load(true)}>
      {refreshing ? "Atualizando…" : "Atualizar"}
    </button>
  </div>

  {#if loading && !data}
    <Skeleton variant="cards" count={6} />
    <Skeleton variant="cards" count={6} />
  {:else if error && !data}
    <ErrorState message={error} onRetry={() => load(false)} />
  {:else if data}
    {#if data.sections.length === 0}
      <EmptyState
        title="Nada por aqui ainda"
        message="Volte mais tarde para ver recomendações."
      />
    {:else}
      {#each data.sections as section (section.title)}
        {#if section.items.length > 0}
          <Shelf title={section.title} items={section.items}>
            {#snippet card(item: SearchItem)}
              {#if item.type === "artist"}
                <MediaCard
                  art={item.art}
                  title={item.name}
                  subtitle={subtitle(item)}
                  rounded
                  onOpen={() => open(section, item)}
                />
              {:else}
                <MediaCard
                  art={item.art}
                  title={item.title}
                  subtitle={subtitle(item)}
                  onOpen={() => open(section, item)}
                />
              {/if}
            {/snippet}
          </Shelf>
        {/if}
      {/each}
    {/if}
  {/if}
</div>

<style>
  .home {
    height: 100%;
    overflow-y: auto;
    padding-bottom: var(--space-8);
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: var(--space-6) var(--space-6) var(--space-4);
  }
  .head h1 {
    margin: 0;
    font-size: 22px;
  }

  .refresh {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-dim);
    font-size: 12px;
  }
  .refresh:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .refresh:disabled {
    color: var(--text-faint);
    opacity: 0.6;
  }
</style>
