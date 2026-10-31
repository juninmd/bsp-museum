// Subabas de modelos da seção Skins: pastas à esquerda, arquivos no meio e o visualizador 3D
// (o mesmo `.mdl` da antiga aba Recursos) à direita. A subaba de escopo (`p_`, `v_`, `w_`, TR, CT,
// zumbi) filtra a listagem do mod; a subaba "Recursos" mostra todos os `.mdl`, agrupados por pasta.

import { invoke } from "@tauri-apps/api/core";
import { t } from "../../i18n.ts";
import { dirName, entryKey, matchesQuery } from "../../lib/explorer.ts";
import { filterModels, groupByDir, scopeCounts, type ModelScope } from "../../lib/skins-catalog.ts";
import { mountModelViewer } from "../../resources.ts";
import type { MdlSummary } from "../../types.ts";
import type { ModFileEntry } from "../../types/modfiles.ts";
import { emptyHtml, errorHtml, esc, kindBadge, layerChip, loadingHtml, sizeText } from "../explorer/common.ts";
import { absPath, entries, state } from "../explorer/state.ts";
import { createVirtualList } from "../explorer/virtual-list.ts";

export interface ModelCatalogOptions {
  /** subaba de escopo; `null` = todos os modelos do mod (Recursos) */
  scope: ModelScope | null;
}

const ROW_H = 30;

/** seleção lembrada entre trocas de subaba (pasta, busca e modelo aberto) */
interface Memory {
  dir: string | null;
  query: string;
  selected: string | null;
}
const memory = new Map<string, Memory>();

function memoryOf(key: string): Memory {
  let mem = memory.get(key);
  if (!mem) memory.set(key, (mem = { dir: null, query: "", selected: null }));
  return mem;
}

/** HTML da barra de ferramentas do visualizador + tela 3D + painel de skins. */
function viewerHtml(): string {
  return `
    <div class="res-viewer-toolbar" id="res-viewer-toolbar" hidden>
      <label class="chk-seq">
        <span>${esc(t("mdl.sequence"))}</span>
        <select id="res-sequence"></select>
      </label>
      <button id="res-play" type="button">${esc(t("mdl.play"))}</button>
      <span class="frame-track">
        <input id="res-frame" type="range" min="0" max="0" step="1" value="0" aria-label="${esc(t("mdl.frame"))}" />
        <span id="res-frame-ticks" class="frame-ticks" aria-hidden="true"></span>
      </span>
      <span id="res-frame-label" class="hintc">–</span>
      <label class="chk-seq">
        <span>${esc(t("mdl.speed"))}</span>
        <select id="res-speed">
          <option value="0.25">0,25×</option>
          <option value="0.5">0,5×</option>
          <option value="1" selected>1×</option>
          <option value="2">2×</option>
        </select>
      </label>
      <label class="chk-seq" id="res-skin-wrap" hidden>
        <span>${esc(t("mdl.skin"))}</span>
        <select id="res-skin"></select>
      </label>
      <span id="res-anim-note" class="hintc"></span>
    </div>
    <div class="res-viewer-body">
      <div class="res-viewer3d" id="res-viewer3d"></div>
      <aside class="res-panel" id="res-panel" hidden aria-label="${esc(t("mdl.panelAria"))}"></aside>
    </div>`;
}

export function mountModelCatalog(host: HTMLElement, opts: ModelCatalogOptions): () => void {
  if (state.loading) {
    host.innerHTML = loadingHtml(t("exp.files.loading"));
    return () => {};
  }
  if (state.error) {
    host.innerHTML = errorHtml(state.error);
    return () => {};
  }
  if (!state.listing) {
    host.innerHTML = emptyHtml(t("skins.needFolder"));
    return () => {};
  }

  const all = filterModels(entries(), opts.scope);
  if (!all.length) {
    host.innerHTML = emptyHtml(opts.scope ? t(`skins.none.${opts.scope}`) : t("mdl.noneFound"));
    return () => {};
  }

  const key = opts.scope ?? "all";
  const mem = memoryOf(key);
  const groups = groupByDir(all);
  const dirs = [...groups.keys()];
  if (!mem.dir || !groups.has(mem.dir)) mem.dir = dirs[0] ?? null;

  host.innerHTML = `
    <div class="skins-models">
      <div class="exp-filters">
        <input type="search" class="m-query" placeholder="${esc(t("skins.search"))}" aria-label="${esc(t("skins.search"))}" autocomplete="off" value="${esc(mem.query)}" />
        <span class="m-count dim" aria-live="polite"></span>
      </div>
      <div class="res-layout">
        <div class="res-col" id="res-dirs" aria-label="${esc(t("skins.dirsAria"))}"></div>
        <div class="res-col" id="res-files" aria-label="${esc(t("skins.listAria"))}"></div>
        <div class="res-viewer-wrap">${viewerHtml()}</div>
      </div>
    </div>`;

  const q = <E extends HTMLElement>(sel: string) => host.querySelector<E>(sel)!;
  const dirsEl = q<HTMLElement>("#res-dirs");
  const filesEl = q<HTMLElement>("#res-files");
  const countEl = q<HTMLElement>(".m-count");
  const queryInput = q<HTMLInputElement>(".m-query");
  const viewerEl = q<HTMLElement>("#res-viewer3d");
  const toolbar = q<HTMLElement>("#res-viewer-toolbar");

  // ---- coluna de pastas
  function renderDirs() {
    dirsEl.replaceChildren();
    for (const dir of dirs) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "res-item";
      item.classList.toggle("active", !queryInput.value.trim() && dir === mem.dir);
      const label = dir || t("skins.rootDir");
      item.textContent = `${label} (${groups.get(dir)!.length})`;
      item.title = dir;
      item.addEventListener("click", () => {
        mem.dir = dir;
        queryInput.value = "";
        mem.query = "";
        renderDirs();
        renderFiles();
      });
      dirsEl.append(item);
    }
  }

  // ---- lista de arquivos
  let disposeViewer: (() => void) | null = null;
  let token = 0;

  const list = createVirtualList<ModFileEntry>({
    host: filesEl,
    rowHeight: ROW_H,
    ariaLabel: t("skins.listAria"),
    render: (entry) => {
      const row = document.createElement("div");
      row.className = `frow${entryKey(entry) === mem.selected ? " selected" : ""}`;
      const dir = dirName(entry.path);
      row.innerHTML = `${kindBadge("model")}<span class="fname"><b>${esc(entry.name)}</b>${dir ? `<small>${esc(dir)}</small>` : ""}</span>${layerChip(entry.layer)}<span class="fsize">${esc(sizeText(entry.size))}</span>`;
      return row;
    },
    onSelect: (entry) => void openModel(entry),
  });

  function visible(): ModFileEntry[] {
    const query = mem.query.trim();
    if (query) return all.filter((e) => matchesQuery(e, query));
    return groups.get(mem.dir ?? "") ?? [];
  }

  function renderFiles() {
    const shown = visible();
    list.setItems(shown);
    countEl.textContent = shown.length
      ? t("skins.count", shown.length.toLocaleString(t("locale")), all.length.toLocaleString(t("locale")))
      : t("skins.noMatch");
    const at = shown.findIndex((e) => entryKey(e) === mem.selected);
    if (at >= 0) list.setActive(at);
  }

  // ---- visualizador
  async function openModel(entry: ModFileEntry) {
    mem.selected = entryKey(entry);
    const abs = absPath(entry);
    if (!abs) {
      disposeViewer?.();
      disposeViewer = null;
      viewerEl.innerHTML = emptyHtml(t("exp.view.model.inPak"));
      return;
    }
    const mine = ++token;
    disposeViewer?.();
    disposeViewer = null;
    toolbar.hidden = true;
    viewerEl.innerHTML = loadingHtml(t("mdl.decoding"));
    try {
      const model = await invoke<MdlSummary>("load_model", { path: abs });
      if (mine !== token) return;
      viewerEl.replaceChildren();
      const mounted = mountModelViewer(viewerEl, model, abs);
      disposeViewer = () => mounted.dispose();
      toolbar.hidden = model.sequences.length === 0;
    } catch (err) {
      if (mine !== token) return;
      viewerEl.innerHTML = errorHtml(String(err));
    }
  }

  let timer = 0;
  queryInput.addEventListener("input", () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      mem.query = queryInput.value;
      renderDirs();
      renderFiles();
    }, 120);
  });

  renderDirs();
  renderFiles();
  // reabre o modelo que estava aberto antes de trocar de subaba
  if (mem.selected) {
    const prev = all.find((e) => entryKey(e) === mem.selected);
    if (prev) void openModel(prev);
  }

  return () => {
    window.clearTimeout(timer);
    token++;
    disposeViewer?.();
    list.dispose();
  };
}

/** Quantos modelos cada subaba mostraria (usado nos contadores das abas). */
export function modelTotals(): { all: number } & Record<ModelScope, number> {
  const counts = scopeCounts(entries());
  const all = filterModels(entries(), null).length;
  return { all, ...counts };
}
