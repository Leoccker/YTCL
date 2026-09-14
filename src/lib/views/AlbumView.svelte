<script lang="ts">
  /**
   * Tela de álbum: cabeçalho com capa/artistas/ano e a lista de faixas
   * completa (o comando `album` já devolve todas — sem paginação aqui).
   *
   * Componente é recriado a cada troca de id (`{#key}` em App.svelte), então
   * o estado abaixo nasce limpo por álbum — não precisa de `$effect` em `id`.
   */
  import DetailHeader from "../components/DetailHeader.svelte";
  import TrackList from "../components/TrackList.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import ErrorState from "../components/ErrorState.svelte";
  import { player } from "../stores/player.svelte";
  import { router } from "../router.svelte";
  import { album, errorMessage, type AlbumView as AlbumViewData } from "../api";

  interface Props {
    id: string;
  }

  let { id }: Props = $props();

  let data = $state<AlbumViewData | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  /**
   * Só a carga mais recente pode escrever no estado — sem isto, uma
   * revalidação silenciosa (stale) que demora mais que um retry manual
   * poderia sobrescrever o resultado do retry com dado velho.
   */
  let epoch = 0;

  async function load(refresh = false) {
    const meu = ++epoch;
    if (!refresh) loading = true;
    error = null;
    try {
      const res = await album(id, refresh);
      if (meu !== epoch) return;
      data = res.data;

      // Veio do cache e está vencido: mostra na hora e revalida por baixo.
      if (res.stale && !refresh) void load(true);
    } catch (e) {
      if (meu !== epoch) return;
      // Revalidação silenciosa falhou: mantém o que já está na tela em vez
      // de trocar por uma mensagem de erro.
      if (!data) error = errorMessage(e);
    } finally {
      if (meu === epoch) loading = false;
    }
  }

  void load();

  function playAll() {
    if (!data || data.tracks.length === 0) return;
    player.play(data.tracks, 0);
  }

  function goArtist(artistId: string) {
    router.push({ name: "artist", id: artistId });
  }
</script>

<div class="album-view">
  {#if loading && !data && !error}
    <Skeleton variant="header" />
    <div class="list-area"><Skeleton variant="rows" count={8} /></div>
  {:else if error && !data}
    <ErrorState message={error} onRetry={() => load()} />
  {:else if data}
    {@const d = data}
    <DetailHeader art={d.album.art} kicker="Álbum" title={d.album.title}>
      {#snippet subtitle()}
        {#each d.album.artists as artist, i (artist.id ?? `${artist.name}-${i}`)}
          {#if i > 0}<span>, </span>{/if}
          {#if artist.id}
            <button class="link" onclick={() => goArtist(artist.id!)}>{artist.name}</button>
          {:else}
            <span>{artist.name}</span>
          {/if}
        {/each}
        {#if d.album.year}<span> · {d.album.year}</span>{/if}
        <span> · {d.album.trackCount ?? d.tracks.length} faixas</span>
      {/snippet}
      {#snippet actions()}
        <button class="btn primary" onclick={playAll} disabled={d.tracks.length === 0}>
          Tocar
        </button>
        <button class="btn" onclick={() => player.playShuffled(d.tracks)} disabled={d.tracks.length === 0}>
          Aleatório
        </button>
      {/snippet}
    </DetailHeader>

    <div class="list-area">
      {#if d.tracks.length === 0}
        <EmptyState title="Álbum sem faixas" message="Este álbum não tem faixas disponíveis." />
      {:else}
        <TrackList tracks={d.tracks} showIndex />
      {/if}
    </div>
  {/if}
</div>

<style>
  .album-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .list-area {
    flex: 1;
    min-height: 0;
  }

  .link {
    color: inherit;
    text-decoration: underline;
    text-decoration-color: transparent;
  }
  .link:hover {
    text-decoration-color: currentColor;
    color: var(--text);
  }
</style>
