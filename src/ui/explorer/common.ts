// Pedaços de HTML/DOM reaproveitados pelas subabas do Explorador.

import { t } from "../../i18n.ts";
import { formatBytes } from "../../lib/explorer.ts";
import type { FileKind, Layer } from "../../types/modfiles.ts";
import { escapeHtml as esc } from "../dom.ts";

export { esc };

export const loadingHtml = (text = t("common.loading")) =>
  `<div class="exp-state"><div class="spinner" aria-hidden="true"></div><span>${esc(text)}</span></div>`;

export const errorHtml = (msg: string) => `<div class="exp-state exp-error" role="alert">${esc(msg)}</div>`;

export const emptyHtml = (text: string) => `<div class="exp-state exp-empty">${esc(text)}</div>`;

export const noticeHtml = (text: string, tone: "warn" | "info" = "warn") =>
  `<div class="exp-notice" data-tone="${tone}" role="note">${esc(text)}</div>`;

const BADGE: Record<FileKind, string> = {
  map: "BSP",
  wad: "WAD",
  model: "MDL",
  sprite: "SPR",
  sound: "SND",
  image: "IMG",
  nav: "NAV",
  demo: "DEM",
  text: "TXT",
  pak: "PAK",
  dll: "BIN",
  cfg: "CFG",
  other: "···",
};

/** Ícone por tipo: etiqueta curta colorida (o CSS cuida da cor por `data-kind`). */
export function kindBadge(kind: FileKind): string {
  return `<span class="kbadge" data-kind="${kind}" title="${esc(t(`exp.kind.${kind}`))}" aria-hidden="true">${BADGE[kind]}</span>`;
}

export function layerChip(layer: Layer): string {
  return `<span class="chip layer-chip" data-layer="${layer}">${esc(t(`exp.layer.${layer}`))}</span>`;
}

export const sizeText = (bytes: number) => formatBytes(bytes, t("locale"));

/** Tabela chave/valor simples; valores já escapados pelo chamador. */
export function factsHtml(rows: [label: string, valueHtml: string][]): string {
  return `<dl class="exp-facts">${rows
    .filter(([, v]) => v !== "")
    .map(([k, v]) => `<div><dt>${esc(k)}</dt><dd>${v}</dd></div>`)
    .join("")}</dl>`;
}

export function warningsHtml(warnings: readonly string[]): string {
  if (!warnings.length) return "";
  return `<details class="exp-warnings"><summary>${esc(t("exp.warnings", warnings.length))}</summary><ul>${warnings
    .map((w) => `<li>${esc(w)}</li>`)
    .join("")}</ul></details>`;
}

/** Baixa um data-URL pelo navegador (`<a download>`); não passa pelo backend. */
export function downloadDataUrl(dataUrl: string, filename: string): void {
  const a = document.createElement("a");
  a.href = dataUrl;
  a.download = filename;
  document.body.append(a);
  a.click();
  a.remove();
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Pisca "copiado" num botão sem mexer no layout. */
export async function copyWithFeedback(btn: HTMLButtonElement, text: string): Promise<void> {
  const ok = await copyText(text);
  const old = btn.textContent;
  btn.textContent = ok ? t("exp.copied") : t("exp.copyFail");
  window.setTimeout(() => {
    btn.textContent = old;
  }, 1200);
}

/** Cria um elemento a partir de HTML (um só nó raiz). */
export function el<T extends HTMLElement = HTMLElement>(html: string): T {
  const tpl = document.createElement("template");
  tpl.innerHTML = html.trim();
  return tpl.content.firstElementChild as T;
}
