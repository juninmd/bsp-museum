// Estado compartilhado do Explorador: pasta do mod, listagem (`list_mod_files`) e utilitários
// para chamar os comandos do backend sempre com a mesma raiz.

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../../i18n.ts";
import { joinPath } from "../../lib/explorer.ts";
import type { ModFileEntry, ModListing } from "../../types/modfiles.ts";

/** Teto de entradas pedido ao backend (a lista é virtual, o limite protege a memória). */
export const MAX_ENTRIES = 100_000;

export interface ExplorerState {
  modRoot: string | null;
  /** pasta de mapas usada por "quais mapas usam" (padrão: `<mod>/maps`) */
  mapsDir: string | null;
  listing: ModListing | null;
  loading: boolean;
  error: string | null;
}

export const state: ExplorerState = { modRoot: null, mapsDir: null, listing: null, loading: false, error: null };

const listeners = new Set<() => void>();
let request = 0;

export function onChange(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function emit() {
  for (const fn of [...listeners]) fn();
}

export function errMsg(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

/** Troca a pasta do mod e relê a listagem. `null` limpa. O último pedido vence. */
export async function setModRoot(root: string | null): Promise<void> {
  const mine = ++request;
  state.modRoot = root;
  state.mapsDir = root ? joinPath(root, "maps") : null;
  state.listing = null;
  state.error = null;
  state.loading = !!root;
  emit();
  if (!root) return;
  try {
    const listing = await invoke<ModListing>("list_mod_files", { modRoot: root, includePaks: true, maxEntries: MAX_ENTRIES });
    if (mine !== request) return;
    state.listing = listing;
  } catch (e) {
    if (mine !== request) return;
    state.error = errMsg(e);
  }
  state.loading = false;
  emit();
}

export function reload(): Promise<void> {
  return setModRoot(state.modRoot);
}

export async function pickModRoot(): Promise<string | null> {
  const dir = await open({ directory: true, multiple: false, title: t("exp.pickTitle") });
  return typeof dir === "string" ? dir : null;
}

export function entries(): readonly ModFileEntry[] {
  return state.listing?.entries ?? [];
}

/** Caminho absoluto de um arquivo solto (mod ou valve); `null` para item de pak. */
export function absPath(e: Pick<ModFileEntry, "path" | "layer" | "origin">): string | null {
  const l = state.listing;
  if (!l || e.layer === "pak") return null;
  const base = e.origin === "valve" ? l.valve_root : l.mod_root;
  return base ? joinPath(base, e.path) : null;
}

/** Argumentos comuns de `read_mod_file`/`analyze_mod_file`/`read_mod_audio`. */
export function fileArgs(e: Pick<ModFileEntry, "path" | "pak" | "origin">): {
  modRoot: string;
  valveRoot: string | null;
  rel: string;
  origin: string;
} {
  const l = state.listing;
  return {
    modRoot: l?.mod_root ?? state.modRoot ?? "",
    valveRoot: l?.valve_root ?? null,
    rel: e.pak ? `${e.pak}/${e.path}` : e.path,
    origin: e.origin,
  };
}
