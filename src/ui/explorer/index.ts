// Seção "Explorador": pasta do mod + subabas (Arquivos, Texturas WAD, Sprites, Áudio).
// Só uma subaba fica montada por vez; trocar de seção ou de subaba desmonta a anterior
// (para o áudio, cancela animações e pedidos pendentes).

import { t } from "../../i18n.ts";
import { modRootFromMapsDir } from "../../lib/explorer.ts";
import { esc, sizeText } from "./common.ts";
import { mountFilesTab } from "./files.ts";
import { mountAudioTab, mountSpritesTab } from "./media-tabs.ts";
import { onChange, pickModRoot, reload, setModRoot, state } from "./state.ts";
import { mountWadTab, requestWad } from "./wad.ts";

export interface ExplorerHooks {
  /** `settings.last_dir` (costuma ser `<mod>/maps`) */
  getLastDir(): string | null;
  /** leva para a seção Recursos (visualizador de modelos) */
  gotoResources(): void;
}

export interface ExplorerController {
  show(): void;
  hide(): void;
  refreshLanguage(): void;
}

type TabId = "files" | "wad" | "sprites" | "audio";
const TABS: TabId[] = ["files", "wad", "sprites", "audio"];

export function initExplorer(section: HTMLElement, hooks: ExplorerHooks): ExplorerController {
  let active: TabId = "files";
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
      <nav class="seg exp-tabs" role="tablist" aria-label="${esc(t("exp.tabsAria"))}">
        ${TABS.map((id) => `<button type="button" role="tab" id="exp-tab-${id}" data-tab="${id}" aria-controls="exp-pane">${esc(t(`exp.tab.${id}`))}</button>`).join("")}
      </nav>
      <div class="exp-pane" id="exp-pane" role="tabpanel"></div>`;
    section.querySelector(".exp-pick")!.addEventListener("click", async () => {
      const dir = await pickModRoot();
      if (!dir) return;
      manual = true;
      await setModRoot(dir);
    });
    section.querySelector(".exp-reload")!.addEventListener("click", () => void reload());
    for (const b of section.querySelectorAll<HTMLButtonElement>("[data-tab]")) {
      b.addEventListener("click", () => selectTab(b.dataset.tab as TabId));
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
    const pane = section.querySelector<HTMLElement>("#exp-pane");
    if (!pane) return;
    pane.setAttribute("aria-labelledby", `exp-tab-${active}`);
    pane.replaceChildren();
    const goWad = (entry: Parameters<typeof requestWad>[0]) => {
      requestWad(entry);
      selectTab("wad");
    };
    switch (active) {
      case "files":
        disposeTab = mountFilesTab(pane, { gotoWad: goWad, gotoResources: hooks.gotoResources });
        break;
      case "wad":
        disposeTab = mountWadTab(pane);
        break;
      case "sprites":
        disposeTab = mountSpritesTab(pane);
        break;
      case "audio":
        disposeTab = mountAudioTab(pane);
        break;
    }
  }

  function selectTab(id: TabId) {
    active = id;
    markTabs();
    mountActive();
  }

  section.addEventListener("keydown", (e) => {
    const target = e.target as HTMLElement;
    if (!target.matches?.("[data-tab]") || (e.key !== "ArrowRight" && e.key !== "ArrowLeft")) return;
    const next = TABS[(TABS.indexOf(active) + (e.key === "ArrowRight" ? 1 : TABS.length - 1)) % TABS.length]!;
    selectTab(next);
    section.querySelector<HTMLButtonElement>(`#exp-tab-${next}`)?.focus();
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
  };
}
