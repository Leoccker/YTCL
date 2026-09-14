<script lang="ts">
  /** Grade responsiva de cards (álbuns, artistas, playlists) — usada em telas
   * de detalhe que não precisam de virtualização (poucas dezenas de itens). */
  import type { Snippet } from "svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
</script>

<div class="grid">
  {@render children()}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: var(--space-3);
    padding: 0 var(--space-6) var(--space-4);
  }
</style>
