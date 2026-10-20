// Subaba "Texturas (WAD)": grade de miniaturas carregada sob demanda, busca, filtro de
// transparentes, ampliação com mips, exportação em PNG e "quais mapas usam".

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../../i18n.ts";
import { baseName, safeFileName } from "../../lib/explorer.ts";
import type { TextureUsage, WadEntry, WadListing, WadTexture, WadUsage } from "../../types/formats.ts";
import type { ModFileEntry } from "../../types/modfiles.ts";
import { closeModal, openModal } from "../modal.ts";
import { copyWithFeedback, downloadDataUrl, emptyHtml, errorHtml, esc, factsHtml, loadingHtml, noticeHtml, sizeText } from "./common.ts";
import { absPath, entries as allEntries, errMsg, state } from "./state.ts";

/** Quantas miniaturas são pedidas ao backend ao mesmo tempo. */
const CONCURRENCY = 6;
/** Teto de tiles na grade (WADs reais têm centenas; passar disso pede filtro). */
const MAX_TILES = 3000;

// ficam entre montagens
let selectedWad: string | null = null; // caminho absoluto
let query = "";
let onlyTransparent = false;
let onlyUnused = false;
let kindFilter = "";
/** WADs extras escolhidos pelo diálogo (fora da listagem do mod) */
const extraWads: { label: string; path: string }[] = [];

/** `wad_usage` por (wad, pasta de mapas): a promessa fica guardada para não repetir a varredura. */
const usageCache = new Map<string, Promise<WadUsage>>();
const usageDone = new Map<string, WadUsage>();

function usageKey(wad: string): string {
  return `${wad}\u0000${state.mapsDir ?? ""}`;
}

export function requestWad(entry: ModFileEntry): void {
  const p = absPath(entry);
  if (p) selectedWad = p;
}

interface WadSource {
  label: string;
  path: string;
}

function wadSources(): WadSource[] {
  const out: WadSource[] = [];
  const seen = new Set<string>();
  for (const e of allEntries()) {
    if (e.kind !== "wad" || e.layer === "pak") continue;
    const p = absPath(e);
    if (!p || seen.has(p)) continue;
    seen.add(p);
    out.push({ label: `${e.name}${e.layer === "valve" ? ` (${t("exp.layer.valve")})` : ""}`, path: p });
  }
  // halflife.wad, cstrike.wad e decals.wad primeiro: são os que o jogador procura
  const first = ["halflife.wad", "cstrike.wad", "decals.wad", "liquids.wad", "xeno.wad"];
  out.sort((a, b) => {
    const ia = first.indexOf(baseName(a.path).toLowerCase());
    const ib = first.indexOf(baseName(b.path).toLowerCase());
    return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib) || a.label.localeCompare(b.label);
  });
  for (const x of extraWads) if (!seen.has(x.path)) out.push(x);
  return out;
}

export function mountWadTab(host: HTMLElement): () => void {
  if (state.loading) {
    host.innerHTML = loadingHtml(t("exp.files.loading"));
    return () => {};
  }
  if (!state.listing && !extraWads.length) {
    host.innerHTML = state.error ? errorHtml(state.error) : emptyHtml(t("exp.needFolder"));
    return () => {};
  }
  const sources = wadSources();
  if (!selectedWad || !sources.some((s) => s.path === selectedWad)) selectedWad = sources[0]?.path ?? null;

  host.innerHTML = `
    <div class="wad-toolbar">
      <label class="spr-ctl"><span>${esc(t("exp.wad.file"))}</span>
        <select class="wad-pick" ${sources.length ? "" : "disabled"} aria-label="${esc(t("exp.wad.file"))}">
          ${sources.map((s) => `<option value="${esc(s.path)}">${esc(s.label)}</option>`).join("")}
        </select></label>
      <button type="button" class="ghost2 wad-browse">${esc(t("exp.wad.browse"))}</button>
      <input type="search" class="wad-q" placeholder="${esc(t("exp.wad.search"))}" aria-label="${esc(t("exp.wad.search"))}" autocomplete="off" value="${esc(query)}" />
      <select class="wad-kind" aria-label="${esc(t("exp.wad.kind"))}"></select>
      <label class="chk"><input type="checkbox" class="wad-transp" ${onlyTransparent ? "checked" : ""} /> ${esc(t("exp.wad.onlyTransp"))}</label>
      <label class="chk wad-unused-wrap"><input type="checkbox" class="wad-unused" ${onlyUnused ? "checked" : ""} disabled /> ${esc(t("exp.wad.onlyUnused"))}</label>
      <button type="button" class="ghost2 wad-usage-btn" title="${esc(t("exp.wad.usageTitle"))}">${esc(t("exp.wad.usageRun"))}</button>
      <span class="wad-usage-state dim" role="status" aria-live="polite"></span>
    </div>
    <div class="wad-info dim"></div>
    <div class="wad-grid-wrap"><div class="wad-grid" role="list"></div></div>`;

  const q = <E extends HTMLElement>(sel: string) => host.querySelector<E>(sel)!;
  const pick = q<HTMLSelectElement>(".wad-pick");
  const qInput = q<HTMLInputElement>(".wad-q");
  const kindSel = q<HTMLSelectElement>(".wad-kind");
  const transp = q<HTMLInputElement>(".wad-transp");
  const unusedChk = q<HTMLInputElement>(".wad-unused");
  const usageBtn = q<HTMLButtonElement>(".wad-usage-btn");
  const usageState = q<HTMLElement>(".wad-usage-state");
  const info = q<HTMLElement>(".wad-info");
  const gridWrap = q<HTMLElement>(".wad-grid-wrap");
  const grid = q<HTMLElement>(".wad-grid");
  if (selectedWad) pick.value = selectedWad;

  let disposed = false;
  let load = 0; // invalida o carregamento anterior ao trocar de WAD
  let listing: WadListing | null = null;
  let observer: IntersectionObserver | null = null;
  const thumbs = new Map<string, string>(); // nome -> png (mip0)
  let queue: HTMLElement[] = [];
  let inflight = 0;
  let lastDialog: { alive: () => boolean } | null = null;

  // ---- usos (mapas)
  function usageFor(wad: string): WadUsage | undefined {
    return usageDone.get(usageKey(wad));
  }

  function usageMap(u: WadUsage | undefined): Map<string, TextureUsage> | null {
    if (!u) return null;
    return new Map(u.textures.map((x) => [x.name.toLowerCase(), x]));
  }

  function startUsage(wad: string): Promise<WadUsage> {
    const key = usageKey(wad);
    let p = usageCache.get(key);
    if (!p) {
      if (!state.mapsDir) return Promise.reject(new Error(t("exp.wad.noMapsDir")));
      p = invoke<WadUsage>("wad_usage", { wadPath: wad, mapsDir: state.mapsDir });
      usageCache.set(key, p);
      p.then(
        (u) => usageDone.set(key, u),
        () => usageCache.delete(key), // falhou: permite tentar de novo
      );
    }
    return p;
  }

  function usageStatus() {
    const wad = selectedWad;
    if (!wad) return;
    const u = usageFor(wad);
    unusedChk.disabled = !u;
    if (!u) {
      unusedChk.checked = false;
      onlyUnused = false;
    }
    if (u) {
      usageBtn.textContent = t("exp.wad.usageDone");
      usageState.textContent = t("exp.wad.usageSummary", u.maps_scanned, u.unused_count) + (u.truncated ? ` — ${t("exp.wad.usageTruncated")}` : "");
    }
  }

  usageBtn.addEventListener("click", async () => {
    const wad = selectedWad;
    if (!wad) return;
    usageBtn.disabled = true;
    usageState.innerHTML = `<span class="spinner inline" aria-hidden="true"></span> ${esc(t("exp.wad.usageRunning", state.mapsDir ?? ""))}`;
    try {
      await startUsage(wad);
      if (disposed || wad !== selectedWad) return;
      usageStatus();
      renderGrid();
    } catch (e) {
      if (!disposed) usageState.textContent = `${t("exp.wad.usageFail")}: ${errMsg(e)}`;
    } finally {
      if (!disposed) usageBtn.disabled = false;
    }
  });

  // ---- miniaturas sob demanda
  function pump() {
    while (inflight < CONCURRENCY && queue.length) {
      const tile = queue.shift()!;
      const name = tile.dataset.name!;
      const wad = selectedWad;
      const mine = load;
      if (!wad || !tile.isConnected) continue;
      const cached = thumbs.get(name);
      if (cached) {
        paintThumb(tile, cached);
        continue;
      }
      inflight++;
      invoke<WadTexture>("wad_texture", { path: wad, name, withMips: false })
        .then((tex) => {
          if (mine !== load) return;
          thumbs.set(name, tex.png);
          paintThumb(tile, tex.png);
        })
        .catch((e: unknown) => {
          if (mine !== load) return;
          tile.classList.add("failed");
          tile.title = errMsg(e);
        })
        .finally(() => {
          inflight--;
          if (!disposed) pump();
        });
    }
  }

  function paintThumb(tile: HTMLElement, png: string) {
    const box = tile.querySelector<HTMLElement>(".wad-thumb")!;
    box.innerHTML = `<img alt="" src="${png}" />`;
    tile.classList.add("loaded");
  }

  function observe(tile: HTMLElement) {
    if (!observer) {
      observer = new IntersectionObserver(
        (items) => {
          for (const it of items) {
            if (!it.isIntersecting) continue;
            observer!.unobserve(it.target);
            queue.push(it.target as HTMLElement);
          }
          pump();
        },
        { root: gridWrap, rootMargin: "300px" },
      );
    }
    observer.observe(tile);
  }

  // ---- grade
  function kinds(): string[] {
    return [...new Set((listing?.entries ?? []).map((e) => e.kind))];
  }

  function visible(): WadEntry[] {
    if (!listing) return [];
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    const usage = onlyUnused ? usageMap(usageFor(listing.path)) : null;
    return listing.entries.filter(
      (e) =>
        (!kindFilter || e.kind === kindFilter) &&
        (!onlyTransparent || e.transparent) &&
        terms.every((term) => e.name.toLowerCase().includes(term)) &&
        (!usage || (usage.get(e.name.toLowerCase())?.map_count ?? 0) === 0),
    );
  }

  function renderGrid() {
    observer?.disconnect();
    observer = null;
    queue = [];
    grid.replaceChildren();
    if (!listing) return;
    const all = visible();
    const shown = all.slice(0, MAX_TILES);
    const usage = usageMap(usageFor(listing.path));
    const frag = document.createDocumentFragment();
    for (const e of shown) {
      const tile = document.createElement("button");
      tile.type = "button";
      tile.className = `wad-tile${e.decodable ? "" : " disabled"}${e.transparent ? " transp" : ""}`;
      tile.dataset.name = e.name;
      tile.setAttribute("role", "listitem");
      const used = usage?.get(e.name.toLowerCase());
      const dims = e.width && e.height ? `${e.width}×${e.height}` : t(`exp.wad.k.${e.kind}`);
      const aspect = e.width && e.height ? Math.min(2, Math.max(0.5, e.width / e.height)) : 1;
      tile.innerHTML = `
        <span class="wad-thumb checker" style="aspect-ratio:${aspect}">${e.decodable ? `<span class="spinner" aria-hidden="true"></span>` : `<span class="dim">${esc(t(`exp.wad.k.${e.kind}`))}</span>`}</span>
        <span class="wad-name" title="${esc(e.name)}">${esc(e.name)}</span>
        <span class="wad-dims dim">${esc(dims)}${used ? ` · <span class="${used.map_count ? "" : "bad"}">${esc(t("exp.wad.maps", used.map_count))}</span>` : ""}</span>`;
      tile.setAttribute("aria-label", `${e.name}, ${dims}`);
      if (e.decodable) {
        tile.addEventListener("click", () => void openTexture(e));
        observe(tile);
      } else {
        tile.disabled = true;
        tile.title = e.compressed ? t("exp.wad.compressed") : t("exp.wad.notDecodable");
      }
      frag.append(tile);
    }
    grid.append(frag);
    info.innerHTML = infoText(all.length, shown.length);
    if (!all.length) grid.innerHTML = emptyHtml(t("exp.wad.noMatch"));
  }

  function infoText(matching: number, shown: number): string {
    if (!listing) return "";
    const base = t(
      "exp.wad.summary",
      esc(listing.name),
      esc(listing.magic),
      listing.texture_count.toLocaleString(t("locale")),
      listing.lump_count.toLocaleString(t("locale")),
      esc(sizeText(listing.file_size)),
    );
    const cut = matching > shown ? ` · ${esc(t("exp.wad.cut", shown, matching))}` : matching !== listing.entries.length ? ` · ${esc(t("exp.wad.matching", matching))}` : "";
    return base + cut;
  }

  async function loadWad(path: string) {
    const mine = ++load;
    thumbs.clear();
    queue = [];
    listing = null;
    observer?.disconnect();
    observer = null;
    grid.innerHTML = loadingHtml(t("exp.wad.loading"));
    info.textContent = "";
    usageState.textContent = "";
    usageBtn.textContent = t("exp.wad.usageRun");
    try {
      const l = await invoke<WadListing>("list_wad", { path });
      if (mine !== load || disposed) return;
      listing = l;
      kindSel.replaceChildren(new Option(t("exp.wad.allKinds"), ""));
      for (const k of kinds()) kindSel.append(new Option(t(`exp.wad.k.${k}`), k));
      if (!kinds().includes(kindFilter)) kindFilter = "";
      kindSel.value = kindFilter;
      usageStatus();
      renderGrid();
    } catch (e) {
      if (mine === load && !disposed) grid.innerHTML = errorHtml(errMsg(e));
    }
  }

  // ---- ampliado
  async function openTexture(entry: WadEntry) {
    const wad = selectedWad;
    if (!wad) return;
    const heading = entry.name;
    const dlg = openModal(heading);
    lastDialog = dlg;
    dlg.body.innerHTML = loadingHtml();
    try {
      const tex = await invoke<WadTexture>("wad_texture", { path: wad, name: entry.name, withMips: true });
      if (!dlg.alive()) return;
      const file = `${safeFileName(tex.name)}.png`;
      dlg.body.innerHTML = `
        <div class="wad-big">
          <div class="wad-big-main">
            <div class="img-tools">
              <label class="spr-ctl"><span>${esc(t("exp.spr.zoom"))}</span>
                <select class="big-zoom"><option value="fit">${esc(t("exp.img.fit"))}</option><option value="1">1×</option><option value="2" selected>2×</option><option value="4">4×</option></select></label>
              <button type="button" class="act-dl">${esc(t("exp.wad.exportPng"))}</button>
              <button type="button" class="ghost2 act-copyname">${esc(t("exp.wad.copyName"))}</button>
            </div>
            <div class="img-stage checker"><img class="big-img" alt="${esc(tex.name)}" src="${tex.png}" /></div>
            ${
              tex.mips.length
                ? `<h4>${esc(t("exp.wad.mips"))}</h4><div class="wad-mips">${tex.mips
                    .map(
                      (m) =>
                        `<figure class="checker"><img alt="" src="${m.png}" style="width:${m.width * 2}px" /><figcaption>mip ${m.level} · ${m.width}×${m.height}</figcaption></figure>`,
                    )
                    .join("")}</div>`
                : ""
            }
          </div>
          <div class="wad-big-side">
            ${factsHtml([
              [t("exp.wad.name"), `<code>${esc(tex.name)}</code>`],
              [t("exp.wad.dims"), `${tex.width} × ${tex.height}`],
              [t("exp.wad.type"), esc(t(`exp.wad.k.${tex.kind}`))],
              [t("exp.wad.transp"), esc(tex.transparent ? t("common.yes") : t("common.no"))],
              [t("exp.wad.lump"), esc(sizeText(entry.size))],
            ])}
            <h4>${esc(t("exp.wad.usedBy"))}</h4>
            <div class="wad-usedby" aria-live="polite"></div>
          </div>
        </div>`;
      const img = dlg.body.querySelector<HTMLImageElement>(".big-img")!;
      const applyZoom = (v: string) => {
        img.classList.toggle("fit", v === "fit");
        img.style.width = v === "fit" ? "" : `${tex.width * Number(v)}px`;
      };
      applyZoom("2");
      dlg.body.querySelector<HTMLSelectElement>(".big-zoom")!.addEventListener("change", (e) => applyZoom((e.target as HTMLSelectElement).value));
      dlg.body.querySelector<HTMLButtonElement>(".act-dl")!.addEventListener("click", () => downloadDataUrl(tex.png, file));
      const copyBtn = dlg.body.querySelector<HTMLButtonElement>(".act-copyname")!;
      copyBtn.addEventListener("click", () => void copyWithFeedback(copyBtn, tex.name));
      void fillUsedBy(dlg.body.querySelector<HTMLElement>(".wad-usedby")!, tex.name, wad, dlg.alive);
    } catch (e) {
      if (dlg.alive()) dlg.body.innerHTML = errorHtml(errMsg(e));
    }
  }

  async function fillUsedBy(box: HTMLElement, name: string, wad: string, alive: () => boolean) {
    const show = (u: WadUsage) => {
      const hit = u.textures.find((x) => x.name.toLowerCase() === name.toLowerCase());
      if (!hit || hit.map_count === 0) {
        box.innerHTML = `<p class="dim">${esc(t("exp.wad.unused", u.maps_scanned))}</p>`;
        return;
      }
      box.innerHTML = `<p>${esc(t("exp.wad.usedIn", hit.map_count, u.maps_scanned))}</p>
        <ul class="exp-chips">${hit.maps.map((m) => `<li class="chip">${esc(m)}</li>`).join("")}</ul>
        ${hit.map_count > hit.maps.length ? `<p class="dim">${esc(t("exp.wad.usedCut", hit.maps.length))}</p>` : ""}
        ${u.truncated ? noticeHtml(t("exp.wad.usageTruncated")) : ""}`;
    };
    const done = usageFor(wad);
    if (done) {
      show(done);
      return;
    }
    box.innerHTML = `<button type="button" class="act-usage">${esc(t("exp.wad.usageRun"))}</button><p class="dim">${esc(t("exp.wad.usageHint", state.mapsDir ?? "–"))}</p>`;
    box.querySelector<HTMLButtonElement>(".act-usage")!.addEventListener("click", async () => {
      box.innerHTML = loadingHtml(t("exp.wad.usageRunning", state.mapsDir ?? ""));
      try {
        const u = await startUsage(wad);
        if (!alive()) {
          // o diálogo fechou, mas o resultado enriquece a grade
          if (!disposed && wad === selectedWad) {
            usageStatus();
            renderGrid();
          }
          return;
        }
        show(u);
        if (!disposed && wad === selectedWad) {
          usageStatus();
          renderGrid();
        }
      } catch (e) {
        if (alive()) box.innerHTML = errorHtml(errMsg(e));
      }
    });
  }

  // ---- eventos
  pick.addEventListener("change", () => {
    selectedWad = pick.value;
    void loadWad(pick.value);
  });
  q<HTMLButtonElement>(".wad-browse").addEventListener("click", async () => {
    const f = await open({ multiple: false, directory: false, title: t("exp.wad.browse"), filters: [{ name: "WAD", extensions: ["wad"] }] });
    if (typeof f !== "string") return;
    if (!extraWads.some((x) => x.path === f) && !wadSources().some((x) => x.path === f)) extraWads.push({ label: baseName(f), path: f });
    selectedWad = f;
    // refaz a lista de opções
    pick.replaceChildren(...wadSources().map((s) => new Option(s.label, s.path)));
    pick.disabled = false;
    pick.value = f;
    void loadWad(f);
  });
  let timer = 0;
  qInput.addEventListener("input", () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      query = qInput.value;
      renderGrid();
    }, 120);
  });
  kindSel.addEventListener("change", () => {
    kindFilter = kindSel.value;
    renderGrid();
  });
  transp.addEventListener("change", () => {
    onlyTransparent = transp.checked;
    renderGrid();
  });
  unusedChk.addEventListener("change", () => {
    onlyUnused = unusedChk.checked;
    renderGrid();
  });

  if (selectedWad) void loadWad(selectedWad);
  else grid.innerHTML = emptyHtml(t("exp.wad.none"));

  return () => {
    disposed = true;
    load++;
    window.clearTimeout(timer);
    observer?.disconnect();
    if (lastDialog?.alive()) closeModal();
  };
}
