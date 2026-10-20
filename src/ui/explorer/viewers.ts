// Painel lateral da subaba "Arquivos do mod": escolhe o visualizador pelo tipo do arquivo
// (imagem, sprite, áudio, texto/hex, nav, demo, pak, liblist, hud, detail, mapcycle…).

import { invoke } from "@tauri-apps/api/core";
import { t } from "../../i18n.ts";
import {
  base64ToBytes,
  baseName,
  classifyName,
  dirName,
  effectiveKind,
  extOf,
  formatDuration,
  hexDump,
  matchesQuery,
} from "../../lib/explorer.ts";
import type { ImageInfo, SpriteInfo } from "../../types/formats.ts";
import {
  NAV_CROUCH,
  NAV_JUMP,
  NAV_NO_JUMP,
  NAV_PRECISE,
  type Analysis,
  type DemoInfo,
  type FilePreview,
  type HudSprites,
  type Liblist,
  type ModFileAnalysis,
  type ModFileEntry,
  type NavInfo,
  type PakEntry,
  type PakListing,
} from "../../types/modfiles.ts";
import { mountAudioPlayer } from "./audio-player.ts";
import {
  copyWithFeedback,
  emptyHtml,
  errorHtml,
  esc,
  factsHtml,
  kindBadge,
  layerChip,
  loadingHtml,
  noticeHtml,
  sizeText,
  warningsHtml,
} from "./common.ts";
import { mountSpritePlayer } from "./sprite-player.ts";
import { absPath, entries as allEntries, errMsg, fileArgs } from "./state.ts";
import { createVirtualList } from "./virtual-list.ts";

export interface ViewerDeps {
  /** abre outra entrada (membro de pak, sprite citado no hud.txt…) no mesmo painel */
  open(entry: ModFileEntry): void;
  /** leva à subaba Texturas (WAD) com este arquivo selecionado */
  gotoWad(entry: ModFileEntry): void;
  /** troca para a seção Recursos (visualizador de modelos) */
  gotoResources(): void;
}

/** Teto de bytes lidos para texto cru e para o hex inicial/ampliado. */
const RAW_TEXT_BYTES = 256 * 1024;
const HEX_FIRST = 4096;
const HEX_MORE = 64 * 1024;
/** imagens soltas de formato do navegador (png/jpg/gif) vêm por `read_mod_file` */
const IMAGE_READ_BYTES = 8 * 1024 * 1024;
const NAV_AREA_ROWS = 300;

const MIME: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", bmp: "image/bmp" };

function lookupByPath(rel: string): ModFileEntry | undefined {
  const want = rel.toLowerCase();
  return allEntries().find((e) => !e.pak && e.path.toLowerCase() === want);
}

function entryFromPak(pakEntry: ModFileEntry, member: string, size: number): ModFileEntry {
  return {
    path: member,
    name: baseName(member),
    kind: classifyName(baseName(member)),
    size,
    layer: "pak",
    origin: pakEntry.origin,
    pak: pakEntry.pak ? `${pakEntry.pak}/${pakEntry.path}` : pakEntry.path,
  };
}

export function mountFileViewer(host: HTMLElement, entry: ModFileEntry, deps: ViewerDeps): () => void {
  const kind = effectiveKind(entry);
  const disposers: (() => void)[] = [];
  let disposed = false;
  const alive = () => !disposed;
  const shownPath = entry.pak ? `${entry.pak} › ${entry.path}` : entry.path;

  host.innerHTML = `
    <header class="exp-fhead">
      ${kindBadge(kind)}
      <div class="exp-fhead-main">
        <h3 class="exp-title" title="${esc(shownPath)}">${esc(entry.name)}</h3>
        <div class="dim exp-path">${esc(shownPath)}</div>
      </div>
      <div class="exp-fhead-chips">${layerChip(entry.layer)}<span class="chip">${esc(sizeText(entry.size))}</span></div>
    </header>
    <div class="exp-actions">
      <button type="button" class="ghost2 act-copy">${esc(t("exp.copyPath"))}</button>
      <button type="button" class="ghost2 act-raw" aria-expanded="false">${esc(t("exp.viewBytes"))}</button>
    </div>
    <div class="exp-fbody"></div>
    <div class="exp-raw" hidden></div>`;
  const body = host.querySelector<HTMLElement>(".exp-fbody")!;
  const rawHost = host.querySelector<HTMLElement>(".exp-raw")!;
  const copyBtn = host.querySelector<HTMLButtonElement>(".act-copy")!;
  const rawBtn = host.querySelector<HTMLButtonElement>(".act-raw")!;

  copyBtn.addEventListener("click", () => void copyWithFeedback(copyBtn, absPath(entry) ?? shownPath));
  rawBtn.addEventListener("click", () => {
    const show = rawHost.hidden;
    rawHost.hidden = !show;
    rawBtn.setAttribute("aria-expanded", String(show));
    if (show && !rawHost.dataset.loaded) {
      rawHost.dataset.loaded = "1";
      void loadRaw(rawHost, entry, alive);
    }
  });

  // ---- escolha do visualizador
  const swap = (fn: (() => void) | void) => {
    if (fn) disposers.push(fn);
  };
  const absolute = absPath(entry);
  switch (kind) {
    case "image":
      void showImage(body, entry, absolute, alive);
      break;
    case "sprite":
      if (absolute) void showSprite(body, entry, absolute, alive, swap);
      else body.innerHTML = noticeHtml(t("exp.view.inPakUnsupported"));
      break;
    case "sound":
      swap(mountAudioPlayer(body, entry));
      break;
    case "model":
      showModel(body, entry, deps);
      break;
    case "wad":
      showWad(body, entry, absolute, deps);
      break;
    case "map":
      body.innerHTML = noticeHtml(t("exp.view.map"), "info");
      break;
    case "pak":
    case "nav":
    case "demo":
    case "text":
    case "cfg":
      void showAnalysis(body, entry, deps, alive, swap);
      break;
    default:
      void showBinary(body, entry, alive);
  }

  return () => {
    disposed = true;
    for (const d of disposers) d();
  };
}

// ------------------------------------------------------------------ imagem

async function showImage(body: HTMLElement, entry: ModFileEntry, absolute: string | null, alive: () => boolean) {
  body.innerHTML = loadingHtml();
  const ext = extOf(entry.name);
  try {
    let src: string;
    let facts: [string, string][] = [];
    let warn = "";
    if (ext in MIME && ext !== "bmp") {
      const p = await invoke<FilePreview>("read_mod_file", { ...fileArgs(entry), maxBytes: IMAGE_READ_BYTES });
      if (!alive()) return;
      if (p.truncated || !p.base64) throw new Error(t("exp.view.imageTooBig"));
      src = `data:${MIME[ext]};base64,${p.base64}`;
    } else if (absolute && ["bmp", "tga", "lmp"].includes(ext)) {
      const info = await invoke<ImageInfo>("load_image", { path: absolute });
      if (!alive()) return;
      src = info.png;
      facts = [
        [t("exp.img.format"), esc(info.format.toUpperCase())],
        [t("exp.img.size"), `${info.width} × ${info.height}`],
        [t("exp.img.bpp"), `${info.bits_per_pixel} bit`],
        [t("exp.img.alpha"), esc(info.has_alpha ? t("common.yes") : t("common.no"))],
        [t("exp.img.rle"), esc(info.rle ? t("common.yes") : t("common.no"))],
        [t("exp.img.palette"), esc(t(`exp.img.palette.${info.palette}`))],
      ];
      if (info.palette === "grayscale") warn = noticeHtml(t("exp.img.grayNote"), "info");
    } else if (ext === "bmp") {
      // dentro de um pak: o WebView desenha BMP direto
      const p = await invoke<FilePreview>("read_mod_file", { ...fileArgs(entry), maxBytes: IMAGE_READ_BYTES });
      if (!alive()) return;
      if (p.truncated || !p.base64) throw new Error(t("exp.view.imageTooBig"));
      src = `data:image/bmp;base64,${p.base64}`;
    } else {
      body.innerHTML = noticeHtml(entry.layer === "pak" ? t("exp.view.inPakUnsupported") : t("exp.view.noPreview", ext.toUpperCase()));
      return;
    }
    body.innerHTML = `
      <div class="img-tools">
        <label class="spr-ctl"><span>${esc(t("exp.spr.zoom"))}</span>
          <select class="img-zoom"><option value="fit">${esc(t("exp.img.fit"))}</option><option value="1">1×</option><option value="2">2×</option><option value="4">4×</option></select></label>
      </div>
      ${warn}
      <div class="img-stage checker"><img class="img-el fit" alt="${esc(entry.name)}" src="${src}" /></div>
      ${factsHtml(facts)}`;
    const img = body.querySelector<HTMLImageElement>(".img-el")!;
    body.querySelector<HTMLSelectElement>(".img-zoom")!.addEventListener("change", (e) => {
      const v = (e.target as HTMLSelectElement).value;
      img.classList.toggle("fit", v === "fit");
      img.style.width = v === "fit" ? "" : `${img.naturalWidth * Number(v)}px`;
    });
    if (!facts.length) {
      img.addEventListener("load", () => {
        body.insertAdjacentHTML("beforeend", factsHtml([[t("exp.img.size"), `${img.naturalWidth} × ${img.naturalHeight}`]]));
      });
    }
  } catch (e) {
    if (alive()) body.innerHTML = errorHtml(errMsg(e));
  }
}

// ------------------------------------------------------------------ sprite

async function showSprite(
  body: HTMLElement,
  entry: ModFileEntry,
  path: string,
  alive: () => boolean,
  swap: (fn: () => void) => void,
) {
  body.innerHTML = loadingHtml();
  try {
    const info = await invoke<SpriteInfo>("load_sprite", { path });
    if (!alive()) return;
    swap(mountSpritePlayer(body, info, entry.name));
  } catch (e) {
    if (alive()) body.innerHTML = errorHtml(errMsg(e));
  }
}

// ------------------------------------------------------------------ modelo e WAD

function showModel(body: HTMLElement, entry: ModFileEntry, deps: ViewerDeps) {
  const where = entry.layer === "pak" ? t("exp.view.model.inPak") : t("exp.view.model.hint");
  body.innerHTML = `${noticeHtml(where, "info")}
    <button type="button" class="act-res">${esc(t("exp.view.model.open"))}</button>`;
  body.querySelector<HTMLButtonElement>(".act-res")!.addEventListener("click", deps.gotoResources);
}

function showWad(body: HTMLElement, entry: ModFileEntry, absolute: string | null, deps: ViewerDeps) {
  if (!absolute) {
    body.innerHTML = noticeHtml(t("exp.view.inPakUnsupported"));
    return;
  }
  body.innerHTML = `${noticeHtml(t("exp.view.wad"), "info")}
    <button type="button" class="act-wad">${esc(t("exp.view.wad.open"))}</button>`;
  body.querySelector<HTMLButtonElement>(".act-wad")!.addEventListener("click", () => deps.gotoWad(entry));
}

// ------------------------------------------------------------------ binário / hex

async function showBinary(body: HTMLElement, entry: ModFileEntry, alive: () => boolean) {
  body.innerHTML = `<div class="exp-raw-inline"></div>`;
  await loadRaw(body.firstElementChild as HTMLElement, entry, alive);
}

/** Texto cru (quando o backend diz que é texto) ou despejo hexadecimal. */
async function loadRaw(host: HTMLElement, entry: ModFileEntry, alive: () => boolean, hexBytes = HEX_FIRST) {
  host.innerHTML = loadingHtml();
  try {
    const p = await invoke<FilePreview>("read_mod_file", {
      ...fileArgs(entry),
      maxBytes: effectiveKind(entry) === "text" || effectiveKind(entry) === "cfg" ? RAW_TEXT_BYTES : hexBytes,
    });
    if (!alive()) return;
    const cut = p.truncated ? noticeHtml(t("exp.raw.truncated", sizeText(p.bytes_read), sizeText(p.size)), "info") : "";
    if (p.is_text && p.text != null) {
      host.innerHTML = `${cut}<pre class="exp-pre" tabindex="0">${esc(p.text)}</pre>`;
      return;
    }
    const bytes = base64ToBytes(p.base64 ?? "");
    host.innerHTML = `${cut}<pre class="exp-pre hex" tabindex="0">${esc(hexDump(bytes))}</pre>
      ${p.truncated && hexBytes < HEX_MORE ? `<button type="button" class="ghost2 act-more">${esc(t("exp.raw.more"))}</button>` : ""}`;
    host.querySelector<HTMLButtonElement>(".act-more")?.addEventListener("click", () => void loadRaw(host, entry, alive, HEX_MORE));
  } catch (e) {
    if (alive()) host.innerHTML = errorHtml(errMsg(e));
  }
}

// ------------------------------------------------------------------ análise por formato

async function showAnalysis(
  body: HTMLElement,
  entry: ModFileEntry,
  deps: ViewerDeps,
  alive: () => boolean,
  swap: (fn: () => void) => void,
) {
  body.innerHTML = loadingHtml();
  try {
    const res = await invoke<ModFileAnalysis>("analyze_mod_file", fileArgs(entry));
    if (!alive()) return;
    renderAnalysis(body, entry, res.analysis, deps, swap);
  } catch (e) {
    if (alive()) body.innerHTML = errorHtml(errMsg(e));
  }
}

function renderAnalysis(
  body: HTMLElement,
  entry: ModFileEntry,
  a: Analysis,
  deps: ViewerDeps,
  swap: (fn: () => void) => void,
) {
  switch (a.type) {
    case "nav":
      body.innerHTML = navHtml(a);
      return;
    case "demo":
      body.innerHTML = demoHtml(a);
      return;
    case "pak":
      mountPak(body, entry, a, deps, swap);
      return;
    case "liblist":
      body.innerHTML = liblistHtml(a);
      return;
    case "hud_sprites":
      body.innerHTML = hudHtml(a);
      wireOpenButtons(body, deps);
      return;
    case "detail_textures":
      body.innerHTML = `<h4>${esc(t("exp.an.detail", a.entries.length))}</h4>${tableHtml(
        [t("exp.an.texture"), t("exp.an.detailTex"), t("exp.an.scale")],
        a.entries.map((d) => [`<code>${esc(d.texture)}</code>`, `<code>${esc(d.detail_path)}</code>`, `${d.x_scale} × ${d.y_scale}`]),
      )}${skipped(a.skipped)}${warningsHtml(a.warnings)}`;
      return;
    case "map_cycle":
      body.innerHTML = `<h4>${esc(t("exp.an.mapcycle", a.entries.length))}</h4>${tableHtml(
        [t("exp.an.map"), t("exp.an.minPlayers"), t("exp.an.maxPlayers")],
        a.entries.map((m) => [`<code>${esc(m.map)}</code>`, m.min_players == null ? "–" : String(m.min_players), m.max_players == null ? "–" : String(m.max_players)]),
      )}${warningsHtml(a.warnings)}`;
      return;
    case "text":
      body.innerHTML = `${factsHtml([
        [t("exp.an.lines"), String(a.line_count)],
        [t("exp.an.bytes"), sizeText(a.bytes)],
      ])}${a.truncated ? noticeHtml(t("exp.an.textTruncated"), "info") : ""}<pre class="exp-pre" tabindex="0">${esc(a.text)}</pre>`;
      return;
    case "audio":
      // não chega aqui (áudio tem player próprio), mas o tipo é exaustivo
      body.innerHTML = emptyHtml(t("exp.view.noAnalysis"));
      return;
    default:
      body.innerHTML = emptyHtml(t("exp.view.noAnalysis"));
  }
}

const skipped = (n: number) => (n > 0 ? noticeHtml(t("exp.an.skipped", n), "info") : "");

function tableHtml(head: string[], rows: string[][]): string {
  if (!rows.length) return emptyHtml(t("exp.an.noRows"));
  return `<div class="table-wrap"><table class="table exp-table"><thead><tr>${head.map((h) => `<th>${esc(h)}</th>`).join("")}</tr></thead><tbody>${rows
    .map((r) => `<tr>${r.map((c) => `<td>${c}</td>`).join("")}</tr>`)
    .join("")}</tbody></table></div>`;
}

function navFlags(attr: number): string {
  const f: string[] = [];
  if (attr & NAV_CROUCH) f.push(t("exp.nav.crouch"));
  if (attr & NAV_JUMP) f.push(t("exp.nav.jump"));
  if (attr & NAV_PRECISE) f.push(t("exp.nav.precise"));
  if (attr & NAV_NO_JUMP) f.push(t("exp.nav.noJump"));
  return f.join(", ");
}

function navHtml(n: NavInfo): string {
  const match =
    n.bsp_size_matches == null ? "" : n.bsp_size_matches ? t("exp.nav.bspMatch") : t("exp.nav.bspMismatch");
  const b = n.bounds;
  const facts = factsHtml([
    [t("exp.nav.version"), String(n.version)],
    [t("exp.nav.areas"), n.area_count.toLocaleString(t("locale"))],
    [t("exp.nav.connections"), n.connection_count.toLocaleString(t("locale"))],
    [t("exp.nav.hiding"), n.hiding_spot_count.toLocaleString(t("locale"))],
    [t("exp.nav.approach"), String(n.approach_count)],
    [t("exp.nav.encounters"), String(n.encounter_count)],
    [t("exp.nav.bspSize"), n.bsp_size == null ? "" : `${sizeText(n.bsp_size)} ${match ? `<span class="${n.bsp_size_matches ? "ok" : "bad"}">${esc(match)}</span>` : ""}`],
    [t("exp.nav.bounds"), b ? `<code>${b[0].toFixed(0)}, ${b[1].toFixed(0)}, ${b[2].toFixed(0)}</code> → <code>${b[3].toFixed(0)}, ${b[4].toFixed(0)}, ${b[5].toFixed(0)}</code>` : ""],
  ]);
  const places = n.places.length
    ? `<h4>${esc(t("exp.nav.places", n.places.length))}</h4><ul class="exp-chips">${n.places.map((p) => `<li class="chip">${esc(p)}</li>`).join("")}</ul>`
    : "";
  const parsed = n.parsed ? "" : noticeHtml(t("exp.nav.unparsed"));
  const rows = n.areas.slice(0, NAV_AREA_ROWS).map((a) => [
    String(a.id),
    esc(a.place ?? ""),
    esc(navFlags(a.attributes)),
    `${(a.max[0] - a.min[0]).toFixed(0)} × ${(a.max[1] - a.min[1]).toFixed(0)}`,
    String(a.connections.filter((c) => c > 0).length),
    String(a.hiding_spots),
  ]);
  const areas = n.areas.length
    ? `<h4>${esc(t("exp.nav.areaList"))}</h4>${tableHtml(
        [t("exp.nav.id"), t("exp.nav.place"), t("exp.nav.attrs"), t("exp.nav.extent"), t("exp.nav.links"), t("exp.nav.hidingShort")],
        rows,
      )}${n.areas.length > NAV_AREA_ROWS || n.areas_truncated ? noticeHtml(t("exp.nav.areasCut", NAV_AREA_ROWS, n.area_count), "info") : ""}`
    : "";
  return `${parsed}${facts}${places}${areas}${warningsHtml(n.warnings)}`;
}

function demoHtml(d: DemoInfo): string {
  const facts = factsHtml([
    [t("exp.demo.map"), `<code>${esc(d.map_name)}</code>`],
    [t("exp.demo.gameDir"), `<code>${esc(d.game_dir)}</code>`],
    [t("exp.demo.protocol"), `demo ${d.demo_protocol} · net ${d.network_protocol}${d.network_version_hint ? ` (${esc(d.network_version_hint)})` : ""}`],
    [t("exp.demo.duration"), esc(formatDuration(d.duration_secs))],
    [t("exp.demo.frames"), d.frames.toLocaleString(t("locale"))],
    [t("exp.demo.crc"), `<code>${(d.map_crc >>> 0).toString(16).padStart(8, "0")}</code>`],
    [t("exp.demo.size"), sizeText(d.file_size)],
  ]);
  const rows = d.entries.map((e) => [
    String(e.index + 1),
    esc(e.title || "–"),
    esc(t(`exp.demo.kind.${e.kind_name}`)),
    esc(formatDuration(e.duration_secs)),
    String(e.frames),
  ]);
  return `${facts}<h4>${esc(t("exp.demo.segments", d.entries.length))}</h4>${tableHtml(
    ["#", t("exp.demo.title"), t("exp.demo.type"), t("exp.demo.duration"), t("exp.demo.frames")],
    rows,
  )}${warningsHtml(d.warnings)}`;
}

function liblistHtml(l: Liblist): string {
  const facts = factsHtml([
    [t("exp.lib.game"), esc(l.game ?? "")],
    [t("exp.lib.type"), esc(l.game_type ?? "")],
    [t("exp.lib.version"), esc(l.version ?? "")],
    [t("exp.lib.startmap"), l.startmap ? `<code>${esc(l.startmap)}</code>` : ""],
    [t("exp.lib.trainmap"), l.trainmap ? `<code>${esc(l.trainmap)}</code>` : ""],
    [t("exp.lib.gamedll"), l.gamedll ? `<code>${esc(l.gamedll)}</code>` : ""],
    [t("exp.lib.gamedllLinux"), l.gamedll_linux ? `<code>${esc(l.gamedll_linux)}</code>` : ""],
    [t("exp.lib.urlInfo"), esc(l.url_info ?? "")],
    [t("exp.lib.urlDl"), esc(l.url_dl ?? "")],
  ]);
  return `${facts}<h4>${esc(t("exp.lib.all", l.entries.length))}</h4>${tableHtml(
    [t("exp.an.key"), t("exp.an.value")],
    l.entries.map((e) => [`<code>${esc(e.key)}</code>`, esc(e.value)]),
  )}${warningsHtml(l.warnings)}`;
}

function hudHtml(h: HudSprites): string {
  const rows = h.entries.map((s) => {
    const target = lookupByPath(s.sprite_path);
    const open = target
      ? `<button type="button" class="link act-open" data-path="${esc(s.sprite_path)}">${esc(s.sprite_path)}</button>`
      : `<span class="dim" title="${esc(t("exp.an.spriteMissing"))}">${esc(s.sprite_path)}</span>`;
    return [`<code>${esc(s.name)}</code>`, String(s.resolution), open, `${s.x}, ${s.y}`, `${s.width} × ${s.height}`];
  });
  return `${h.declared_count != null ? `<p class="dim">${esc(t("exp.an.hudDeclared", h.declared_count, h.entries.length))}</p>` : ""}${tableHtml(
    [t("exp.an.name"), t("exp.an.res"), t("exp.an.sprite"), t("exp.an.pos"), t("exp.an.rect")],
    rows,
  )}${skipped(h.skipped)}${warningsHtml(h.warnings)}`;
}

function wireOpenButtons(body: HTMLElement, deps: ViewerDeps) {
  for (const b of body.querySelectorAll<HTMLButtonElement>(".act-open")) {
    b.addEventListener("click", () => {
      const e = lookupByPath(b.dataset.path ?? "");
      if (e) deps.open(e);
    });
  }
}

// ------------------------------------------------------------------ pak

function mountPak(body: HTMLElement, entry: ModFileEntry, p: PakListing, deps: ViewerDeps, swap: (fn: () => void) => void) {
  const total = p.entries.reduce((s, e) => s + e.size, 0);
  body.innerHTML = `
    ${factsHtml([
      [t("exp.pak.members"), p.entries.length.toLocaleString(t("locale"))],
      [t("exp.pak.content"), sizeText(total)],
      [t("exp.pak.file"), sizeText(p.file_size)],
    ])}
    ${p.skipped ? noticeHtml(t("exp.an.skipped", p.skipped), "info") : ""}
    ${warningsHtml(p.warnings)}
    <input type="search" class="pak-search" placeholder="${esc(t("exp.pak.search"))}" aria-label="${esc(t("exp.pak.search"))}" autocomplete="off" />
    <div class="pak-list"></div>
    <div class="dim pak-count"></div>`;
  const search = body.querySelector<HTMLInputElement>(".pak-search")!;
  const count = body.querySelector<HTMLElement>(".pak-count")!;
  let shown = p.entries;
  const list = createVirtualList<PakEntry>({
    host: body.querySelector<HTMLElement>(".pak-list")!,
    rowHeight: 28,
    ariaLabel: t("exp.pak.members"),
    render: (m, _i, selected) => {
      const row = document.createElement("div");
      row.className = `frow${selected ? " selected" : ""}`;
      const k = classifyName(baseName(m.name));
      row.innerHTML = `${kindBadge(k)}<span class="fname"><b>${esc(baseName(m.name))}</b><small>${esc(dirName(m.name))}</small></span><span class="fsize">${esc(sizeText(m.size))}</span>`;
      return row;
    },
    onSelect: (m) => deps.open(entryFromPak(entry, m.name, m.size)),
  });
  swap(() => list.dispose());
  const apply = () => {
    shown = p.entries.filter((m) => matchesQuery({ path: m.name, pak: null }, search.value));
    list.setItems(shown);
    count.textContent = t("exp.pak.count", shown.length, p.entries.length);
  };
  let timer = 0;
  search.addEventListener("input", () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(apply, 120);
  });
  apply();
}
