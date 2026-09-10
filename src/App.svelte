<script lang="ts">
  import DebugOverlay from "./lib/components/DebugOverlay.svelte";
  import Search from "./lib/views/Search.svelte";

  type Route = "home" | "search" | "library";

  const nav: { id: Route; label: string }[] = [
    { id: "home", label: "Início" },
    { id: "search", label: "Buscar" },
    { id: "library", label: "Biblioteca" },
  ];

  let current = $state<Route>("search");

  function onKey(e: KeyboardEvent) {
    if (e.key === "f" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      current = "search";
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<DebugOverlay />

<div class="app">
  <nav class="sidebar">
    <div class="brand">YTMC</div>
    {#each nav as item (item.id)}
      <button
        class="nav-item"
        class:active={current === item.id}
        onclick={() => (current = item.id)}
      >
        {item.label}
      </button>
    {/each}
  </nav>

  <main class="content">
    {#if current === "search"}
      <Search />
    {:else if current === "home"}
      <div class="placeholder">
        <h1>Início</h1>
        <p>Recomendações entram depois da autenticação (fase 2).</p>
      </div>
    {:else}
      <div class="placeholder">
        <h1>Biblioteca</h1>
        <p>Precisa de conta conectada — fase 2.</p>
      </div>
    {/if}
  </main>

  <footer class="player">
    <span class="dim">a barra do player entra na fase 3</span>
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

  .content {
    grid-area: content;
    min-height: 0;
    overflow: hidden;
  }

  .placeholder {
    padding: var(--space-8);
  }
  .placeholder h1 {
    margin: 0 0 var(--space-2);
    font-size: 22px;
  }
  .placeholder p {
    color: var(--text-dim);
  }

  .player {
    grid-area: player;
    background: var(--bg-elevated);
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    padding: 0 var(--space-4);
  }

  .dim {
    color: var(--text-dim);
  }
</style>
