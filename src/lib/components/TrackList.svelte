<script lang="ts">
  /** Lista virtualizada de faixas — usada em álbum, playlist, curtidas etc. */
  import VirtualList from "./VirtualList.svelte";
  import TrackRow from "./TrackRow.svelte";
  import { player } from "../stores/player.svelte";
  import type { Track } from "../api";

  const ROW_HEIGHT = 56;

  interface Props {
    tracks: Track[];
    showIndex?: boolean;
    /** Chamado ao aproximar do fim — para paginação. */
    onEnd?: () => void;
    /** Padrão: `player.play(tracks, index)`. */
    onPlay?: (tracks: Track[], index: number) => void;
  }

  let { tracks, showIndex = false, onEnd, onPlay }: Props = $props();

  function play(index: number) {
    if (onPlay) onPlay(tracks, index);
    else player.play(tracks, index);
  }
</script>

<div class="track-list">
  <VirtualList items={tracks} itemHeight={ROW_HEIGHT} {onEnd}>
    {#snippet row(track: Track, i: number)}
      <TrackRow
        {track}
        index={i}
        {showIndex}
        active={player.current?.id === track.id}
        onPlay={() => play(i)}
      />
    {/snippet}
  </VirtualList>
</div>

<style>
  /* Ocupa a altura do container pai — a VirtualList rola dentro. */
  .track-list {
    height: 100%;
    min-height: 0;
  }
</style>
