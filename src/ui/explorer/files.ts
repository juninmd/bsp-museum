// Subaba "Arquivos do mod": lista/árvore filtrável (tipo, nome, camada) com visualizador ao lado.

import { t } from "../../i18n.ts";
import {
  buildTree,
  dirName,
  effectiveKind,
  EMPTY_FILE_FILTER,
  filterAndSort,
  flattenTree,
  totalsByKind,
  type FileFilter,
  type FileSort,
  type TreeRow,
} from "../../lib/explorer.ts";
import type { FileKind, Layer, ModFileEntry } from "../../types/modfiles.ts";
import { emptyHtml, errorHtml, esc, kindBadge, layerChip, loadingHtml, noticeHtml, sizeText, warningsHtml } from "./common.ts";
import { entries as allEntries, state } from "./state.ts";
import { entryKey } from "../../lib/explorer.ts";
import { mountFileViewer, type ViewerDeps } from "./viewers.ts";
import { createVirtualList } from "./virtual-list.ts";

const KINDS: FileKind[] = ["map", "wad", "model", "sprite", "sound", "image", "nav", "demo", "text", "cfg", "pak", "dll", "other"];
const LAYERS: Layer[] = ["mod", "valve", "pak"];
const SORTS: FileSort[] = ["path", "name", "size", "kind"];
/** acima disto, buscar numa árvore não abre todas as pastas sozinha */
const OPEN_ALL_LIMIT = 3000;

// ficam entre montagens (trocar de subaba não zera o filtro)
const filter: FileFilter = { ...EMPTY_FILE_FILTER };
let viewMode: "list" | "tree" = "list";
const openDirs = new Set<string>();
let selectedKey: string | null = null;

type Row = { type: "file"; entry: ModFileEntry } | { type: "dir"; row: TreeRow } | { type: "treefile"; row: TreeRow; entry: ModFileEntry };

export function mountFilesTab(host: HTMLElement, deps: Omit<ViewerDeps, "open">): () => void {
  const listing = state.listing;
  if (state.loading) {
    host.innerHTML = loadingHtml(t("exp.files.loading"));
    return () => {};
  }
  if (state.error) {
    host.innerHTML = errorHtml(state.error);
    return () => {};
  }
  if (!listing) {
    host.innerHTML = emptyHtml(t("exp.needFolder"));
    return () => {};
  }

  const total = listing.entries.length;
  host.innerHTML = `
    <div class="exp-split">
      <section class="exp-listpane" aria-label="${esc(t("exp.files.listAria"))}">
        <div class="exp-chips-row" role="group" aria-label="${esc(t("exp.files.kindChips"))}"></div>
        ${listing.truncated ? noticeHtml(t("exp.files.truncated", total.toLocaleString(t("locale")))) : ""}
        ${warningsHtml(listing.warnings)}
        <div class="exp-filters">
          <input type="search" class="f-query" placeholder="${esc(t("exp.files.search"))}" aria-label="${esc(t("exp.files.search"))}" autocomplete="off" value="${esc(filter.query)}" />
          <select class="f-kind" aria-label="${esc(t("exp.files.kind"))}">
            <option value="">${esc(t("exp.files.allKinds"))}</option>
            ${KINDS.map((k) => `<option value="${k}">${esc(t(`exp.kind.${k}`))}</option>`).join("")}
          </select>
          <select class="f-layer" aria-label="${esc(t("exp.files.layer"))}">
            <option value="">${esc(t("exp.files.allLayers"))}</option>
            ${LAYERS.map((l) => `<option value="${l}">${esc(t(`exp.layer.${l}`))}</option>`).join("")}
          </select>
          <select class="f-sort" aria-label="${esc(t("exp.files.sort"))}">
            ${SORTS.map((s) => `<option value="${s}">${esc(t(`exp.sort.${s}`))}</option>`).join("")}
          </select>
          <button type="button" class="ghost2 f-desc" aria-pressed="false" title="${esc(t("exp.files.desc"))}">↓</button>
          <div class="seg f-mode" role="group" aria-label="${esc(t("exp.files.mode"))}">
            <button type="button" data-mode="list">${esc(t("exp.files.modeList"))}</button>
            <button type="button" data-mode="tree">${esc(t("exp.files.modeTree"))}</button>
          </div>
        </div>
        <div class="exp-list"></div>
        <div class="exp-nomatch" hidden>${emptyHtml(t("exp.files.noMatch"))}</div>
        <div class="exp-count dim" aria-live="polite"></div>
      </section>
      <section class="exp-viewer" aria-live="polite" aria-label="${esc(t("exp.files.viewerAria"))}">
        <div class="exp-state exp-empty">${esc(t("exp.files.pick"))}</div>
      </section>
    </div>`;

  const q = <E extends HTMLElement>(sel: string) => host.querySelector<E>(sel)!;
  const chipsRow = q<HTMLElement>(".exp-chips-row");
  const queryInput = q<HTMLInputElement>(".f-query");
  const kindSel = q<HTMLSelectElement>(".f-kind");
  const layerSel = q<HTMLSelectElement>(".f-layer");
  const sortSel = q<HTMLSelectElement>(".f-sort");
  const descBtn = q<HTMLButtonElement>(".f-desc");
  const modeBtns = host.querySelectorAll<HTMLButtonElement>(".f-mode button");
  const countEl = q<HTMLElement>(".exp-count");
  const viewer = q<HTMLElement>(".exp-viewer");
  const noMatch = q<HTMLElement>(".exp-nomatch");
  let openAllNow = false;

  kindSel.value = filter.kind;
  layerSel.value = filter.layer;
  sortSel.value = filter.sort;

  // ---- visualizador
  let disposeViewer: (() => void) | null = null;
  const viewerDeps: ViewerDeps = { ...deps, open: (e) => openEntry(e, true) };

  function openEntry(entry: ModFileEntry, fromViewer = false) {
    disposeViewer?.();
    selectedKey = entryKey(entry);
    viewer.scrollTop = 0;
    disposeViewer = mountFileViewer(viewer, entry, viewerDeps);
    if (fromViewer) syncSelection(entry);
  }

  /** abriu algo pelo painel (ex.: sprite citado no hud.txt): destaca na lista se estiver lá */
  function syncSelection(entry: ModFileEntry) {
    const key = entryKey(entry);
    const idx = list.items().findIndex((r) => (r.type === "file" || r.type === "treefile") && entryKey(r.entry) === key);
    list.setActive(idx);
  }

  // ---- lista
  const list = createVirtualList<Row>({
    host: q<HTMLElement>(".exp-list"),
    rowHeight: 30,
    ariaLabel: t("exp.files.listAria"),
    render: (item, _i, active) => {
      const row = document.createElement("div");
      const entry = item.type === "dir" ? null : item.entry;
      const selected = entry ? entryKey(entry) === selectedKey : active;
      row.className = `frow${selected ? " selected" : ""}`;
      if (item.type === "dir") {
        const n = item.row.node;
        const open = openDirs.has(n.path) || openAllNow;
        row.style.paddingLeft = `${8 + item.row.depth * 16}px`;
        row.innerHTML = `<span class="caret" aria-hidden="true">${open ? "▾" : "▸"}</span><span class="fname"><b>${esc(n.name)}</b></span><span class="dim fcount">${esc(t("exp.files.nFiles", n.fileCount))}</span><span class="fsize">${esc(sizeText(n.size))}</span>`;
        row.setAttribute("aria-expanded", String(open));
        return row;
      }
      const e = item.entry;
      const depth = item.type === "treefile" ? item.row.depth : 0;
      if (depth) row.style.paddingLeft = `${8 + depth * 16 + 14}px`;
      const dir = item.type === "treefile" ? "" : dirName(e.pak ? `${e.pak}/${e.path}` : e.path);
      row.innerHTML = `${kindBadge(effectiveKind(e))}<span class="fname"><b>${esc(e.name)}</b>${dir ? `<small>${esc(dir)}</small>` : ""}</span>${layerChip(e.layer)}<span class="fsize">${esc(sizeText(e.size))}</span>`;
      return row;
    },
    onSelect: (item) => {
      if (item.type === "dir") {
        const p = item.row.node.path;
        if (openDirs.has(p)) openDirs.delete(p);
        else openDirs.add(p);
        rebuild(true);
        return;
      }
      openEntry(item.entry);
    },
  });

  function rebuild(keepScroll = false) {
    const shown = filterAndSort(listing!.entries, filter);
    const filtering = !!(filter.query.trim() || filter.kind || filter.layer);
    let rows: Row[];
    if (viewMode === "tree") {
      openAllNow = filtering && shown.length <= OPEN_ALL_LIMIT;
      const tree = flattenTree(buildTree(shown), openDirs, openAllNow);
      rows = tree.map((r): Row => (r.node.isDir ? { type: "dir", row: r } : { type: "treefile", row: r, entry: r.node.entry! }));
    } else {
      openAllNow = false;
      rows = shown.map((entry): Row => ({ type: "file", entry }));
    }
    list.setItems(rows, keepScroll);
    const bytes = shown.reduce((s, e) => s + e.size, 0);
    countEl.textContent = shown.length
      ? t("exp.files.count", shown.length.toLocaleString(t("locale")), total.toLocaleString(t("locale")), sizeText(bytes))
      : "";
    noMatch.hidden = shown.length > 0;
    renderChips();
    for (const b of modeBtns) {
      b.classList.toggle("active", b.dataset.mode === viewMode);
      b.setAttribute("aria-pressed", String(b.dataset.mode === viewMode));
    }
  }

  function renderChips() {
    const totals = totalsByKind(listing!.entries);
    chipsRow.innerHTML = KINDS.filter((k) => totals.has(k))
      .map((k) => {
        const v = totals.get(k)!;
        return `<button type="button" class="chip-btn${filter.kind === k ? " active" : ""}" data-kind="${k}" aria-pressed="${filter.kind === k}" title="${esc(sizeText(v.bytes))}">${esc(t(`exp.kind.${k}`))} <b>${v.count.toLocaleString(t("locale"))}</b></button>`;
      })
      .join("");
  }

  let timer = 0;
  queryInput.addEventListener("input", () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      filter.query = queryInput.value;
      rebuild();
    }, 120);
  });
  kindSel.addEventListener("change", () => {
    filter.kind = kindSel.value as FileKind | "";
    rebuild();
  });
  layerSel.addEventListener("change", () => {
    filter.layer = layerSel.value as Layer | "";
    rebuild();
  });
  sortSel.addEventListener("change", () => {
    filter.sort = sortSel.value as FileSort;
    rebuild();
  });
  descBtn.setAttribute("aria-pressed", String(filter.desc));
  descBtn.textContent = filter.desc ? "↑" : "↓";
  descBtn.addEventListener("click", () => {
    filter.desc = !filter.desc;
    descBtn.setAttribute("aria-pressed", String(filter.desc));
    descBtn.textContent = filter.desc ? "↑" : "↓";
    rebuild();
  });
  for (const b of modeBtns) {
    b.addEventListener("click", () => {
      viewMode = b.dataset.mode === "tree" ? "tree" : "list";
      rebuild();
    });
  }
  chipsRow.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-kind]");
    if (!btn) return;
    filter.kind = filter.kind === btn.dataset.kind ? "" : (btn.dataset.kind as FileKind);
    kindSel.value = filter.kind;
    rebuild();
  });

  rebuild();
  // reabre o arquivo que estava selecionado antes de trocar de subaba
  if (selectedKey) {
    const prev = allEntries().find((e) => entryKey(e) === selectedKey);
    if (prev) {
      openEntry(prev);
      syncSelection(prev);
    }
  }

  return () => {
    window.clearTimeout(timer);
    disposeViewer?.();
    list.dispose();
  };
}
