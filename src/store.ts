import { invoke } from "@tauri-apps/api/core";
import { emptyAnnotation, isEmptyAnnotation, annotationKey } from "./lib/filters.ts";
import type { Annotation, MapSummary, Settings } from "./types.ts";

/** Estado global mínimo: configurações (persistidas no backend) e o acervo carregado. */
export const settings: Settings = { last_dir: null, slots: 32, lang: null, theme: null, annotations: {} };
export let maps: MapSummary[] = [];

export function setMaps(next: MapSummary[]): void {
  maps = next;
}

export function applySettings(loaded: Partial<Settings>): void {
  Object.assign(settings, loaded);
  settings.annotations ??= {};
}

export async function saveSettings(): Promise<void> {
  try {
    await invoke("save_settings", { settings });
  } catch {
    // sem backend (ex.: pré-visualização no navegador) as preferências só valem na sessão
  }
}

export function annotationOf(map: MapSummary): Annotation {
  return settings.annotations[annotationKey(map)] ?? emptyAnnotation();
}

/** Atualiza a anotação do mapa; anotação vazia é removida para o arquivo não crescer à toa. */
export async function updateAnnotation(map: MapSummary, patch: Partial<Annotation>): Promise<Annotation> {
  const next: Annotation = { ...annotationOf(map), ...patch };
  const key = annotationKey(map);
  if (isEmptyAnnotation(next)) delete settings.annotations[key];
  else settings.annotations[key] = next;
  await saveSettings();
  return next;
}
