import { DICT } from "./lib/dict.ts";
import { FINDINGS_EN } from "./lib/findings-en.ts";
import type { Finding, GameMode, Lang } from "./types.ts";

let current: Lang = "pt";

export function detectLang(saved: Lang | null | undefined, navigatorLang?: string): Lang {
  if (saved === "pt" || saved === "en") return saved;
  return (navigatorLang ?? "pt").toLowerCase().startsWith("pt") ? "pt" : "en";
}

export function setLang(lang: Lang): void {
  current = lang;
  if (typeof document !== "undefined") document.documentElement.lang = lang === "pt" ? "pt-BR" : "en";
}

export function getLang(): Lang {
  return current;
}

function fill(template: string, args: readonly (string | number)[]): string {
  return template.replace(/\{(\d+)\}/g, (_, i: string) => String(args[Number(i)] ?? ""));
}

/** Texto da interface no idioma atual. Chave inexistente devolve a própria chave (visível, não quebra). */
export function t(key: string, ...args: (string | number)[]): string {
  const entry = DICT[key];
  if (!entry) return key;
  return fill(entry[current === "pt" ? 0 : 1], args);
}

const MODE_KEYS: Record<GameMode, string> = {
  bomb: "mode.bomb",
  hostage: "mode.hostage",
  assassination: "mode.assassination",
  escape: "mode.escape",
  zombie: "mode.zombie",
  deathmatch: "mode.deathmatch",
  unknown: "mode.unknown",
};

export function modeLabel(mode: GameMode): string {
  return t(MODE_KEYS[mode] ?? "mode.unknown");
}

/** Título/detalhe/dica do achado no idioma atual (português = texto do backend). */
export function findingText(f: Finding): { title: string; detail: string; hint: string } {
  if (current === "pt") return { title: f.title, detail: f.detail, hint: f.hint };
  const variant = f.id === "cs-sem-resgate" ? `${f.id}:${f.args[1] ?? "info"}` : f.id;
  const tpl = FINDINGS_EN[variant];
  if (!tpl) return { title: f.title, detail: f.detail, hint: f.hint };
  const args = [...f.args];
  if (f.id === "prefixo-divergente") {
    args[0] = modeLabel((args[0] ?? "unknown") as GameMode);
    args[1] = modeLabel((args[1] ?? "unknown") as GameMode);
  }
  return { title: fill(tpl.title, args), detail: fill(tpl.detail, args), hint: fill(tpl.hint, args) };
}

/** Título curto do achado só pelo id (usado nas etiquetas do card e nos filtros). */
export function ruleLabel(id: string): string {
  return t(`rule.${id}`);
}

/** Aplica `data-i18n*` ao DOM estático do index.html. */
export function applyStatic(root: ParentNode = document): void {
  for (const el of root.querySelectorAll<HTMLElement>("[data-i18n]")) el.textContent = t(el.dataset.i18n!);
  for (const el of root.querySelectorAll<HTMLElement>("[data-i18n-ph]")) el.setAttribute("placeholder", t(el.dataset.i18nPh!));
  for (const el of root.querySelectorAll<HTMLElement>("[data-i18n-title]")) el.setAttribute("title", t(el.dataset.i18nTitle!));
  for (const el of root.querySelectorAll<HTMLElement>("[data-i18n-aria]")) el.setAttribute("aria-label", t(el.dataset.i18nAria!));
}
