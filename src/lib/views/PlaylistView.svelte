<script lang="ts">
  /**
   * Tela de playlist: metadados (`playlist`) e a primeira página de faixas
   * (`playlistTracks`) são carregados em paralelo, porque `playlistTracks`
   * não vem embutida na resposta de metadados (ao contrário de álbum) — é
   * paginada no backend e não tem cache (ver CLAUDE.md, regra 2).
   *
   * Cada uma tem seu próprio estado de carregando/erro: se uma falhar e a
   * outra vier, mostramos o que deu certo em vez de esconder tudo.
   *
   * Componente é recriado a cada troca de id (`{#key}` em App.svelte), então
   * o estado abaixo nasce limpo por playlist.
   */
  import DetailHeader from "../components/DetailHeader.svelte";
  import TrackList from "../components/TrackList.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import ErrorState from "../components/ErrorState.svelte";
  import { player } from "../stores/player.svelte";
  import { playlist, playlistTracks, errorMessage, type Playlist, type Track } from "../api";

  interface Props {
    id: string;
  }

  let { id }: Props = $props();

  // --- metadados -----------------------------------------------------
  let meta = $state<Playlist | null>(null);
  let metaLoading = $state(true);
  let metaError = $state<string | null>(null);
  /** Descarta resposta de um retry velho de metadados chegando atrasada. */
  let metaEpoch = 0;

  async function loadMeta(refresh = false) {
    const meu = ++metaEpoch;
    if (!refresh) metaLoading = true;
    metaError = null;
    try {
      const res = await playlist(id, refresh);
      if (meu !== metaEpoch) return;
      meta = res.data;
      if (res.stale && !refresh) void loadMeta(true);
    } catch (e) {
      if (meu !== metaEpoch) return;
      if (!meta) metaError = errorMessage(e);
    } finally {
      if (meu === metaEpoch) metaLoading = false;
    }
  }

  // --- faixas (paginadas) ---------------------------------------------
  let tracks = $state<Track[]>([]);
  let continuation = $state<string | null>(null);
  let tracksLoading = $state(true);
  let tracksError = $state<string | null>(null);
  let loadingMore = $state(false);
  let moreError = $state<string | null>(null);
  /** Descarta resposta de retry velho da primeira página chegando atrasada. */
  let tracksEpoch = 0;

  async function loadFirstPage() {
    const meu = ++tracksEpoch;
    tracksLoading = true;
    tracksError = null;
    try {
      const page = await playlistTracks(id);
      if (meu !== tracksEpoch) return;
      tracks = page.items;
      continuation = page.continuation;
    } catch (e) {
      if (meu !== tracksEpoch) return;
      tracksError = errorMessage(e);
      tracks = [];
      continuation = null;
    } finally {
      if (meu === tracksEpoch) tracksLoading = false;
    }
  }

  async function loadMore() {
    // Guarda contra pedido duplicado: a VirtualList pode chamar onEnd de
    // novo antes da página anterior voltar, e para quando não há mais token.
    if (!continuation || loadingMore) return;
    const meu = tracksEpoch;
    loadingMore = true;
    moreError = null;
    try {
      const page = await playlistTracks(id, continuation);
      if (meu !== tracksEpoch) return;
      tracks = [...tracks, ...page.items];
      continuation = page.continuation;
    } catch (e) {
      if (meu !== tracksEpoch) return;
      // Erro ao paginar não apaga o que já carregou — só avisa no rodapé.
      moreError = errorMessage(e);
    } finally {
      if (meu === tracksEpoch) loadingMore = false;
    }
  }

  void loadMeta();
  void loadFirstPage();

  function playAll() {
    if (tracks.length === 0) return;
    player.play(tracks, 0);
  }

</script>

<div class="playlist-view">
  {#if metaLoading && !meta && !metaError && tracksLoading && tracks.length === 0 && !tracksError}
    <Skeleton variant="header" />
    <div class="list-area"><Skeleton variant="rows" count={8} /></div>
  {:else}
    {#if meta}
      {@const m = meta}
      <DetailHeader art={m.art} kicker="Playlist" title={m.title}>
        {#snippet subtitle()}
          {#if m.author}<span>{m.author}</span><span> · </span>{/if}
          <span>{m.trackCount ?? tracks.length} faixas</span>
        {/snippet}
        {#snippet actions()}
          <button class="btn primary" onclick={playAll} disabled={tracks.length === 0}>
            Tocar
          </button>
          <button class="btn" onclick={() => player.playShuffled(tracks)} disabled={tracks.length === 0}>
            Aleatório
          </button>
        {/snippet}
      </DetailHeader>
    {:else if metaLoading}
      <Skeleton variant="header" />
    {:else if metaError}
      <div class="meta-error"><ErrorState message={metaError} onRetry={() => loadMeta()} /></div>
    {/if}

    <div class="list-area">
      {#if tracksLoading && tracks.length === 0}
        <Skeleton variant="rows" count={8} />
      {:else if tracksError && tracks.length === 0}
        <ErrorState message={tracksError} onRetry={() => loadFirstPage()} />
      {:else if tracks.length === 0}
        <EmptyState title="Playlist vazia" message="Esta playlist não tem faixas." />
      {:else}
        <TrackList {tracks} onEnd={loadMore} />
      {/if}
    </div>

    {#if moreError}
      <div class="more-banner">
        <span>{moreError}</span>
        <button class="retry-inline" onclick={() => loadMore()}>Tentar de novo</button>
      </div>
    {:else if loadingMore}
      <div class="more-status">carregando mais…</div>
    {/if}
  {/if}
</div>

<style>
  .playlist-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .list-area {
    flex: 1;
    min-height: 0;
  }

  .meta-error {
    flex: none;
  }

  /* Aviso discreto de erro ao paginar — não cobre a lista, fica no rodapé. */
  .more-banner {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-6);
    font-size: 12px;
    color: #f87171;
  }
  .retry-inline {
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
  }
  .retry-inline:hover {
    background: var(--bg-hover);
  }

  .more-status {
    flex: none;
    padding: var(--space-2) var(--space-6);
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
