// Classificação dos `.mdl` do mod para as subabas de Skins: prefixo do nome (`v_`, `w_`, `p_`),
// pasta de jogador (`models/player/…`) e time (TR, CT, zumbi). Só funções puras — sem DOM nem
// backend — testadas em `skins-catalog.test.ts`.
import { dirName, effectiveKind } from "./explorer.ts";
import type { ModFileEntry } from "../types/modfiles.ts";

/** Subaba de modelos: prefixo (`p`/`v`/`w`) ou time (`tr`/`ct`/`zombie`). */
export type ModelScope = "p" | "v" | "w" | "tr" | "ct" | "zombie";

/** Ordem das subabas de modelo na interface. */
export const MODEL_SCOPES: readonly ModelScope[] = ["p", "v", "w", "tr", "ct", "zombie"];

export type TeamId = "tr" | "ct" | "zombie";

/** Nomes (ou pedaços de nome) que identificam o time. Comparados com os tokens do nome/pasta. */
const TR_HINTS = ["terror", "terrorist", "leet", "arctic", "guerilla", "militia", "phoenix", "anarch"];
const CT_HINTS = ["urban", "gign", "gsg9", "sas", "vip", "spetsnaz", "seal", "swat", "fbi", "nighthawk"];
/** modos zumbi usam nomes livres: `zombie_source`, `zumbi`, `zp_`, `zclass_*`… */
const ZOMBIE_HINTS = ["zomb", "zumb", "zp", "zclass"];

/** `models/player/` (com `/` ou `\`) em qualquer posição do caminho. */
const PLAYER_DIR = /(^|[\\/])models[\\/]player([\\/]|$)/i;

/** Nome do arquivo sem extensão, minúsculo (`v_ak47.mdl` -> `v_ak47`). */
export function modelStem(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? name;
  const dot = base.lastIndexOf(".");
  return (dot > 0 ? base.slice(0, dot) : base).toLowerCase();
}

/** Prefixo convencional do modelo (`v_` = 1ª pessoa, `w_` = no mundo, `p_` = jogador). */
export function modelPrefix(name: string): "p" | "v" | "w" | null {
  const stem = modelStem(name);
  if (stem.startsWith("v_")) return "v";
  if (stem.startsWith("w_")) return "w";
  if (stem.startsWith("p_")) return "p";
  return null;
}

/** Modelo de jogador: prefixo `p_` ou arquivo dentro de `models/player/`. */
export function isPlayerModel(path: string, name: string): boolean {
  return modelPrefix(name) === "p" || PLAYER_DIR.test(path.replace(/\\/g, "/"));
}

/** Tokens alfabéticos do nome sem extensão + pasta pai (para achar "urban" em `urban.mdl`). */
function nameTokens(path: string, name: string): string[] {
  const parts = [modelStem(name), ...dirName(path).split("/")];
  return parts
    .flatMap((p) => p.toLowerCase().split(/[^a-z0-9]+/))
    .filter((tok) => tok.length > 1);
}

function hits(tokens: readonly string[], hints: readonly string[]): boolean {
  return tokens.some((tok) => hints.some((h) => tok.startsWith(h)));
}

/**
 * Time do modelo de jogador, pelos nomes clássicos do CS (`urban`, `gign`, `terror`, `leet`…) e
 * pelas pistas de mod zumbi. `null` quando o nome não diz nada — a subaba do time não mostra.
 * Zumbi tem prioridade: um mod zumbi costuma reaproveitar nomes de time nos modelos base.
 */
export function teamOf(path: string, name: string): TeamId | null {
  const tokens = nameTokens(path, name);
  if (hits(tokens, ZOMBIE_HINTS)) return "zombie";
  if (hits(tokens, CT_HINTS)) return "ct";
  if (hits(tokens, TR_HINTS)) return "tr";
  return null;
}

/** O `.mdl` pertence à subaba `scope`? */
export function modelInScope(scope: ModelScope, model: Pick<ModFileEntry, "path" | "name">): boolean {
  switch (scope) {
    case "p":
      return isPlayerModel(model.path, model.name);
    case "v":
    case "w":
      return modelPrefix(model.name) === scope;
    default:
      return teamOf(model.path, model.name) === scope;
  }
}

/**
 * Modelos exibíveis (`.mdl` solto no disco — item de `.pak` não abre no visualizador) que caem na
 * subaba `scope`; `null` devolve todos.
 */
export function filterModels(entries: readonly ModFileEntry[], scope: ModelScope | null): ModFileEntry[] {
  return entries.filter(
    (e) => effectiveKind(e) === "model" && e.layer !== "pak" && (scope === null || modelInScope(scope, e)),
  );
}

/** Modelos agrupados pela pasta, com as pastas em ordem e os arquivos por nome. */
export function groupByDir(models: readonly ModFileEntry[]): Map<string, ModFileEntry[]> {
  const collator = new Intl.Collator("en", { numeric: true, sensitivity: "base" });
  const groups = new Map<string, ModFileEntry[]>();
  for (const m of models) {
    const dir = dirName(m.path);
    const list = groups.get(dir);
    if (list) list.push(m);
    else groups.set(dir, [m]);
  }
  for (const list of groups.values()) list.sort((a, b) => collator.compare(a.name, b.name));
  return new Map([...groups].sort((a, b) => collator.compare(a[0], b[0])));
}

/** Quantos modelos cada subaba mostraria (para o contador das abas). */
export function scopeCounts(entries: readonly ModFileEntry[]): Record<ModelScope, number> {
  const all = filterModels(entries, null);
  const out = { p: 0, v: 0, w: 0, tr: 0, ct: 0, zombie: 0 } as Record<ModelScope, number>;
  for (const scope of MODEL_SCOPES) out[scope] = all.filter((m) => modelInScope(scope, m)).length;
  return out;
}
