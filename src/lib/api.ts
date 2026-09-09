/**
 * Unico lugar do frontend que chama `invoke`.
 *
 * Manter os wrappers tipados aqui significa que uma mudanca de assinatura no
 * Rust quebra o `svelte-check` num arquivo, e nao espalhado por dez views.
 */
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

export interface AppInfo {
  version: string;
  configDir: string;
  cacheDir: string;
}

export interface Config {
  volume: number;
  artCacheMb: number;
  preferOpus: boolean;
  normalizeVolume: boolean;
  lastAccountId: string | null;
}

export interface MemInfo {
  rssMb: number | null;
  processCount: number;
}

/** O Rust serializa em snake_case; convertemos na fronteira. */
function camel<T>(obj: Record<string, unknown>): T {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(obj)) {
    out[k.replace(/_(\w)/g, (_, c: string) => c.toUpperCase())] = v;
  }
  return out as T;
}

function snake(obj: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(obj)) {
    out[k.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`)] = v;
  }
  return out;
}

export async function appInfo(): Promise<AppInfo> {
  return camel(await invoke("app_info"));
}

export async function getConfig(): Promise<Config> {
  return camel(await invoke("get_config"));
}

export async function setConfig(config: Config): Promise<void> {
  await invoke("set_config", { config: snake(config as unknown as Record<string, unknown>) });
}

export async function memInfo(): Promise<MemInfo> {
  return camel(await invoke("mem_info"));
}

/**
 * URL de uma capa.
 *
 * Isto NAO faz IPC: monta a URL do protocolo custom e o `<img>` cuida do
 * resto. Ver `src-tauri/src/protocol.rs` para o porque.
 */
export function artUrl(hash: string | null | undefined): string | null {
  return hash ? convertFileSrc(hash, "ytmart") : null;
}
