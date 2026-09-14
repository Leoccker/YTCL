<script lang="ts">
  import Login from "./Login.svelte";
  import MediaCard from "../components/MediaCard.svelte";
  import TrackList from "../components/TrackList.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import ErrorState from "../components/ErrorState.svelte";
  import { auth } from "../stores/auth.svelte";
  import { router } from "../router.svelte";
  import {
    libraryPlaylists,
    libraryAlbums,
    libraryArtists,
    likedSongs,
    errorMessage,
    type Album,
    type Artist,
    type ArtRef,
    type Playlist,
    type Track,
  } from "../api";

  type Tab = "playlists" | "albums" | "artists" | "liked";
  const tabs: { id: Tab; label: string }[] = [
    { id: "playlists", label: "Playlists" },
    { id: "albums", label: "Álbuns" },
    { id: "artists", label: "Artistas" },
    { id: "liked", label: "Curtidas" },
  ];
  let tab = $state<Tab>("playlists");

  let playlists = $state<Playlist[]>([]);
  let albums = $state<Album[]>([]);
  let artists = $state<Artist[]>([]);
  let liked = $state<Track[]>([]);
  let likedCont = $state<string | null>(null);

  /**
   * Estado por aba. Antes era um `loading`/`error` só, compartilhado — daí um
   * erro na aba de playlists aparecia nas outras. Agora cada aba tem o seu.
   */
  interface TabState {
    loading: boolean;
    error: string | null;
    loaded: boolean;
  }
  const st = $state<Record<Tab, TabState>>({
    playlists: { loading: false, error: null, loaded: false },
    albums: { loading: false, error: null, loaded: false },
    artists: { loading: false, error: null, loaded: false },
    liked: { loading: false, error: null, loaded: false },
  });

  /**
   * Uma requisição por aba de cada vez. Trocar de aba e voltar antes de a
   * primeira responder descartaria a resposta velha — sem isto, ela
   * sobrescreveria a aba atual.
   */
  const epoch: Record<Tab, number> = {
    playlists: 0,
    albums: 0,
    artists: 0,
    liked: 0,
  };

  interface CardItem {
    id: string;
    art: ArtRef | null;
    title: string;
    subtitle: string;
  }

  function cards(t: Tab): CardItem[] {
    if (t === "playlists")
      return playlists.map((p) => ({
        id: p.id,
        art: p.art,
        title: p.title,
        subtitle: p.author ?? (p.trackCount ? `${p.trackCount} faixas` : ""),
      }));
    if (t === "albums")
      return albums.map((a) => ({
        id: a.id,
        art: a.art,
        title: a.title,
        subtitle: [a.artists.map((x) => x.name).join(", "), a.year]
          .filter(Boolean)
          .join(" · "),
      }));
    if (t === "artists")
      return artists.map((a) => ({
        id: a.id,
        art: a.art,
        title: a.name,
        subtitle: a.subscribers ? `${a.subscribers} inscritos` : "",
      }));
    return [];
  }

  async function load(t: Tab, force = false) {
    const s = st[t];
    if (!auth.loggedIn || s.loading || (s.loaded && !force)) return;

    const mine = ++epoch[t];
    s.loading = true;
    s.error = null;

    try {
      if (t === "playlists") {
        const r = await libraryPlaylists();
        if (epoch[t] !== mine) return;
        playlists = r;
      } else if (t === "albums") {
        const r = await libraryAlbums();
        if (epoch[t] !== mine) return;
        albums = r;
      } else if (t === "artists") {
        const r = await libraryArtists();
        if (epoch[t] !== mine) return;
        artists = r;
      } else {
        const page = await likedSongs();
        if (epoch[t] !== mine) return;
        liked = page.items;
        likedCont = page.continuation;
      }
      s.loaded = true;
    } catch (e) {
      if (epoch[t] === mine) s.error = errorMessage(e);
    } finally {
      if (epoch[t] === mine) s.loading = false;
    }
  }

  async function moreLiked() {
    const s = st.liked;
    if (!likedCont || s.loading) return;
    s.loading = true;
    try {
      const page = await likedSongs(likedCont);
      liked = [...liked, ...page.items];
      likedCont = page.continuation;
    } catch (e) {
      s.error = errorMessage(e);
    } finally {
      s.loading = false;
    }
  }

  function retry(t: Tab) {
    st[t].error = null;
    load(t, true);
  }

  /** Abre o card: cada aba mapeia para uma rota de detalhe diferente. */
  function openCard(t: Tab, id: string) {
    if (t === "playlists") router.push({ name: "playlist", id });
    else if (t === "albums") router.push({ name: "album", id });
    else if (t === "artists") router.push({ name: "artist", id });
  }

  // Sessão nova (login/reconexão): zera tudo e recarrega a aba atual.
  let hadSession = false;
  $effect(() => {
    if (auth.loggedIn && !hadSession) {
      playlists = [];
      albums = [];
      artists = [];
      liked = [];
      for (const k of Object.keys(st) as Tab[]) {
        st[k] = { loading: false, error: null, loaded: false };
      }
      load(tab, true);
    }
    hadSession = auth.loggedIn;
  });

  $effect(() => {
    void tab;
    load(tab);
  });
</script>

{#if !auth.loggedIn}
  <Login />
{:else}
  <div class="library">
    <div class="tabs">
      {#each tabs as t (t.id)}
        <button class="tab" class:active={tab === t.id} onclick={() => (tab = t.id)}>
          {t.label}
          {#if st[t.id].loading}<span class="spin">·</span>{/if}
        </button>
      {/each}
    </div>

    <div class="body">
      {#if st[tab].error}
        <ErrorState message={st[tab].error ?? ""} onRetry={() => retry(tab)} />
      {:else if st[tab].loading && !st[tab].loaded}
        {#if tab === "liked"}
          <Skeleton variant="rows" count={10} />
        {:else}
          <Skeleton variant="cards" count={12} />
        {/if}
      {:else if tab === "liked"}
        {#if liked.length}
          <TrackList tracks={liked} onEnd={moreLiked} />
        {:else}
          <EmptyState
            title="Nenhuma música curtida"
            message="As músicas que você curtir no YouTube Music aparecem aqui."
          />
        {/if}
      {:else}
        {@const items = cards(tab)}
        {#if items.length}
          <div class="grid">
            {#each items as it (it.id)}
              <MediaCard
                art={it.art}
                title={it.title}
                subtitle={it.subtitle}
                rounded={tab === "artists"}
                onOpen={() => openCard(tab, it.id)}
              />
            {/each}
          </div>
        {:else}
          <EmptyState title="Nada aqui ainda" />
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .library {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .tabs {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-6) var(--space-3);
  }
  .tab {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.active {
    background: var(--text);
    border-color: var(--text);
    color: var(--bg);
  }
  .spin {
    display: inline-block;
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.2;
    }
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--space-4);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--space-2);
    padding-bottom: var(--space-6);
  }
</style>
