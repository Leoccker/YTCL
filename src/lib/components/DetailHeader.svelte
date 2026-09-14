<script lang="ts">
  /** Cabeçalho das telas de detalhe (álbum, artista, playlist). */
  import type { Snippet } from "svelte";
  import Art from "./Art.svelte";
  import type { ArtRef } from "../api";

  interface Props {
    art: ArtRef | null;
    /** "Álbum" / "Artista" / "Playlist". */
    kicker: string;
    title: string;
    /** Artista: capa redonda. */
    rounded?: boolean;
    subtitle?: Snippet;
    actions?: Snippet;
  }

  let { art, kicker, title, rounded = false, subtitle, actions }: Props = $props();
</script>

<div class="header">
  <Art {art} size={180} {rounded} alt="" />

  <div class="info">
    <span class="kicker">{kicker}</span>
    <h1 class="title">{title}</h1>
    {#if subtitle}
      <div class="subtitle">{@render subtitle()}</div>
    {/if}
    {#if actions}
      <div class="actions">{@render actions()}</div>
    {/if}
  </div>
</div>

<style>
  .header {
    display: flex;
    gap: var(--space-6);
    align-items: flex-end;
    padding: var(--space-6);
  }

  .info {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .kicker {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }

  .title {
    margin: 0;
    font-size: 32px;
    line-height: 1.15;
    overflow-wrap: break-word;
  }

  .subtitle {
    color: var(--text-dim);
    font-size: 13px;
  }

  .actions {
    margin-top: var(--space-2);
    display: flex;
    gap: var(--space-2);
  }

  /* Janela baixa: a capa de 180px engoliria a lista de faixas logo abaixo. */
  @media (max-height: 700px) {
    .header {
      gap: var(--space-4);
      padding: var(--space-3) var(--space-6);
    }
    /* O Art fixa o tamanho por estilo inline, daí o !important. */
    .header > :global(.art) {
      width: 96px !important;
      height: 96px !important;
    }
    .title {
      font-size: 22px;
    }
  }

  /* Tela estreita: empilha em vez de espremer a capa grande. */
  @media (max-width: 560px) {
    .header {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
