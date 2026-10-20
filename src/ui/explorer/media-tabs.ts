// Subabas "Sprites" e "Áudio": lista filtrável à esquerda, player à direita.

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../../i18n.ts";
import { baseName, dirName, entryKey, matchesQuery } from "../../lib/explorer.ts";
import type { SpriteInfo } from "../../types/formats.ts";
import type { Layer, ModFileEntry } from "../../types/modfiles.ts";
import { mountAudioPlayer } from "./audio-player.ts";
import { emptyHtml, errorHtml, esc, kindBadge, layerChip, loadingHtml, noticeHtml, sizeText } from "./common.ts";
import { mountSpritePlayer } from "./sprite-player.ts";
import { absPath, entries as allEntries, errMsg, state } from "./state.ts";
import { createVirtualList } from "./virtual-list.ts";

interface Item {
  key: string;
  name: string;
  dir: string;
  layer: Layer | null;
  size: number | null;
  entry: ModFileEntry | null;
  /** caminho absoluto (arquivo escolhido pelo diálogo ou solto no mod) */
  abs: string | null;
}

interface TabConfig {
  kind: "sprite" | "sound";
  /** itens extras (arquivos escolhidos fora do mod) */
  extras: Item[];
  browse: { label: string; extensions: string[] } | null;
  mountViewer(host: HTMLElement, item: Item, alive: () => boolean): () => void;
  /** guarda entre montagens */
  mem: { query: string; layer: Layer | ""; selected: string | null };
}

const ROW_H = 30;
const COLLATOR = new Intl.Collator("en", { numeric: true, sensitivity: "base" });

function toItem(e: ModFileEntry): Item {
  const full = e.pak ? `${e.pak}/${e.path}` : e.path;
  return { key: entryKey(e), name: e.name, dir: dirName(full), layer: e.layer, size: e.size, entry: e, abs: absPath(e) };
}

function mountListTab(host: HTMLElement, cfg: TabConfig): () => void {
  if (state.loading) {
    host.innerHTML = loadingHtml(t("exp.files.loading"));
    return () => {};
  }
  if (!state.listing && !cfg.extras.length) {
    host.innerHTML = state.error ? errorHtml(state.error) : emptyHtml(t("exp.needFolder"));
    return () => {};
  }
  const base = (): Item[] => {
    const fromMod = allEntries()
      .filter((e) => (e.kind === cfg.kind || (cfg.kind === "sound" && e.kind === "sound")) && (cfg.kind !== "sprite" || e.layer !== "pak"))
      .map(toItem);
    return [...cfg.extras, ...fromMod];
  };
  const items = base();

  host.innerHTML = `
    <div class="exp-split">
      <section class="exp-listpane" aria-label="${esc(t(`exp.${cfg.kind}.listAria`))}">
        ${state.listing?.truncated ? noticeHtml(t("exp.files.truncated", state.listing.entries.length.toLocaleString(t("locale")))) : ""}
        <div class="exp-filters">
          <input type="search" class="m-query" placeholder="${esc(t(`exp.${cfg.kind}.search`))}" aria-label="${esc(t(`exp.${cfg.kind}.search`))}" autocomplete="off" value="${esc(cfg.mem.query)}" />
          <select class="m-layer" aria-label="${esc(t("exp.files.layer"))}">
            <option value="">${esc(t("exp.files.allLayers"))}</option>
            <option value="mod">${esc(t("exp.layer.mod"))}</option>
            <option value="valve">${esc(t("exp.layer.valve"))}</option>
            ${cfg.kind === "sound" ? `<option value="pak">${esc(t("exp.layer.pak"))}</option>` : ""}
          </select>
          ${cfg.browse ? `<button type="button" class="ghost2 m-browse">${esc(cfg.browse.label)}</button>` : ""}
        </div>
        <div class="exp-list"></div>
        <div class="exp-count dim" aria-live="polite"></div>
      </section>
      <section class="exp-viewer" aria-live="polite" aria-label="${esc(t("exp.files.viewerAria"))}">
        <div class="exp-state exp-empty">${esc(t(`exp.${cfg.kind}.pick`))}</div>
      </section>
    </div>`;
  const q = <E extends HTMLElement>(sel: string) => host.querySelector<E>(sel)!;
  const queryInput = q<HTMLInputElement>(".m-query");
  const layerSel = q<HTMLSelectElement>(".m-layer");
  const countEl = q<HTMLElement>(".exp-count");
  const viewer = q<HTMLElement>(".exp-viewer");
  layerSel.value = cfg.mem.layer;

  let disposeViewer: (() => void) | null = null;
  let token = 0;
  let current = items;

  function open_(item: Item) {
    disposeViewer?.();
    cfg.mem.selected = item.key;
    const mine = ++token;
    viewer.scrollTop = 0;
    disposeViewer = cfg.mountViewer(viewer, item, () => mine === token);
  }

  const list = createVirtualList<Item>({
    host: q<HTMLElement>(".exp-list"),
    rowHeight: ROW_H,
    ariaLabel: t(`exp.${cfg.kind}.listAria`),
    render: (item) => {
      const row = document.createElement("div");
      row.className = `frow${item.key === cfg.mem.selected ? " selected" : ""}`;
      row.innerHTML = `${kindBadge(cfg.kind)}<span class="fname"><b>${esc(item.name)}</b>${item.dir ? `<small>${esc(item.dir)}</small>` : ""}</span>${item.layer ? layerChip(item.layer) : ""}<span class="fsize">${item.size != null ? esc(sizeText(item.size)) : ""}</span>`;
      return row;
    },
    onSelect: (item) => open_(item),
  });

  function apply() {
    const pool = base();
    const sorted = [...pool].sort(
      (x, y) => Number(!!x.entry) - Number(!!y.entry) || (x.entry ? COLLATOR.compare(`${x.dir}/${x.name}`, `${y.dir}/${y.name}`) : 0),
    );
    current = sorted.filter(
      (i) =>
        (!cfg.mem.layer || i.layer === cfg.mem.layer) &&
        (i.entry ? matchesQuery(i.entry, cfg.mem.query) : cfg.mem.query.split(/\s+/).every((w) => i.name.toLowerCase().includes(w.toLowerCase()))),
    );
    list.setItems(current);
    countEl.textContent = current.length
      ? t(`exp.${cfg.kind}.count`, current.length.toLocaleString(t("locale")), pool.length.toLocaleString(t("locale")))
      : t(`exp.${cfg.kind}.none`);
  }

  let timer = 0;
  queryInput.addEventListener("input", () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      cfg.mem.query = queryInput.value;
      apply();
    }, 120);
  });
  layerSel.addEventListener("change", () => {
    cfg.mem.layer = layerSel.value as Layer | "";
    apply();
  });
  host.querySelector<HTMLButtonElement>(".m-browse")?.addEventListener("click", async () => {
    if (!cfg.browse) return;
    const f = await open({ multiple: false, directory: false, title: cfg.browse.label, filters: [{ name: cfg.browse.label, extensions: cfg.browse.extensions }] });
    if (typeof f !== "string") return;
    const item: Item = { key: `abs:${f}`, name: baseName(f), dir: "", layer: null, size: null, entry: null, abs: f };
    if (!cfg.extras.some((x) => x.key === item.key)) cfg.extras.unshift(item);
    cfg.mem.query = "";
    queryInput.value = "";
    apply();
    list.setActive(0);
    open_(item);
  });

  apply();
  const prev = current.findIndex((i) => i.key === cfg.mem.selected);
  if (prev >= 0) {
    list.setActive(prev);
    open_(current[prev]!);
  }

  return () => {
    window.clearTimeout(timer);
    token++;
    disposeViewer?.();
    list.dispose();
  };
}

// ------------------------------------------------------------------ sprites

const spriteCfg: TabConfig = {
  kind: "sprite",
  extras: [],
  browse: null,
  mem: { query: "", layer: "", selected: null },
  mountViewer(host, item, alive) {
    if (!item.abs) {
      host.innerHTML = emptyHtml(t("exp.view.inPakUnsupported"));
      return () => {};
    }
    host.innerHTML = loadingHtml();
    let dispose: (() => void) | null = null;
    invoke<SpriteInfo>("load_sprite", { path: item.abs }).then(
      (info) => {
        if (alive()) dispose = mountSpritePlayer(host, info, item.name);
      },
      (e: unknown) => {
        if (alive()) host.innerHTML = errorHtml(errMsg(e));
      },
    );
    return () => dispose?.();
  },
};

export function mountSpritesTab(host: HTMLElement): () => void {
  spriteCfg.browse = { label: t("exp.sprite.browse"), extensions: ["spr"] };
  return mountListTab(host, spriteCfg);
}

// ------------------------------------------------------------------ áudio

const audioCfg: TabConfig = {
  kind: "sound",
  extras: [],
  browse: null,
  mem: { query: "", layer: "", selected: null },
  mountViewer(host, item) {
    if (!item.entry) {
      host.innerHTML = emptyHtml(t("exp.view.noPreview", "WAV"));
      return () => {};
    }
    const ext = item.name.toLowerCase().split(".").pop();
    if (ext === "ogg") {
      host.innerHTML = emptyHtml(t("exp.sound.ogg"));
      return () => {};
    }
    return mountAudioPlayer(host, item.entry);
  },
};

export function mountAudioTab(host: HTMLElement): () => void {
  return mountListTab(host, audioCfg);
}
