// Seção "Skins": pasta do mod + subabas. Modelos por prefixo (`p_`, `v_`, `w_`), por time
// (TRs, CTs, zumbis), todos os `.mdl` (Recursos), texturas (WAD), sprites, áudio e arquivos do mod.
// Só uma subaba fica montada por vez; trocar de seção ou de subaba desmonta a anterior.

import { t } from "../../i18n.ts";
import { modRootFromMapsDir } from "../../lib/explorer.ts";
import { MODEL_SCOPES, type ModelScope } from "../../lib/skins-catalog.ts";
import { esc, sizeText } from "../explorer/common.ts";
import { mountFilesTab } from "../explorer/files.ts";
import { listCount, mountAudioTab, mountDataTab, mountImagesTab, mountMapsTab, mountSpritesTab, type ListKind } from "../explorer/media-tabs.ts";
import { onChange, pickModRoot, reload, setModRoot, state } from "../explorer/state.ts";
import { mountWadTab, requestWad } from "../explorer/wad.ts";
import { modelTotals, mountModelCatalog } from "./model-catalog.ts";

export interface SkinsHooks {
  /** `settings.last_dir` (costuma ser `<mod>/maps`) */
  getLastDir(): string | null;
}

export interface SkinsController {
  show(): void;
  hide(): void;
  refreshLanguage(): void;
  /** troca de subaba (usado pelos atalhos das outras subabas) */
  select(tab: SkinsTabId): void;
}

export type SkinsTabId = ModelScope | "models" | "wad" | "images" | "sprites" | "audio" | "maps" | "data" | "files";

const TABS: readonly SkinsTabId[] = [...MODEL_SCOPES, "models", "wad", "images", "sprites", "audio", "maps", "data", "files"];

/** subabas de lista de arquivos: qual grupo contam (o contador aparece ao lado do rótulo) */
const LIST_TAB: Partial<Record<SkinsTabId, ListKind>> = { images: "image", sprites: "sprite", audio: "sound", maps: "map", data: "data" };

/** chave i18n do rótulo de cada subaba */
const LABEL: Record<SkinsTabId, string> = {
  p: "skins.tab.p",
  v: "skins.tab.v",
  w: "skins.tab.w",
  tr: "skins.tab.tr",
  ct: "skins.tab.ct",
  zombie: "skins.tab.zombie",
  models: "skins.tab.models",
  wad: "exp.tab.wad",
  images: "exp.tab.images",
  sprites: "exp.tab.sprites",
  audio: "exp.tab.audio",
  maps: "exp.tab.maps",
  data: "exp.tab.data",
  files: "exp.tab.files",
};

/** ícone de cada subaba (traços de 24x24) */
const ICON: Record<SkinsTabId, string> = {
  p: "M12 3l8 4.5v9L12 21l-8-4.5v-9z",
  v: "M12 3l8 4.5v9L12 21l-8-4.5v-9z",
  w: "M12 3l8 4.5v9L12 21l-8-4.5v-9z",
  tr: "M12 3l8 4.5v9L12 21l-8-4.5v-9z",
  ct: "M12 3l8 4.5v9L12 21l-8-4.5v-9z",
  zombie: "M12 3l8 4.5v9L12 21l-8-4.5v-9z",
  models: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  wad: "M4 4h16v16H4zM4 12h16M12 4v16",
  images: "M4 5h16v14H4zM20 16l-5-5-8 8",
  sprites: "M12 3l2.5 6 6.5.5-5 4.3 1.6 6.4L12 17l-5.6 3.2L8 13.8 3 9.5 9.5 9z",
  audio: "M4 10v4M8 6v12M12 3v18M16 8v8M20 11v2",
  maps: "M3 6l6-2 6 2 6-2v14l-6 2-6-2-6 2zM9 4v14M15 6v14",
  data: "M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9zM14 3v6h6",
  files: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
};

/** grupos da navegação lateral: o rótulo aparece antes da primeira aba do grupo */
const GROUP_BEFORE: Partial<Record<SkinsTabId, string>> = { p: "skins.group.models", wad: "skins.group.media", maps: "skins.group.data" };

/** subabas de modelo contam itens: o contador fica ao lado do rótulo */
function isModelTab(id: SkinsTabId): id is ModelScope | "models" {
  return id === "models" || (MODEL_SCOPES as readonly string[]).includes(id);
}

export function initSkins(section: HTMLElement, hooks: SkinsHooks): SkinsController {
  let active: SkinsTabId = "p";
  let visible = false;
  /** o usuário escolheu a pasta à mão: não troca mais sozinho pela do último mapa */
  let manual = false;
  let disposeTab: (() => void) | null = null;

  function shell() {
    section.innerHTML = `
      <div class="exp-toolbar">
        <button type="button" class="exp-pick">${esc(t("exp.pick"))}</button>
        <button type="button" class="ghost2 exp-reload" ${state.modRoot ? "" : "disabled"}>${esc(t("exp.reload"))}</button>
        <span class="exp-root dim" title="${esc(state.modRoot ?? "")}"></span>
        <span class="exp-sum dim" role="status"></span>
      </div>
      <nav class="seg exp-tabs skins-tabs" role="tablist" aria-label="${esc(t("skins.tabsAria"))}">
        ${TABS.map(
          (id) =>
            `${GROUP_BEFORE[id] ? `<div class="tabs-group" role="presentation">${esc(t(GROUP_BEFORE[id]!))}</div>` : ""}<button type="button" role="tab" id="skins-tab-${id}" data-tab="${id}" aria-controls="skins-pane"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${ICON[id]}"/></svg><span class="tab-label">${esc(t(LABEL[id]))}</span>${
              isModelTab(id) || LIST_TAB[id] ? '<span class="tab-count dim" aria-hidden="true"></span>' : ""
            }</button>`,
        ).join("")}
      </nav>
      <div class="exp-pane" id="skins-pane" role="tabpanel"></div>`;
    section.querySelector(".exp-pick")!.addEventListener("click", async () => {
      const dir = await pickModRoot();
      if (!dir) return;
      manual = true;
      await setModRoot(dir);
    });
    section.querySelector(".exp-reload")!.addEventListener("click", () => void reload());
    for (const b of section.querySelectorAll<HTMLButtonElement>("[data-tab]")) {
      b.addEventListener("click", () => select(b.dataset.tab as SkinsTabId));
    }
    summary();
    markTabs();
  }

  function summary() {
    const root = section.querySelector<HTMLElement>(".exp-root");
    const sum = section.querySelector<HTMLElement>(".exp-sum");
    const reloadBtn = section.querySelector<HTMLButtonElement>(".exp-reload");
    if (!root || !sum) return;
    root.textContent = state.modRoot ?? t("exp.noFolder");
    root.title = state.modRoot ?? "";
    if (reloadBtn) reloadBtn.disabled = !state.modRoot || state.loading;
    const l = state.listing;
    if (state.loading) sum.textContent = t("exp.files.loading");
    else if (l) {
      const bytes = l.entries.reduce((s, e) => s + e.size, 0);
      const parts = [t("exp.sum", l.entries.length.toLocaleString(t("locale")), sizeText(bytes))];
      if (l.valve_root) parts.push(t("exp.sum.valve", l.valve_root));
      sum.textContent = parts.join(" · ");
    } else sum.textContent = "";
    renderCounts();
  }

  /** contadores das subabas de modelo (só depois da listagem) */
  function renderCounts() {
    if (!state.listing) return;
    const totals = modelTotals();
    for (const b of section.querySelectorAll<HTMLButtonElement>("[data-tab]")) {
      const id = b.dataset.tab as SkinsTabId;
      const holder = b.querySelector<HTMLElement>(".tab-count");
      if (!holder) continue;
      const list = LIST_TAB[id];
      const n = list ? listCount(list) : id === "models" ? totals.all : isModelTab(id) ? totals[id] : 0;
      holder.textContent = n ? n.toLocaleString(t("locale")) : "";
    }
  }

  function markTabs() {
    for (const b of section.querySelectorAll<HTMLButtonElement>("[data-tab]")) {
      const on = b.dataset.tab === active;
      b.classList.toggle("active", on);
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
      if (on) b.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    }
  }

  function mountActive() {
    disposeTab?.();
    disposeTab = null;
    if (!visible) return;
    const pane = section.querySelector<HTMLElement>("#skins-pane");
    if (!pane) return;
    pane.setAttribute("aria-labelledby", `skins-tab-${active}`);
    pane.replaceChildren();
    const goWad = (entry: Parameters<typeof requestWad>[0]) => {
      requestWad(entry);
      select("wad");
    };
    switch (active) {
      case "wad":
        disposeTab = mountWadTab(pane);
        break;
      case "images":
        disposeTab = mountImagesTab(pane, { gotoWad: goWad, gotoResources: () => select("models") });
        break;
      case "maps":
        disposeTab = mountMapsTab(pane, { gotoWad: goWad, gotoResources: () => select("models") });
        break;
      case "data":
        disposeTab = mountDataTab(pane, { gotoWad: goWad, gotoResources: () => select("models") });
        break;
      case "sprites":
        disposeTab = mountSpritesTab(pane);
        break;
      case "audio":
        disposeTab = mountAudioTab(pane);
        break;
      case "files":
        disposeTab = mountFilesTab(pane, { gotoWad: goWad, gotoResources: () => select("models") });
        break;
      case "models":
        disposeTab = mountModelCatalog(pane, { scope: null });
        break;
      default:
        disposeTab = mountModelCatalog(pane, { scope: active });
        break;
    }
  }

  function select(id: SkinsTabId) {
    active = id;
    markTabs();
    mountActive();
  }

  section.addEventListener("keydown", (e) => {
    const target = e.target as HTMLElement;
    const forward = e.key === "ArrowRight" || e.key === "ArrowDown";
    const backward = e.key === "ArrowLeft" || e.key === "ArrowUp";
    if (!target.matches?.("[data-tab]") || (!forward && !backward)) return;
    e.preventDefault();
    const next = TABS[(TABS.indexOf(active) + (forward ? 1 : TABS.length - 1)) % TABS.length]!;
    select(next);
    section.querySelector<HTMLButtonElement>(`#skins-tab-${next}`)?.focus();
  });

  onChange(() => {
    summary();
    mountActive();
  });

  /** pasta do mod sugerida: a do último mapa aberto */
  function suggestedRoot(): string | null {
    const last = hooks.getLastDir();
    return last ? modRootFromMapsDir(last) : null;
  }

  shell();

  return {
    show() {
      visible = true;
      const wanted = suggestedRoot();
      if (!manual && wanted && wanted !== state.modRoot) void setModRoot(wanted);
      else mountActive();
    },
    hide() {
      visible = false;
      disposeTab?.();
      disposeTab = null;
    },
    refreshLanguage() {
      shell();
      mountActive();
    },
    select,
  };
}
