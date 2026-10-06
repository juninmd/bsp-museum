import { trapTab } from "./dom.ts";

const modal = () => document.querySelector<HTMLElement>("#modal")!;
const scrim = () => document.querySelector<HTMLElement>("#modal-scrim")!;
const title = () => document.querySelector<HTMLElement>("#modal-title")!;
const body = () => document.querySelector<HTMLElement>("#modal-body")!;

let returnFocus: HTMLElement | null = null;
let onClose: (() => void) | null = null;

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    closeModal();
  } else trapTab(modal(), e);
}

/** Abre o diálogo genérico (comparação, auditoria). Devolve o corpo para preencher. */
export function openModal(heading: string, close?: () => void): HTMLElement {
  returnFocus = document.activeElement as HTMLElement | null;
  onClose = close ?? null;
  title().textContent = heading;
  body().replaceChildren();
  modal().hidden = false;
  scrim().hidden = false;
  document.addEventListener("keydown", onKey, true);
  document.querySelector<HTMLButtonElement>("#modal-close")!.focus();
  return body();
}

export function closeModal(): void {
  if (modal().hidden) return;
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
