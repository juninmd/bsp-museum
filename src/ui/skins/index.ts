// Seção "Skins": pasta do mod + subabas. Modelos por prefixo (`p_`, `v_`, `w_`), por time
// (TRs, CTs, zumbis), todos os `.mdl` (Recursos), texturas (WAD), sprites, áudio e arquivos do mod.
// Só uma subaba fica montada por vez; trocar de seção ou de subaba desmonta a anterior.

import { t } from "../../i18n.ts";
import { modRootFromMapsDir } from "../../lib/explorer.ts";
import { MODEL_SCOPES, type ModelScope } from "../../lib/skins-catalog.ts";
import { esc, sizeText } from "../explorer/common.ts";
import { mountFilesTab } from "../explorer/files.ts";
import { mountAudioTab, mountSpritesTab } from "../explorer/media-tabs.ts";
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

export type SkinsTabId = ModelScope | "models" | "wad" | "sprites" | "audio" | "files";

const TABS: readonly SkinsTabId[] = [...MODEL_SCOPES, "models", "wad", "sprites", "audio", "files"];

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
  sprites: "exp.tab.sprites",
  audio: "exp.tab.audio",
  files: "exp.tab.files",
};

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
            `<button type="button" role="tab" id="skins-tab-${id}" data-tab="${id}" aria-controls="skins-pane">${esc(t(LABEL[id]))}${
              isModelTab(id) ? '<span class="tab-count dim" aria-hidden="true"></span>' : ""
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
      const n = id === "models" ? totals.all : isModelTab(id) ? totals[id] : 0;
      holder.textContent = n ? n.toLocaleString(t("locale")) : "";
    }
  }

  function markTabs() {
    for (const b of section.querySelectorAll<HTMLButtonElement>("[data-tab]")) {
      const on = b.dataset.tab === active;
      b.classList.toggle("active", on);
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
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
    if (!target.matches?.("[data-tab]") || (e.key !== "ArrowRight" && e.key !== "ArrowLeft")) return;
    const next = TABS[(TABS.indexOf(active) + (e.key === "ArrowRight" ? 1 : TABS.length - 1)) % TABS.length]!;
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
