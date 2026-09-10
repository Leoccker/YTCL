/**
 * Unico lugar do frontend que chama `invoke`.
 *
 * Manter os wrappers tipados aqui significa que uma mudanca de assinatura no
 * Rust quebra o `svelte-check` num arquivo, e nao espalhado por dez views.
 *
 * O Rust serializa os modelos em camelCase (`#[serde(rename_all)]`), entao
 * nao ha conversao de nomes aqui — os tipos abaixo espelham o que chega.
 */
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

// --- modelos -------------------------------------------------------------

export interface ArtRef {
  hash: string;
  url: string;
  width: number;
  height: number;
}

export interface ArtistRef {
  id: string | null;
  name: string;
}

export interface AlbumRef {
  id: string | null;
  title: string;
}

export interface Track {
  id: string;
  title: string;
  artists: ArtistRef[];
  album: AlbumRef | null;
  durationSecs: number | null;
  art: ArtRef | null;
  isExplicit: boolean;
  setVideoId: string | null;
}

export interface Album {
  id: string;
  title: string;
  artists: ArtistRef[];
  year: number | null;
  art: ArtRef | null;
  trackCount: number | null;
}

export interface Artist {
  id: string;
  name: string;
  art: ArtRef | null;
  subscribers: string | null;
}

export interface Playlist {
  id: string;
  title: string;
  author: string | null;
  art: ArtRef | null;
  trackCount: number | null;
}

export type SearchItem =
  | ({ type: "track" } & Track)
  | ({ type: "album" } & Album)
  | ({ type: "artist" } & Artist)
  | ({ type: "playlist" } & Playlist);

export interface Page<T> {
  items: T[];
  /** Token opaco. Devolver ao backend sem interpretar. */
  continuation: string | null;
}

/** Resposta de leitura: `stale` pede uma revalidação. */
export interface Cached<T> {
  data: T;
  stale: boolean;
}

export type SearchFilter = "all" | "songs" | "albums" | "artists" | "playlists";

export interface ErrorPayload {
  kind:
    | "unauthenticated"
    | "session_expired"
    | "network"
    | "parse"
    | "not_found"
    | "other";
  message: string;
}

// --- comandos ------------------------------------------------------------

export interface AppInfo {
  version: string;
  configDir: string;
  cacheDir: string;
}

export interface MemInfo {
  /** PSS: memória compartilhada dividida entre os processos. */
  pssMb: number | null;
  /** RSS somado: é o critério de aceite (alvo < 500 MB). */
  rssMb: number | null;
  processCount: number;
}

export const appInfo = () => invoke<AppInfo>("app_info");
export const memInfo = () => invoke<MemInfo>("mem_info");

export const search = (query: string, filter: SearchFilter, refresh = false) =>
  invoke<Cached<Page<SearchItem>>>("search", { query, filter, refresh });

export const searchMore = (continuation: string) =>
  invoke<Page<SearchItem>>("search_more", { continuation });

export interface AlbumView {
  album: Album;
  tracks: Track[];
}

export const album = (id: string, refresh = false) =>
  invoke<Cached<AlbumView>>("album", { id, refresh });

export interface ArtistView {
  artist: Artist;
  tracks: Track[];
  albums: Album[];
}

export const artist = (id: string, refresh = false) =>
  invoke<Cached<ArtistView>>("artist", { id, refresh });

export const playlist = (id: string, refresh = false) =>
  invoke<Cached<Playlist>>("playlist", { id, refresh });

export const playlistTracks = (id: string, continuation?: string) =>
  invoke<Page<Track>>("playlist_tracks", { id, continuation });

// --- contas -------------------------------------------------------------

export interface Account {
  id: string;
  label: string;
  addedAt: number;
}

export type Session =
  | { kind: "logged_out" }
  | { kind: "active"; account: Account }
  | { kind: "expired"; account: Account };

export interface AuthStatus {
  session: Session;
  accounts: Account[];
}

export const authStatus = () => invoke<AuthStatus>("auth_status");

/** Abre a janela de login do Google. Resolve quando a conta é conectada. */
export const authLoginGoogle = () => invoke<Account>("auth_login_google");

/** Alternativa: o usuário cola o cabeçalho Cookie ou um cookies.txt. */
export const authLoginCookie = (cookie: string) =>
  invoke<Account>("auth_login_cookie", { cookie });

export const authSwitch = (id: string) =>
  invoke<Session>("auth_switch", { id });

export const authReconnect = () => invoke<Session>("auth_reconnect");

export const authLogout = (id: string) =>
  invoke<Session>("auth_logout", { id });

export const authRename = (id: string, label: string) =>
  invoke<Account[]>("auth_rename", { id, label });

// --- biblioteca (exige conta) -----------------------------------------

export const libraryPlaylists = () =>
  invoke<Playlist[]>("library_playlists");

export const libraryAlbums = () => invoke<Album[]>("library_albums");

export const libraryArtists = () => invoke<Artist[]>("library_artists");

export const likedSongs = (continuation?: string) =>
  invoke<Page<Track>>("liked_songs", { continuation });

// --- capas ---------------------------------------------------------------

/**
 * URL de uma capa.
 *
 * Isto NÃO faz IPC: monta a URL do protocolo custom e o `<img>` cuida do
 * resto — inclusive de esperar o download na primeira vez. Ver
 * `src-tauri/src/protocol.rs`.
 */
export function artUrl(art: ArtRef | null | undefined): string | null {
  return art ? convertFileSrc(art.hash, "ytmart") : null;
}

/** Erros do backend chegam como objeto, não como Error. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as ErrorPayload).message);
  }
  return String(e);
}
