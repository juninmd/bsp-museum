import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { buildChunksAsync } from "../lib/chunker.ts";
import type { Chunk } from "../lib/geometry.ts";
import { fastdlText, fmtSize } from "../lib/report.ts";
import { findingText, modeLabel, t } from "../i18n.ts";
import { annotationOf, updateAnnotation } from "../store.ts";
import { normalizeTag } from "../lib/filters.ts";
import type { EntityRow, Finding, MapDetail, MapSummary, MeshDetail, ResourceItem } from "../types.ts";
import { mount3D, type Viewer3D } from "../viewer3d.ts";
import { $, escapeHtml } from "./dom.ts";

type ViewMode = "plan" | "3d" | "tex";

export interface DetailContext {
  slots: number;
  setStatus(text: string, tone?: "info" | "error"): void;
  /** anotação mudou: a galeria precisa redesenhar o card */
  onAnnotationChange(): void;
}

const SEVERITY_ICON: Record<Finding["severity"], string> = { critical: "✖", warn: "▲", info: "•" };

/** malha 3D e chunks por caminho: só são pedidos/montados na primeira vez que abre a aba 3D */
const meshCache = new Map<string, { mesh: MeshDetail; chunks: Chunk[] }>();

let activeViewer: Viewer3D | null = null;
let cleanup: (() => void) | null = null;

export function disposeDetail(): void {
  cleanup?.();
  cleanup = null;
  activeViewer?.dispose();
  activeViewer = null;
}

function kindLabel(kind: ResourceItem["kind"]): string {
  return t(`res.kind.${kind}`);
}

function findingsHtml(findings: Finding[]): string {
  if (!findings.length) return `<li class="finding ok">✔ ${escapeHtml(t("detail.noIssues"))}</li>`;
  return findings
    .map((f) => {
      const text = findingText(f);
      return `
      <li class="finding ${f.severity}">
        <div class="finding-head">${SEVERITY_ICON[f.severity]} ${escapeHtml(text.title)} <code>${escapeHtml(f.id)}</code></div>
        <p>${escapeHtml(text.detail)}</p>
        <p class="hint">→ ${escapeHtml(text.hint)}</p>
      </li>`;
    })
    .join("");
}

function resourcesHtml(d: MapDetail): string {
  const r = d.resources;
  const rows = r.items
    .map((i) => {
      const state = !i.found ? "missing" : i.shared ? "shared" : "download";
      const label = !i.found ? t("res.state.missing") : i.shared ? t("res.state.shared") : t("res.state.download");
      return `<tr class="res-${state}"><td>${escapeHtml(kindLabel(i.kind))}</td><td><code>${escapeHtml(i.path)}</code></td><td>${i.found ? fmtSize(i.size) : "—"}</td><td><span class="pill ${state}">${escapeHtml(label)}</span></td></tr>`;
    })
    .join("");
  return `
    <section>
      <h3>${escapeHtml(t("res.title"))}</h3>
      <p class="dim small">${escapeHtml(t("res.summary", r.download_count, fmtSize(r.download_size), r.missing))}</p>
      <div class="table-wrap"><table class="table res-table"><tbody>${rows}</tbody></table></div>
      <button id="export-fastdl" class="ghost2">${escapeHtml(t("res.export"))}</button>
    </section>`;
}

export function detailHtml(d: MapDetail, map: MapSummary): string {
  const s = d.summary;
  const note = annotationOf(map);
  const size = s.bounds
    ? `${Math.round(s.bounds.size[0])} × ${Math.round(s.bounds.size[1])} × ${Math.round(s.bounds.size[2])} ${t("detail.units")}`
    : "—";

  const topEntities = d.histogram
    .slice(0, 14)
    .map(([name, count]) => `<tr><td><code>${escapeHtml(name)}</code></td><td>${count}</td></tr>`)
    .join("");

  const lumps = d.lumps
    .filter((l) => l.length > 0)
    .slice(0, 8)
    .map(
      (l) => `<tr>
        <td>${escapeHtml(l.name)}</td>
        <td>${fmtSize(l.length)}</td>
        <td><div class="bar" style="width:${Math.max(2, l.percent).toFixed(1)}%"></div>${l.percent.toFixed(1)}%</td>
      </tr>`,
    )
    .join("");

  return `
    <header class="detail-head">
      <h2>${escapeHtml(s.name)}</h2>
      <span class="chip mode-${s.mode}">${escapeHtml(modeLabel(s.mode))}</span>
      ${s.bsp_version === 29 ? `<span class="chip">Quake BSP v29</span>` : ""}
      <button id="fav" class="ghost2 fav" aria-pressed="${note.favorite}" title="${escapeHtml(t("note.favorite"))}">${note.favorite ? "★" : "☆"}</button>
      <button id="export" class="ghost">${escapeHtml(t("detail.exportSvg"))}</button>
    </header>
    ${s.title ? `<p class="title">“${escapeHtml(s.title)}”</p>` : ""}

    <div class="plan-toolbar">
      <div class="seg" role="tablist">
        <button data-view="plan" class="active">${escapeHtml(t("view.plan"))}</button>
        <button data-view="3d">${escapeHtml(t("view.3d"))}</button>
        <button data-view="tex">${escapeHtml(t("view.tex"))}</button>
      </div>
      <span class="dim">${escapeHtml(t("detail.polygons", d.polygons))}</span>
    </div>
    <div class="viewer-controls" id="viewer-controls" hidden>
      <label class="chk" id="lm-wrap"><input type="checkbox" id="lm-toggle" checked /> ${escapeHtml(t("viewer.lightmaps"))}</label>
      <label class="chk" id="pvs-wrap"><input type="checkbox" id="pvs-toggle" checked /> ${escapeHtml(t("viewer.pvs"))}</label>
      <label class="chk"><input type="checkbox" id="tex-alpha" /> ${escapeHtml(t("viewer.alpha"))}</label>
      <button id="fps" class="ghost2">${escapeHtml(t("viewer.fps"))}</button>
      <button id="fullscreen" class="ghost2">${escapeHtml(t("viewer.fullscreen"))}</button>
      <code id="camera-label" class="hintc">${escapeHtml(t("viewer.orbitHint"))}</code>
    </div>
    <div class="plan" id="plan-holder">${d.svg}</div>
    <div class="viewer3d" id="viewer3d" hidden><div class="hud" id="hud" aria-live="off"></div></div>
    <p class="legend">
      <span class="key ct"></span> ${escapeHtml(t("legend.ct"))}
      <span class="key t"></span> ${escapeHtml(t("legend.t"))}
      <span class="key grad"></span> ${escapeHtml(t("legend.height"))}
      <span class="dim">${escapeHtml(t("viewer.orbitHint"))}</span>
    </p>

    <section>
      <h3>${escapeHtml(t("detail.diagnosis"))}</h3>
      <ul class="findings">${findingsHtml(d.findings)}</ul>
    </section>

    <section class="notes">
      <h3>${escapeHtml(t("note.title"))}</h3>
      <div class="tags" id="tags"></div>
      <div class="tag-add">
        <input id="tag-input" type="text" maxlength="32" placeholder="${escapeHtml(t("note.tagPlaceholder"))}" aria-label="${escapeHtml(t("note.tagPlaceholder"))}" />
        <button id="tag-add" class="ghost2">${escapeHtml(t("note.addTag"))}</button>
      </div>
      <label class="sr-only" for="note-text">${escapeHtml(t("note.note"))}</label>
      <textarea id="note-text" rows="3" placeholder="${escapeHtml(t("note.notePlaceholder"))}">${escapeHtml(note.note)}</textarea>
    </section>

    <section class="grid-2">
      <div>
        <h3>${escapeHtml(t("detail.geometry"))}</h3>
        <dl class="facts col">
          <div><dt>${escapeHtml(t("detail.dimensions"))}</dt><dd>${size}</dd></div>
          <div><dt>${escapeHtml(t("detail.faces"))}</dt><dd>${d.face_count.toLocaleString(t("locale"))}</dd></div>
          <div><dt>${escapeHtml(t("detail.vertices"))}</dt><dd>${d.vertex_count.toLocaleString(t("locale"))}</dd></div>
          <div><dt>${escapeHtml(t("detail.models"))}</dt><dd>${d.model_count}</dd></div>
          <div><dt>${escapeHtml(t("detail.file"))}</dt><dd>${fmtSize(s.file_size)}</dd></div>
        </dl>
      </div>
      <div>
        <h3>${escapeHtml(t("detail.textures"))}</h3>
        <dl class="facts col">
          <div><dt>${escapeHtml(t("detail.texTotal"))}</dt><dd>${d.texture_count}</dd></div>
          <div><dt>${escapeHtml(t("detail.texEmbedded"))}</dt><dd>${d.embedded_textures}</dd></div>
          <div><dt>${escapeHtml(t("detail.sky"))}</dt><dd>${d.sky ? escapeHtml(d.sky) : "—"}</dd></div>
          <div><dt>${escapeHtml(t("detail.wads"))}</dt><dd>${d.wads.length ? escapeHtml(d.wads.join(", ")) : escapeHtml(t("detail.none"))}</dd></div>
        </dl>
      </div>
    </section>

    <section class="grid-2">
      <div>
        <h3>${escapeHtml(t("detail.topEntities"))}</h3>
        <table class="table"><tbody>${topEntities}</tbody></table>
      </div>
      <div>
        <h3>${escapeHtml(t("detail.lumps"))}</h3>
        <table class="table lumps"><tbody>${lumps}</tbody></table>
      </div>
    </section>

    ${resourcesHtml(d)}

    <section>
      <h3>${escapeHtml(t("ent.title"))}</h3>
      <details id="ent-panel" class="panel">
        <summary>${escapeHtml(t("ent.open", d.summary.entity_count))}</summary>
        <div class="ent-tools">
          <input id="ent-search" type="search" placeholder="${escapeHtml(t("ent.search"))}" aria-label="${escapeHtml(t("ent.search"))}" />
          <span class="dim small" id="ent-count"></span>
        </div>
        <div id="ent-list" class="ent-list"></div>
      </details>
    </section>

    <section>
      <h3>${escapeHtml(t("radar.title"))}</h3>
      <p class="dim small">${escapeHtml(t("radar.desc"))}</p>
      <div class="radar-actions">
        <button id="radar-preview" class="ghost2">${escapeHtml(t("radar.preview"))}</button>
        <button id="radar-export" class="ghost2" disabled>${escapeHtml(t("radar.export"))}</button>
      </div>
      <div id="radar-holder" class="radar-holder" hidden></div>
    </section>

    <details class="texlist">
      <summary>${escapeHtml(t("detail.texUsed", d.textures.length))}</summary>
      <p>${d.textures.map((x) => `<code>${escapeHtml(x)}</code>`).join(" ")}</p>
    </details>`;
}

/** Liga todos os controles do detalhe (abas 2D/3D, painéis, anotações, exportações). */
export function wireDetail(root: HTMLElement, d: MapDetail, map: MapSummary, ctx: DetailContext): void {
  const planHolder = $<HTMLElement>(root, "#plan-holder");
  const viewerHolder = $<HTMLElement>(root, "#viewer3d");
  const controlsBar = $<HTMLElement>(root, "#viewer-controls");
  const alphaBox = $<HTMLInputElement>(root, "#tex-alpha");
  const lmBox = $<HTMLInputElement>(root, "#lm-toggle");
  const pvsBox = $<HTMLInputElement>(root, "#pvs-toggle");
  const fpsBtn = $<HTMLButtonElement>(root, "#fps");
  const fsBtn = $<HTMLButtonElement>(root, "#fullscreen");
  const cameraLabel = $<HTMLElement>(root, "#camera-label");
  const buttons = Array.from(root.querySelectorAll<HTMLButtonElement>("[data-view]"));
  let mode: ViewMode = "plan";
  let hud: HTMLElement | null = null;

  // ----- exportar planta
  $<HTMLButtonElement>(root, "#export").addEventListener("click", async () => {
    const target = await save({ defaultPath: `${d.summary.name}.svg`, filters: [{ name: "SVG", extensions: ["svg"] }] });
    if (!target) return;
    await invoke("export_text", { target, content: d.svg });
    ctx.setStatus(t("status.exported", target));
  });

  // ----- viewer
  const setCameraHint = (key: string) => (cameraLabel.textContent = t(key));
  alphaBox.addEventListener("change", () => activeViewer?.setTransparent(alphaBox.checked));
  lmBox.addEventListener("change", () => activeViewer?.setLightmaps(lmBox.checked));
  pvsBox.addEventListener("change", () => activeViewer?.setPvs(pvsBox.checked));
  const resetFpsBtn = () => {
    fpsBtn.dataset.on = "0";
    fpsBtn.textContent = t("viewer.fps");
    setCameraHint("viewer.orbitHint");
  };
  fpsBtn.addEventListener("click", () => {
    if (!activeViewer) return;
    if (fpsBtn.dataset.on === "1") {
      activeViewer.exitFirstPerson();
      resetFpsBtn();
    } else {
      activeViewer.enterFirstPerson();
      fpsBtn.dataset.on = "1";
      fpsBtn.textContent = t("viewer.fpsExit");
      setCameraHint("viewer.fpsHint");
    }
  });
  const onPointerLock = () => {
    if (!document.pointerLockElement) resetFpsBtn();
  };
  document.addEventListener("pointerlockchange", onPointerLock);
  fsBtn.addEventListener("click", () => activeViewer?.setFullscreen(!document.fullscreenElement));
  const onFsChange = () => {
    fsBtn.textContent = document.fullscreenElement ? t("viewer.fullscreenExit") : t("viewer.fullscreen");
  };
  document.addEventListener("fullscreenchange", onFsChange);

  const setMode = async (next: ViewMode): Promise<void> => {
    activeViewer?.dispose();
    activeViewer = null;
    mode = next;
    planHolder.hidden = next !== "plan";
    viewerHolder.hidden = next === "plan";
    for (const b of buttons) b.classList.toggle("active", b.dataset.view === next);
    controlsBar.hidden = next === "plan";
    $<HTMLElement>(root, "#lm-wrap").hidden = next !== "tex";
    if (next === "plan") {
      alphaBox.checked = false;
      resetFpsBtn();
      return;
    }
    setCameraHint("viewer.orbitHint");

    let cached = meshCache.get(d.summary.path);
    if (!cached) {
      viewerHolder.innerHTML = `<div class="loading">${escapeHtml(t("viewer.loading"))}</div>`;
      try {
        const mesh = await invoke<MeshDetail>("map_mesh", { path: d.summary.path });
        cached = { mesh, chunks: await buildChunksAsync(mesh) };
        meshCache.set(d.summary.path, cached);
      } catch (err) {
        viewerHolder.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
        return;
      }
    }
    if (mode !== next) return; // trocou de aba enquanto carregava
    viewerHolder.replaceChildren();
    hud = document.createElement("div");
    hud.className = "hud";
    viewerHolder.append(hud);
    const { mesh, chunks } = cached;
    $<HTMLElement>(root, "#lm-wrap").hidden = next !== "tex" || !mesh.lightmap;
    $<HTMLElement>(root, "#pvs-wrap").hidden = !mesh.pvs;
    activeViewer = mount3D(viewerHolder, mesh, { textured: next === "tex", chunks });
    activeViewer.setLightmaps(lmBox.checked);
    activeViewer.setPvs(pvsBox.checked);
    activeViewer.onStats((s) => {
      if (!hud) return;
      const leaf = s.leaf >= 0 ? ` · ${t("viewer.leaf", s.leaf)}` : "";
      hud.textContent = `${t("viewer.tris", s.drawn.toLocaleString(t("locale")), s.total.toLocaleString(t("locale")))}${leaf}${s.pvsActive ? " · PVS" : ""}`;
    });
  };
  for (const b of buttons) b.addEventListener("click", () => void setMode(b.dataset.view as ViewMode));

  // ----- favorito, tags e nota
  const favBtn = $<HTMLButtonElement>(root, "#fav");
  const tagsBox = $<HTMLElement>(root, "#tags");
  const tagInput = $<HTMLInputElement>(root, "#tag-input");
  const noteBox = $<HTMLTextAreaElement>(root, "#note-text");
  const renderTags = () => {
    tagsBox.replaceChildren();
    for (const tag of annotationOf(map).tags) {
      const chip = document.createElement("span");
      chip.className = "tag";
      chip.textContent = tag;
      const rm = document.createElement("button");
      rm.type = "button";
      rm.className = "tag-x";
      rm.textContent = "×";
      rm.setAttribute("aria-label", t("note.removeTag", tag));
      rm.addEventListener("click", async () => {
        await updateAnnotation(map, { tags: annotationOf(map).tags.filter((x) => x !== tag) });
        renderTags();
        ctx.onAnnotationChange();
      });
      chip.append(rm);
      tagsBox.append(chip);
    }
  };
  const addTag = async () => {
    const tag = normalizeTag(tagInput.value);
    tagInput.value = "";
    if (!tag || annotationOf(map).tags.includes(tag)) return;
    await updateAnnotation(map, { tags: [...annotationOf(map).tags, tag] });
    renderTags();
    ctx.onAnnotationChange();
  };
  $<HTMLButtonElement>(root, "#tag-add").addEventListener("click", () => void addTag());
  tagInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void addTag();
    }
  });
  favBtn.addEventListener("click", async () => {
    const next = !annotationOf(map).favorite;
    await updateAnnotation(map, { favorite: next });
    favBtn.textContent = next ? "★" : "☆";
    favBtn.setAttribute("aria-pressed", String(next));
    ctx.onAnnotationChange();
  });
  noteBox.addEventListener("change", async () => {
    await updateAnnotation(map, { note: noteBox.value });
    ctx.onAnnotationChange();
  });
  renderTags();

  // ----- recursos / FastDL
  $<HTMLButtonElement>(root, "#export-fastdl").addEventListener("click", async () => {
    const target = await save({ defaultPath: `${d.summary.name}-fastdl.txt`, filters: [{ name: "TXT", extensions: ["txt"] }] });
    if (!target) return;
    await invoke("export_text", { target, content: fastdlText(d.resources) });
    ctx.setStatus(t("status.exported", target));
  });

  // ----- entidades (carrega só ao abrir o painel)
  const entPanel = $<HTMLDetailsElement>(root, "#ent-panel");
  const entList = $<HTMLElement>(root, "#ent-list");
  const entSearch = $<HTMLInputElement>(root, "#ent-search");
  const entCount = $<HTMLElement>(root, "#ent-count");
  let entities: EntityRow[] | null = null;

  const focusEntity = async (row: EntityRow) => {
    if (!row.origin) return;
    if (!activeViewer) await setMode("tex");
    activeViewer?.focusOn(row.origin);
    viewerHolder.scrollIntoView({ block: "center", behavior: "smooth" });
  };
  const renderEntities = () => {
    if (!entities) return;
    const q = entSearch.value.trim().toLowerCase();
    const rows = entities.filter(
      (r) =>
        !q ||
        r.classname.toLowerCase().includes(q) ||
        (r.targetname ?? "").toLowerCase().includes(q) ||
        r.keys.some(([k, v]) => k.toLowerCase().includes(q) || v.toLowerCase().includes(q)),
    );
    entCount.textContent = t("ent.count", rows.length, entities.length);
    entList.replaceChildren();
    for (const row of rows.slice(0, 300)) {
      const item = document.createElement("div");
      item.className = "ent-row";
      const where = row.origin ? row.origin.map((n) => Math.round(n)).join(" ") : "—";
      item.innerHTML = `
        <div class="ent-main">
          <code class="ent-class">${escapeHtml(row.classname)}</code>
          ${row.targetname ? `<span class="ent-name">${escapeHtml(row.targetname)}</span>` : ""}
          <span class="dim small">${escapeHtml(where)}</span>
          <button class="ghost2 ent-go" ${row.origin ? "" : "disabled"}>${escapeHtml(t("ent.go"))}</button>
        </div>
        <details class="ent-keys"><summary>${escapeHtml(t("ent.keys", row.keys.length))}</summary>
          <table class="table"><tbody>${row.keys.map(([k, v]) => `<tr><td><code>${escapeHtml(k)}</code></td><td>${escapeHtml(v)}</td></tr>`).join("")}</tbody></table>
        </details>`;
      item.querySelector(".ent-go")!.addEventListener("click", () => void focusEntity(row));
      entList.append(item);
    }
    if (rows.length > 300) {
      const more = document.createElement("p");
      more.className = "dim small";
      more.textContent = t("ent.more", rows.length - 300);
      entList.append(more);
    }
  };
  entPanel.addEventListener("toggle", async () => {
    if (!entPanel.open || entities) return;
    entList.innerHTML = `<div class="loading">${escapeHtml(t("common.loading"))}</div>`;
    try {
      entities = await invoke<EntityRow[]>("map_entities", { path: d.summary.path });
      renderEntities();
    } catch (err) {
      entList.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
    }
  });
  let entDebounce = 0;
  entSearch.addEventListener("input", () => {
    window.clearTimeout(entDebounce);
    entDebounce = window.setTimeout(renderEntities, 120);
  });

  // ----- radar
  const radarPreview = $<HTMLButtonElement>(root, "#radar-preview");
  const radarExport = $<HTMLButtonElement>(root, "#radar-export");
  const radarHolder = $<HTMLElement>(root, "#radar-holder");
  radarPreview.addEventListener("click", async () => {
    radarHolder.hidden = false;
    radarHolder.innerHTML = `<div class="loading">${escapeHtml(t("common.loading"))}</div>`;
    try {
      const png = await invoke<string>("map_radar", { path: d.summary.path });
      radarHolder.innerHTML = `<img alt="${escapeHtml(t("radar.alt", d.summary.name))}" src="${png}" />`;
      radarExport.disabled = false;
    } catch (err) {
      radarHolder.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
    }
  });
  radarExport.addEventListener("click", async () => {
    const dir = await open({ directory: true, multiple: false, title: t("radar.pickDir") });
    if (typeof dir !== "string") return;
    try {
      const written = await invoke<string[]>("export_radar", { path: d.summary.path, outDir: dir });
      ctx.setStatus(t("radar.done", written.length, dir));
    } catch (err) {
      ctx.setStatus(String(err), "error");
    }
  });

  cleanup = () => {
    document.removeEventListener("pointerlockchange", onPointerLock);
    document.removeEventListener("fullscreenchange", onFsChange);
    window.clearTimeout(entDebounce);
  };
}
