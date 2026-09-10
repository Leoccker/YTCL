/**
 * Estado de reprodução, compartilhado pela UI.
 *
 * Fonte da verdade é o backend: um snapshot no boot + os eventos "player"
 * daí em diante. Os métodos são fire-and-forget (o backend responde por
 * evento).
 */
import {
  onPlayerEvent,
  playerSnapshot,
  playerAvailable,
  playTracks,
  playerToggle,
  playerNext,
  playerPrev,
  playerSeek,
  playerSetVolume,
  playerSetRepeat,
  playerSetShuffle,
  playerPlayNext,
  playerEnqueue,
  playerMoveQueue,
  playerJumpQueue,
  errorMessage,
  type PlaybackState,
  type QueueEntry,
  type RepeatMode,
  type Track,
} from "../api";

class PlayerState {
  available = $state(false);
  playbackState = $state<PlaybackState>("idle");
  current = $state<Track | null>(null);
  /** Posição vinda do backend (~4 Hz). A barra interpola entre eventos. */
  position = $state(0);
  duration = $state(0);
  volume = $state(0.8);
  repeat = $state<RepeatMode>("off");
  shuffled = $state(false);
  queue = $state<QueueEntry[]>([]);
  error = $state<string | null>(null);

  /** Timestamp (ms) do último evento de posição — base da interpolação. */
  #posAt = 0;

  readonly playing = $derived(this.playbackState === "playing");
  readonly hasTrack = $derived(this.current !== null);

  /** Posição interpolada para a barra de progresso, sem esperar o próximo evento. */
  displayPosition(now: number): number {
    if (this.playbackState !== "playing") return this.position;
    const drift = (now - this.#posAt) / 1000;
    return Math.min(this.position + drift, this.duration || Infinity);
  }

  async init() {
    this.available = await playerAvailable().catch(() => false);

    const snap = await playerSnapshot().catch(() => null);
    if (snap) {
      this.playbackState = snap.state;
      this.current = snap.current;
      this.position = snap.position;
      this.duration = snap.duration;
      this.volume = snap.volume;
      this.repeat = snap.repeat;
      this.shuffled = snap.shuffled;
      this.queue = snap.queue;
      this.#posAt = performance.now();
    }

    await onPlayerEvent((e) => {
      switch (e.event) {
        case "state":
          this.playbackState = e.state;
          break;
        case "position":
          this.position = e.secs;
          this.duration = e.durationSecs || this.duration;
          this.#posAt = performance.now();
          break;
        case "track_changed":
          this.current = e.track;
          this.position = 0;
          this.#posAt = performance.now();
          void this.refreshQueue();
          break;
        case "queue_changed":
          void this.refreshQueue();
          break;
        case "volume":
          this.volume = e.level;
          break;
        case "error":
          this.error = e.message;
          break;
      }
    });
  }

  private async refreshQueue() {
    const snap = await playerSnapshot().catch(() => null);
    if (snap) {
      this.queue = snap.queue;
      this.repeat = snap.repeat;
      this.shuffled = snap.shuffled;
    }
  }

  private guard(p: Promise<unknown>) {
    p.catch((e) => (this.error = errorMessage(e)));
  }

  #lastPlayAt = 0;
  #lastPlayId: string | null = null;

  play(tracks: Track[], start = 0) {
    const id = tracks[start]?.id ?? null;
    const now = performance.now();
    // Ignora clique repetido na mesma faixa, e qualquer clique dentro de
    // 700ms do anterior — senão uma sequência de cliques vira uma tempestade
    // de resoluções e o YouTube passa a devolver 403.
    if (id && id === this.#lastPlayId && now - this.#lastPlayAt < 4000) return;
    if (now - this.#lastPlayAt < 700) return;
    this.#lastPlayAt = now;
    this.#lastPlayId = id;
    this.guard(playTracks(tracks, start));
  }
  toggle() {
    this.guard(playerToggle());
  }
  next() {
    this.guard(playerNext());
  }
  prev() {
    this.guard(playerPrev());
  }
  seek(secs: number) {
    this.position = secs;
    this.#posAt = performance.now();
    this.guard(playerSeek(secs));
  }
  setVolume(level: number) {
    this.volume = level;
    this.guard(playerSetVolume(level));
  }
  cycleRepeat() {
    const order: RepeatMode[] = ["off", "all", "one"];
    const nextMode = order[(order.indexOf(this.repeat) + 1) % 3]!;
    this.repeat = nextMode;
    this.guard(playerSetRepeat(nextMode));
  }
  toggleShuffle() {
    this.shuffled = !this.shuffled;
    this.guard(playerSetShuffle(this.shuffled));
  }
  playNext(track: Track) {
    this.guard(playerPlayNext(track));
  }
  enqueue(track: Track) {
    this.guard(playerEnqueue(track));
  }
  moveInQueue(from: number, to: number) {
    this.guard(playerMoveQueue(from, to));
  }
  jumpTo(orderIndex: number) {
    this.guard(playerJumpQueue(orderIndex));
  }
}

export const player = new PlayerState();
