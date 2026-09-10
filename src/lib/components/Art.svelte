<script lang="ts">
  /**
   * Capa de álbum, faixa ou artista.
   *
   * O `src` aponta para o protocolo `ytmart://`, que serve do cache em disco e
   * baixa no primeiro acesso. Nenhum byte de imagem passa pelo IPC.
   */
  import { artUrl, type ArtRef } from "../api";

  interface Props {
    art: ArtRef | null | undefined;
    size: number;
    alt?: string;
    rounded?: boolean;
  }

  let { art, size, alt = "", rounded = false }: Props = $props();

  const src = $derived(artUrl(art));
  let failed = $state(false);

  // Uma capa nova precisa de uma tentativa nova.
  $effect(() => {
    void src;
    failed = false;
  });
</script>

<div
  class="art"
  class:rounded
  style:width="{size}px"
  style:height="{size}px"
>
  {#if src && !failed}
    <img
      {src}
      {alt}
      width={size}
      height={size}
      loading="lazy"
      decoding="async"
      onerror={() => (failed = true)}
    />
  {:else}
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M9 18V6l10-2v12"
        fill="none"
        stroke="currentColor"
        stroke-width="1.6"
      />
      <circle cx="7" cy="18" r="2.2" fill="currentColor" />
      <circle cx="17" cy="16" r="2.2" fill="currentColor" />
    </svg>
  {/if}
</div>

<style>
  .art {
    flex: none;
    background: var(--bg-active);
    border-radius: var(--radius-sm);
    overflow: hidden;
    display: grid;
    place-items: center;
    color: var(--text-faint);
  }
  .rounded {
    border-radius: 50%;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  svg {
    width: 45%;
    height: 45%;
  }
</style>
