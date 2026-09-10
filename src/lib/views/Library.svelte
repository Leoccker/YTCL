<script lang="ts">
  import Login from "./Login.svelte";
  import MediaCard from "../components/MediaCard.svelte";
  import VirtualList from "../components/VirtualList.svelte";
  import ResultRow from "../components/ResultRow.svelte";
  import { auth } from "../stores/auth.svelte";
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

  let playlists = $state<Playlist[] | null>(null);
  let albums = $state<Album[] | null>(null);
  let artists = $state<Artist[] | null>(null);
  let liked = $state<Track[]>([]);
  let likedCont = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  interface CardItem {
    id: string;
    art: ArtRef | null;
    title: string;
    subtitle: string;
  }

  function cards(t: Tab): CardItem[] {
    if (t === "playlists")
      return (playlists ?? []).map((p) => ({
        id: p.id,
        art: p.art,
        title: p.title,
        subtitle: p.author ?? (p.trackCount ? `${p.trackCount} faixas` : ""),
      }));
    if (t === "albums")
      return (albums ?? []).map((a) => ({
        id: a.id,
        art: a.art,
        title: a.title,
        subtitle: [a.artists.map((x) => x.name).join(", "), a.year]
          .filter(Boolean)
          .join(" · "),
      }));
    if (t === "artists")
      return (artists ?? []).map((a) => ({
        id: a.id,
        art: a.art,
        title: a.name,
        subtitle: a.subscribers ? `${a.subscribers} inscritos` : "",
      }));
    return [];
  }

  function loaded(t: Tab): boolean {
    if (t === "playlists") return playlists !== null;
    if (t === "albums") return albums !== null;
    if (t === "artists") return artists !== null;
    return liked.length > 0;
  }

  async function load(t: Tab, force = false) {
    if (!auth.loggedIn || (loaded(t) && !force)) return;
    loading = true;
    error = null;
    try {
      if (t === "playlists") playlists = await libraryPlaylists();
      else if (t === "albums") albums = await libraryAlbums();
      else if (t === "artists") artists = await libraryArtists();
      else {
        const page = await likedSongs();
        liked = page.items;
        likedCont = page.continuation;
      }
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function moreLiked() {
    if (!likedCont || loading) return;
    loading = true;
    try {
      const page = await likedSongs(likedCont);
      liked = [...liked, ...page.items];
      likedCont = page.continuation;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  // Sessão nova (login/reconexão): zera os caches locais e recarrega a aba.
  let hadSession = false;
  $effect(() => {
    if (auth.loggedIn && !hadSession) {
      playlists = albums = artists = null;
      liked = [];
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
        </button>
      {/each}
    </div>

    <div class="body">
      {#if error}
        <p class="msg error">{error}</p>
      {:else if loading && !loaded(tab)}
        <p class="msg">carregando…</p>
      {:else if tab === "liked"}
        {#if liked.length}
          <VirtualList items={liked} itemHeight={56} onEnd={moreLiked}>
            {#snippet row(t: Track)}
              <ResultRow item={{ type: "track", ...t }} />
            {/snippet}
          </VirtualList>
        {:else}
          <p class="msg">Nenhuma música curtida.</p>
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
              />
            {/each}
          </div>
        {:else}
          <p class="msg">Nada aqui ainda.</p>
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
  .msg {
    padding: var(--space-6) var(--space-2);
    color: var(--text-dim);
  }
  .msg.error {
    color: #f87171;
  }
</style>
