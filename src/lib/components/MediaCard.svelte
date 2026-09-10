<script lang="ts">
  import Art from "./Art.svelte";
  import type { ArtRef } from "../api";

  interface Props {
    art: ArtRef | null;
    title: string;
    subtitle?: string;
    rounded?: boolean;
    onOpen?: () => void;
  }

  let { art, title, subtitle = "", rounded = false, onOpen }: Props = $props();
</script>

<button class="card" onclick={() => onOpen?.()} title={title}>
  <Art {art} size={160} {rounded} alt="" />
  <span class="title">{title}</span>
  {#if subtitle}<span class="subtitle">{subtitle}</span>{/if}
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border-radius: var(--radius);
    width: 100%;
    text-align: left;
  }
  .card:hover {
    background: var(--bg-hover);
  }
  .title,
  .subtitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title {
    font-size: 13px;
  }
  .subtitle {
    font-size: 12px;
    color: var(--text-dim);
  }
  /* A capa é 160 no markup mas o card encolhe com a coluna. */
  .card :global(.art) {
    width: 100% !important;
    height: auto !important;
    aspect-ratio: 1;
  }
</style>
