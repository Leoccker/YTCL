<script lang="ts">
  /**
   * Linha de faixa — altura fixa de 56px porque a `VirtualList` assume
   * altura de linha constante. Usada em álbum, artista, playlist, curtidas.
   */
  import Art from "./Art.svelte";
  import { router } from "../router.svelte";
  import { player } from "../stores/player.svelte";
  import { contextMenu, type MenuItem } from "../stores/contextMenu.svelte";
  import type { Track } from "../api";

  interface Props {
    track: Track;
    index?: number;
    /** Mostra o número da faixa no lugar da capa. */
    showIndex?: boolean;
    /** Faixa tocando agora: destaque com --accent. */
    active?: boolean;
    onPlay: () => void;
  }

  let { track, index, showIndex = false, active = false, onPlay }: Props = $props();

  function fmt(secs: number | null): string {
    if (secs == null) return "";
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${String(s).padStart(2, "0")}`;
  }

  function menuItems(): MenuItem[] {
    const items: MenuItem[] = [
      { label: "Tocar em seguida", action: () => player.playNext(track) },
      { label: "Adicionar à fila", action: () => player.enqueue(track) },
    ];

    const albumId = track.album?.id;
    if (albumId) {
      items.push({
        label: "Ir para o álbum",
        action: () => router.push({ name: "album", id: albumId }),
      });
    }

    const artistId = track.artists.find((a) => a.id)?.id;
    if (artistId) {
      items.push({
        label: "Ir para o artista",
        action: () => router.push({ name: "artist", id: artistId }),
      });
    }

    return items;
  }

  function openMenu(e: MouseEvent) {
    e.stopPropagation();
    contextMenu.open(e, menuItems());
  }

  function goArtist(e: MouseEvent, id: string | null) {
    e.stopPropagation();
    if (id) router.push({ name: "artist", id });
  }

  function goAlbum(e: MouseEvent) {
    e.stopPropagation();
    if (track.album?.id) router.push({ name: "album", id: track.album.id });
  }

  // Só Enter: Espaço é o play/pause global (App.svelte). Tratar os dois aqui
  // tocaria a faixa e alternaria a reprodução no mesmo toque.
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      onPlay();
    }
  }
</script>

<div
  class="row"
  class:active
  role="button"
  tabindex="0"
  onclick={onPlay}
  onkeydown={onKeydown}
  oncontextmenu={openMenu}
>
  {#if showIndex}
    <span class="index">{active ? "▶" : (index ?? 0) + 1}</span>
  {:else}
    <Art art={track.art} size={40} alt="" />
  {/if}

  <span class="text">
    <span class="title">{track.title}</span>
    <span class="subtitle">
      {#each track.artists as a, i (a.id ?? `${a.name}-${i}`)}
        {#if i > 0}<span class="sep">, </span>{/if}
        {#if a.id}
          <button class="link" onclick={(e) => goArtist(e, a.id)}>{a.name}</button>
        {:else}
          <span>{a.name}</span>
        {/if}
      {/each}
      {#if track.album}
        <span class="sep"> · </span>
        {#if track.album.id}
          <button class="link" onclick={goAlbum}>{track.album.title}</button>
        {:else}
          <span>{track.album.title}</span>
        {/if}
      {/if}
    </span>
  </span>

  <span class="dur">{fmt(track.durationSecs)}</span>

  <button class="more" onclick={openMenu} title="Mais opções">⋯</button>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 56px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.active {
    background: var(--bg-active);
  }
  .row.active .title {
    color: var(--accent);
  }

  .index {
    flex: none;
    width: 40px;
    text-align: center;
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .row.active .index {
    color: var(--accent);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .subtitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
    font-size: 12px;
  }
  .subtitle .sep {
    color: var(--text-faint);
  }
  .subtitle .link {
    color: var(--text-dim);
  }
  .subtitle .link:hover {
    color: var(--text);
    text-decoration: underline;
  }

  .dur {
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 12px;
    flex: none;
    min-width: 44px;
    text-align: right;
  }

  .more {
    flex: none;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--text-faint);
    font-size: 16px;
    line-height: 1;
  }
  .more:hover {
    color: var(--text);
    background: var(--bg-active);
  }
</style>
