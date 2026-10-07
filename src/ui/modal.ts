import { trapTab } from "./dom.ts";

const modal = () => document.querySelector<HTMLElement>("#modal")!;
const scrim = () => document.querySelector<HTMLElement>("#modal-scrim")!;
const title = () => document.querySelector<HTMLElement>("#modal-title")!;
const body = () => document.querySelector<HTMLElement>("#modal-body")!;

let returnFocus: HTMLElement | null = null;
/** identifica o diálogo aberto: trabalho assíncrono antigo não escreve no diálogo de outro */
let generation = 0;
let onClose: (() => void) | null = null;

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    closeModal();
  } else trapTab(modal(), e);
}

/**
 * Abre o diálogo genérico (comparação, auditoria). Devolve o corpo para preencher e
 * `alive()`, que vira falso quando o diálogo foi fechado ou substituído por outro.
 */
export function openModal(heading: string, close?: () => void): { body: HTMLElement; alive: () => boolean } {
  const mine = ++generation;
  returnFocus = document.activeElement as HTMLElement | null;
  onClose = close ?? null;
  title().textContent = heading;
  body().replaceChildren();
  modal().hidden = false;
  scrim().hidden = false;
  document.addEventListener("keydown", onKey, true);
  document.querySelector<HTMLButtonElement>("#modal-close")!.focus();
  return { body: body(), alive: () => mine === generation && !modal().hidden };
}

export function closeModal(): void {
  if (modal().hidden) return;
  generation++;
  modal().hidden = true;
  scrim().hidden = true;
  document.removeEventListener("keydown", onKey, true);
  onClose?.();
  onClose = null;
  returnFocus?.focus();
  returnFocus = null;
}

export function initModal(): void {
  document.querySelector("#modal-close")!.addEventListener("click", closeModal);
  scrim().addEventListener("click", closeModal);
}

export function isModalOpen(): boolean {
  return !modal().hidden;
}
