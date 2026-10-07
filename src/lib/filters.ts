import type { Annotation, MapSummary, Problem, Severity } from "../types.ts";

export type SeverityFilter = "" | "critical" | "warn" | "clean";
export type SortKey = "name" | "size" | "spawns" | "area" | "problems";

export interface FilterState {
  query: string;
  mode: string;
  sort: SortKey;
  severity: SeverityFilter;
  /** id de regra do diagnóstico ("" = qualquer) */
  rule: string;
  tag: string;
  favoritesOnly: boolean;
}

export const EMPTY_FILTERS: FilterState = {
  query: "",
  mode: "",
  sort: "name",
  severity: "",
  rule: "",
  tag: "",
  favoritesOnly: false,
};

const SEVERITY_RANK: Record<Severity, number> = { critical: 0, warn: 1, info: 2 };

/** Chave da anotação: nome do arquivo, sem a pasta (sobrevive a mover o acervo). */
export function annotationKey(map: Pick<MapSummary, "path">): string {
  return map.path.split(/[\\/]/).pop() ?? map.path;
}

export function spawnsOf(m: MapSummary): number {
  return m.ct_spawns + m.t_spawns;
}

export function areaOf(m: MapSummary): number {
  return m.bounds ? m.bounds.size[0] * m.bounds.size[1] : 0;
}

/**
 * Problemas do mapa: os do backend + `poucos-spawns` (que depende dos slots da UI)
 * + erro de leitura. É a única definição de "problema" — card, filtro e ordenação usam esta.
 */
export function problemsOf(m: MapSummary, slots: number): Problem[] {
  const out = [...m.problems];
  const spawns = spawnsOf(m);
  if (spawns > 0 && spawns < slots) out.push({ id: "poucos-spawns", severity: "warn" });
  if (m.error) out.push({ id: "erro-leitura", severity: "critical" });
  return out.sort((a, b) => SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity]);
}

/** Só o que merece sinalizar no card: crítico e aviso (info é ruído na galeria). */
export function flagsOf(m: MapSummary, slots: number): Problem[] {
  return problemsOf(m, slots).filter((p) => p.severity !== "info");
}

export function allRules(maps: MapSummary[], slots: number): string[] {
  const ids = new Set<string>();
  for (const m of maps) for (const p of problemsOf(m, slots)) ids.add(p.id);
  return [...ids].sort();
}

export function allTags(annotations: Record<string, Annotation>): string[] {
  const tags = new Set<string>();
  for (const a of Object.values(annotations)) for (const tag of a.tags) tags.add(tag);
  return [...tags].sort((a, b) => a.localeCompare(b));
}

export function filterAndSort(
  maps: MapSummary[],
  f: FilterState,
  annotations: Record<string, Annotation>,
  slots: number,
  modeLabel: (m: MapSummary) => string = (m) => m.mode_label,
): MapSummary[] {
  const q = f.query.trim().toLowerCase();
  const list = maps.filter((m) => {
    if (f.mode && m.mode !== f.mode) return false;
    const note = annotations[annotationKey(m)];
    if (f.favoritesOnly && !note?.favorite) return false;
    if (f.tag && !note?.tags.includes(f.tag)) return false;
    if (f.severity || f.rule) {
      const problems = problemsOf(m, slots);
      if (f.severity === "critical" && !problems.some((p) => p.severity === "critical")) return false;
      if (f.severity === "warn" && !problems.some((p) => p.severity === "warn")) return false;
      if (f.severity === "clean" && problems.some((p) => p.severity !== "info")) return false;
      if (f.rule && !problems.some((p) => p.id === f.rule)) return false;
    }
    if (!q) return true;
    return (
      m.name.toLowerCase().includes(q) ||
      (m.title ?? "").toLowerCase().includes(q) ||
      modeLabel(m).toLowerCase().includes(q) ||
      (note?.tags ?? []).some((tag) => tag.toLowerCase().includes(q)) ||
      (note?.note ?? "").toLowerCase().includes(q)
    );
  });

  const weight = (m: MapSummary) =>
    problemsOf(m, slots).reduce((sum, p) => sum + (p.severity === "critical" ? 100 : p.severity === "warn" ? 10 : 0), 0);
  return [...list].sort((a, b) => {
    switch (f.sort) {
      case "size":
        return b.file_size - a.file_size;
      case "spawns":
        return spawnsOf(b) - spawnsOf(a);
      case "area":
        return areaOf(b) - areaOf(a);
      case "problems":
        return weight(b) - weight(a) || a.name.localeCompare(b.name);
      default:
        return a.name.localeCompare(b.name);
    }
  });
}

/** Normaliza o texto digitado como tag: minúsculas, sem espaços nas pontas, sem vírgula. */
export function normalizeTag(raw: string): string {
  return raw.trim().toLowerCase().replace(/[,;]+/g, " ").replace(/\s+/g, "-").slice(0, 32);
}

export function emptyAnnotation(): Annotation {
  return { favorite: false, tags: [], note: "" };
}

/** Remove anotação vazia (sem favorito, tag ou nota) para o settings não crescer à toa. */
export function isEmptyAnnotation(a: Annotation): boolean {
  return !a.favorite && a.tags.length === 0 && a.note.trim() === "";
}
