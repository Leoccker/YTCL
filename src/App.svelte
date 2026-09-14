<script lang="ts">
  import { onMount, tick } from "svelte";
  import DebugOverlay from "./lib/components/DebugOverlay.svelte";
  import ReconnectBanner from "./lib/components/ReconnectBanner.svelte";
  import ContextMenu from "./lib/components/ContextMenu.svelte";
  import Search from "./lib/views/Search.svelte";
  import Library from "./lib/views/Library.svelte";
  import HomeView from "./lib/views/HomeView.svelte";
  import QueueView from "./lib/views/QueueView.svelte";
  import AlbumView from "./lib/views/AlbumView.svelte";
  import ArtistView from "./lib/views/ArtistView.svelte";
  import PlaylistView from "./lib/views/PlaylistView.svelte";
  import { auth } from "./lib/stores/auth.svelte";
  import { player } from "./lib/stores/player.svelte";
  import { router } from "./lib/router.svelte";
  import PlayerBar from "./lib/components/PlayerBar.svelte";

  const nav: { id: "home" | "search" | "library"; label: string }[] = [
    { id: "home", label: "Início" },
    { id: "search", label: "Buscar" },
    { id: "library", label: "Biblioteca" },
  ];

  onMount(() => {
    void auth.init();
    void player.init();
  });

  const route = $derived(router.current);

  // Telas de topo continuam montadas (escondidas) depois da primeira visita —
  // ver o template. Só monta na primeira visita para a Biblioteca não buscar
  // nada na abertura do app se a pessoa nunca a abrir.
  const visited = $state({ home: false, search: false, library: false });
  $effect(() => {
    const name = route.name;
    if (name === "home" || name === "search" || name === "library") visited[name] = true;
  });

  function isTypingTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    if (target instanceof HTMLInputElement) return target.type !== "range";
    return target.tagName === "TEXTAREA" || target.isContentEditable;
  }

  const SEEK_STEP = 5;
  const VOLUME_STEP = 0.05;

  function onKey(e: KeyboardEvent) {
    // Ctrl+F/Ctrl+L funcionam mesmo digitando (ex.: sair de um campo de texto
    // para ir pra busca ou biblioteca). Os demais atalhos são ignorados
    // enquanto o foco está num campo, para não roubar tecla de digitação.
    if (e.ctrlKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      router.push({ name: "search" });
      // Espera a busca montar/aparecer: campo escondido não recebe foco, e na
      // primeira visita o listener dela ainda nem existe.
      void tick().then(() => window.dispatchEvent(new CustomEvent("ytcl:focus-search")));
      return;
    }
    if (e.ctrlKey && e.key.toLowerCase() === "l") {
      e.preventDefault();
      router.push({ name: "library" });
      return;
    }

    if (isTypingTarget(e.target)) return;

    if (e.key === " ") {
      // O preventDefault também impede o Espaço de "clicar" o botão que
      // estiver focado — senão o último botão clicado seria acionado de novo.
      e.preventDefault();
      player.toggle();
      return;
    }

    // Setas num slider focado já mexem nele; não duplicar.
    const onSlider = e.target instanceof HTMLInputElement && e.target.type === "range";
    if (e.altKey && e.key === "ArrowLeft") {
      e.preventDefault();
      router.back();
      return;
    }
    if (e.altKey && e.key === "ArrowRight") {
      e.preventDefault();
      router.forward();
      return;
    }
    if (onSlider || e.altKey || e.shiftKey || !player.hasTrack) return;

    if (e.ctrlKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      if (e.key === "ArrowLeft") player.prev();
      else player.next();
    } else if (e.ctrlKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      // ↑/↓ sem Ctrl ficam com a rolagem nativa das listas.
      e.preventDefault();
      const delta = e.key === "ArrowUp" ? VOLUME_STEP : -VOLUME_STEP;
      player.setVolume(Math.min(1, Math.max(0, player.volume + delta)));
    } else if (!e.ctrlKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      const now = player.displayPosition(performance.now());
      const delta = e.key === "ArrowLeft" ? -SEEK_STEP : SEEK_STEP;
      const max = player.duration > 0 ? player.duration : now + SEEK_STEP;
      player.seek(Math.min(max, Math.max(0, now + delta)));
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<DebugOverlay />
<ContextMenu />

<div class="app">
  <nav class="sidebar">
    <div class="brand">YTCL</div>
    {#each nav as item (item.id)}
      <button
        class="nav-item"
        class:active={route.name === item.id}
        onclick={() => router.push({ name: item.id })}
      >
        {item.label}
      </button>
    {/each}

    <div class="account">
      {#if auth.account}
        <span class="who" title={auth.account.label}>
          {auth.expired ? "⚠ " : ""}{auth.account.label}
        </span>
        <button class="acct-btn" onclick={() => auth.logout(auth.account!.id)}>
          sair
        </button>
      {:else}
        <button class="acct-btn" onclick={() => router.push({ name: "library" })}>
          Entrar
        </button>
      {/if}
    </div>

    <p class="disclaimer">Não afiliado ao Google ou ao YouTube.</p>
  </nav>

  <main class="content">
    <div class="toolbar">
      <button
        class="nav-btn"
        disabled={!router.canBack}
        onclick={() => router.back()}
        title="Voltar"
      >
        ←
      </button>
      <button
        class="nav-btn"
        disabled={!router.canForward}
        onclick={() => router.forward()}
        title="Avançar"
      >
        →
      </button>
    </div>

    <ReconnectBanner />

    <div class="view">
      <!-- Início, Buscar e Biblioteca ficam vivas depois da primeira visita:
           voltar de um álbum para a busca tem que encontrar os resultados e a
           rolagem onde estavam, sem refazer a busca. -->
      {#if visited.home || route.name === "home"}
        <div class="page" class:off={route.name !== "home"}><HomeView /></div>
      {/if}
      {#if visited.search || route.name === "search"}
        <div class="page" class:off={route.name !== "search"}><Search /></div>
      {/if}
      {#if visited.library || route.name === "library"}
        <div class="page" class:off={route.name !== "library"}><Library /></div>
      {/if}

      {#if route.name === "queue"}
        <div class="page"><QueueView /></div>
      {:else if route.name === "album"}
        {#key route.id}
          <div class="page"><AlbumView id={route.id} /></div>
        {/key}
      {:else if route.name === "artist"}
        {#key route.id}
          <div class="page"><ArtistView id={route.id} /></div>
        {/key}
      {:else if route.name === "playlist"}
        {#key route.id}
          <div class="page"><PlaylistView id={route.id} /></div>
        {/key}
      {/if}
    </div>
  </main>

  <footer class="player">
    <PlayerBar />
  </footer>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: var(--sidebar-w) 1fr;
    grid-template-rows: 1fr var(--player-h);
    grid-template-areas:
      "sidebar content"
      "player player";
    height: 100vh;
  }

  .sidebar {
    grid-area: sidebar;
    background: var(--bg-elevated);
    border-right: 1px solid var(--border);
    padding: var(--space-4) var(--space-2);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .brand {
    padding: 0 var(--space-3) var(--space-4);
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--accent);
  }

  .nav-item {
    text-align: left;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    color: var(--text-dim);
  }
  .nav-item:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--bg-active);
    color: var(--text);
  }

  .account {
    margin-top: auto;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
  }
  .who {
    font-size: 12px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .acct-btn {
    font-size: 12px;
    color: var(--text-dim);
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .acct-btn:hover {
    color: var(--text);
  }

  .disclaimer {
    padding: var(--space-3);
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-faint);
  }

  .content {
    grid-area: content;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-4);
    flex: none;
  }
  .nav-btn {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-dim);
  }
  .nav-btn:hover:not(:disabled) {
    color: var(--text);
    background: var(--bg-hover);
  }
  .nav-btn:disabled {
    color: var(--text-faint);
    opacity: 0.5;
  }

  .view {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }
  .page {
    position: absolute;
    inset: 0;
  }
  /* Escondida mas viva: `content-visibility` pula o trabalho de render e
     preserva o estado de rolagem; `visibility` tira do foco e dos cliques. */
  .page.off {
    content-visibility: hidden;
    visibility: hidden;
  }

  .player {
    grid-area: player;
    background: var(--bg-elevated);
    border-top: 1px solid var(--border);
  }
</style>
