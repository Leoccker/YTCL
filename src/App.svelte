<script lang="ts">
  import { onMount } from "svelte";
  import DebugOverlay from "./lib/components/DebugOverlay.svelte";
  import { appInfo, type AppInfo } from "./lib/api";

  let info = $state<AppInfo | null>(null);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      info = await appInfo();
    } catch (e) {
      error = String(e);
    }
  });

  const nav = [
    { id: "home", label: "Início" },
    { id: "search", label: "Buscar" },
    { id: "library", label: "Biblioteca" },
  ];
  let current = $state("home");
</script>

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
    {#if error}
      <p class="error">Falha ao falar com o backend: {error}</p>
    {:else if info}
      <h1>Fase 0 — fundação</h1>
      <p class="dim">
        IPC funcionando. Versão {info.version}.
      </p>
      <dl>
        <dt>config</dt>
        <dd>{info.configDir}</dd>
        <dt>cache</dt>
        <dd>{info.cacheDir}</dd>
      </dl>
      <p class="dim">Pressione <kbd>F3</kbd> para o overlay de performance.</p>
    {:else}
      <p class="dim">carregando…</p>
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
    overflow-y: auto;
    padding: var(--space-8);
  }

  .player {
    grid-area: player;
    background: var(--bg-elevated);
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    padding: 0 var(--space-4);
  }

  h1 {
    margin: 0 0 var(--space-2);
    font-size: 22px;
  }

  .dim {
    color: var(--text-dim);
  }

  .error {
    color: #f87171;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-1) var(--space-4);
    margin: var(--space-6) 0;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  dt {
    color: var(--text-faint);
  }
  dd {
    margin: 0;
    color: var(--text-dim);
  }

  kbd {
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-elevated);
  }
</style>
