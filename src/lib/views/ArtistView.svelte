<script lang="ts">
  /**
   * Página de artista: cabeçalho, populares (com "mostrar mais") e álbuns.
   * A tela é dona da própria rolagem — o container `.view` do App.svelte vem
   * com overflow:hidden de propósito, então o `overflow-y:auto` é daqui.
   */
  import { onMount } from "svelte";
  import DetailHeader from "../components/DetailHeader.svelte";
  import TrackRow from "../components/TrackRow.svelte";
  import MediaCard from "../components/MediaCard.svelte";
  import CardGrid from "../components/CardGrid.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import ErrorState from "../components/ErrorState.svelte";
  import { router } from "../router.svelte";
  import { player } from "../stores/player.svelte";
  import { artist as fetchArtist, errorMessage, type ArtistView as ArtistViewData } from "../api";

  interface Props {
    id: string;
  }

  let { id }: Props = $props();

  const POPULAR_LIMIT = 5;

  let data = $state<ArtistViewData | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let showAllTracks = $state(false);

  // O componente inteiro é recriado a cada troca de id (App.svelte envolve
  // em {#key id}), então onMount basta — não precisa de $effect watando id.
  onMount(() => void load(false));

  /**
   * Mesmo padrão de Search/Library: carrega do cache na hora; se `stale`,
   * revalida por baixo e troca quando chegar. Se a revalidação falhar,
   * mantém o que já está na tela em vez de substituir por um erro.
   */
  let epoch = 0;
  async function load(refresh: boolean) {
    const meu = ++epoch;
    if (!refresh) loading = true;

    try {
      const res = await fetchArtist(id, refresh);
      if (meu !== epoch) return;
      data = res.data;
      error = null;
      if (res.stale && !refresh) void load(true);
    } catch (e) {
      if (meu !== epoch) return;
      if (!data) error = errorMessage(e);
    } finally {
      if (meu === epoch) loading = false;
    }
  }

  function playFrom(i: number) {
    if (!data || data.tracks.length === 0) return;
    player.play(data.tracks, i);
  }

</script>

<div class="artist-view">
  {#if loading && !data}
    <Skeleton variant="header" />
    <Skeleton variant="rows" count={5} />
    <Skeleton variant="cards" count={6} />
  {:else if error && !data}
    <ErrorState message={error} onRetry={() => load(false)} />
  {:else if data}
    {@const tracks = showAllTracks ? data.tracks : data.tracks.slice(0, POPULAR_LIMIT)}
    {@const isEmpty = data.tracks.length === 0 && data.albums.length === 0}
    {@const d = data}

    <DetailHeader art={d.artist.art} kicker="Artista" title={d.artist.name} rounded>
      {#snippet subtitle()}
        {#if d.artist.subscribers}
          <span>{d.artist.subscribers} inscritos</span>
        {/if}
      {/snippet}
      {#snippet actions()}
        <button class="btn primary" disabled={d.tracks.length === 0} onclick={() => playFrom(0)}>
          Tocar
        </button>
        <button
          class="btn"
          disabled={d.tracks.length === 0}
          onclick={() => player.playShuffled(d.tracks)}
        >
          Aleatório
        </button>
      {/snippet}
    </DetailHeader>

    {#if isEmpty}
      <EmptyState
        title="Sem conteúdo"
        message="Não encontramos faixas nem álbuns para este artista."
      />
    {:else}
      {#if data.tracks.length > 0}
        <section class="section">
          <h2>Populares</h2>
          <div class="tracks">
            {#each tracks as track, i (track.id)}
              <TrackRow
                {track}
                index={i}
                showIndex
                active={player.current?.id === track.id}
                onPlay={() => playFrom(i)}
              />
            {/each}
          </div>
          {#if data.tracks.length > POPULAR_LIMIT}
            <button class="toggle" onclick={() => (showAllTracks = !showAllTracks)}>
              {showAllTracks ? "Mostrar menos" : "Mostrar mais"}
            </button>
          {/if}
        </section>
      {/if}

      {#if data.albums.length > 0}
        <section class="section">
          <h2>Álbuns</h2>
          <CardGrid>
            {#each data.albums as album (album.id)}
              <MediaCard
                art={album.art}
                title={album.title}
                subtitle={[album.artists.map((a) => a.name).join(", "), album.year]
                  .filter(Boolean)
                  .join(" · ")}
                onOpen={() => router.push({ name: "album", id: album.id })}
              />
            {/each}
          </CardGrid>
        </section>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .artist-view {
    height: 100%;
    overflow-y: auto;
    padding-bottom: var(--space-8);
  }

  .section {
    padding: var(--space-4) 0;
  }
  .section h2 {
    margin: 0 0 var(--space-2);
    padding: 0 var(--space-6);
    font-size: 16px;
  }

  .tracks {
    display: flex;
    flex-direction: column;
    padding: 0 var(--space-4);
  }

  .toggle {
    margin: var(--space-2) var(--space-6) 0;
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-dim);
    font-size: 12px;
  }
  .toggle:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
</style>
