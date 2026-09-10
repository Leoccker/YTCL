<script lang="ts">
  /** Uma linha de resultado de busca — faixa, álbum, artista ou playlist. */
  import Art from "./Art.svelte";
  import type { SearchItem } from "../api";

  interface Props {
    item: SearchItem;
    onOpen?: (item: SearchItem) => void;
  }

  let { item, onOpen }: Props = $props();

  function duration(secs: number | null): string {
    if (secs == null) return "";
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${String(s).padStart(2, "0")}`;
  }

  const title = $derived(item.type === "artist" ? item.name : item.title);

  const subtitle = $derived.by(() => {
    switch (item.type) {
      case "track":
        return item.artists.map((a) => a.name).join(", ");
      case "album":
        return [
          item.artists.map((a) => a.name).join(", "),
          item.year?.toString(),
        ]
          .filter(Boolean)
          .join(" · ");
      case "artist":
        return item.subscribers ? `${item.subscribers} inscritos` : "Artista";
      case "playlist":
        return [item.author, item.trackCount ? `${item.trackCount} faixas` : null]
          .filter(Boolean)
          .join(" · ");
    }
  });

  const kindLabel = $derived(
    { track: "", album: "Álbum", artist: "Artista", playlist: "Playlist" }[
      item.type
    ],
  );
</script>

<button class="row" onclick={() => onOpen?.(item)} title={title}>
  <Art art={item.art} size={40} alt="" rounded={item.type === "artist"} />

  <span class="text">
    <span class="title">{title}</span>
    <span class="subtitle">{subtitle}</span>
  </span>

  {#if kindLabel}
    <span class="kind">{kindLabel}</span>
  {/if}

  {#if item.type === "track"}
    <span class="dur">{duration(item.durationSecs)}</span>
  {/if}
</button>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 100%;
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
    text-align: left;
  }
  .row:hover {
    background: var(--bg-hover);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .title,
  .subtitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .subtitle {
    color: var(--text-dim);
    font-size: 12px;
  }

  .kind {
    color: var(--text-faint);
    font-size: 11px;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    flex: none;
  }

  .dur {
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 12px;
    flex: none;
    min-width: 44px;
    text-align: right;
  }
</style>
