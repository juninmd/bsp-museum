import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n.ts";
import { HullPager } from "../lib/hulls.ts";
import type { HullMesh, VolumeReport } from "../types/bsp-extra.ts";
import type { SpriteInfo } from "../types/formats.ts";
import type { MapDetail } from "../types.ts";
import type { LayerName, Viewer3D } from "../viewer3d.ts";
import { zoneLabel } from "../viewer-overlays.ts";
import { createSoundPanel, type SoundPanel } from "./sound-panel.ts";

/**
 * Painel "Camadas" da vista 3D: liga e desliga as animações de superfície, os hulls de
 * colisão, os volumes/zonas e os marcadores da cena. Só mostra o que o mapa tem; o que
 * precisa de dados extras (hulls, volumes, sprites) é pedido ao backend na primeira vez.
 */

export interface LayersPanel {
  /** para as cargas em andamento; o DOM sai junto com o container do viewer */
  dispose(): void;
}

interface Options {
  holder: HTMLElement;
  viewer: Viewer3D;
  detail: MapDetail;
  /** modo texturizado: as animações de superfície só existem nele */
  textured: boolean;
}

/** `.../cstrike/maps/x.bsp` -> `.../cstrike` (aceita `\` e `/`). */
export function modDirOf(mapPath: string): string {
  const parts = mapPath.split(/[\\/]/);
  parts.pop(); // arquivo
  parts.pop(); // maps
  const sep = mapPath.includes("\\") && !mapPath.includes("/") ? "\\" : "/";
  return parts.join(sep);
}

export function joinPath(dir: string, rel: string): string {
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return `${dir}${sep}${rel.replace(/[\\/]/g, sep)}`;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** camadas de marcadores: mostram a quantidade mesmo quando é 1 */
const COUNTED = new Set<LayerName>(["lights", "sprites", "sounds", "sound3d", "decals", "routes"]);

export function createLayersPanel(opts: Options): LayersPanel {
  const { holder, viewer, detail, textured } = opts;
  let dead = false;
  const path = detail.summary.path;

  const root = el("div", "layers");
  const toggle = el("button", "layers-btn", t("vw.layers"));
  toggle.type = "button";
  toggle.setAttribute("aria-expanded", "false");
  const panel = el("div", "layers-panel");
  panel.hidden = true;
  panel.setAttribute("role", "group");
  panel.setAttribute("aria-label", t("vw.layers"));
  toggle.addEventListener("click", () => {
    panel.hidden = !panel.hidden;
    toggle.setAttribute("aria-expanded", String(!panel.hidden));
  });
  root.append(toggle, panel);
  holder.append(root);

  const stats = viewer.layers();

  /** linha com caixa de seleção ligada a uma camada do viewer */
  const layerRow = (name: LayerName, label: string, hint?: string): HTMLLabelElement | null => {
    if (stats[name].count <= 0) return null;
    const row = el("label", "chk");
    const box = el("input");
    box.type = "checkbox";
    box.checked = stats[name].on;
    box.addEventListener("change", () => viewer.setLayer(name, box.checked));
    row.append(box, document.createTextNode(` ${label} `));
    if (COUNTED.has(name) || stats[name].count > 1) row.append(el("span", "dim", `(${stats[name].count})`));
    if (hint) row.title = hint;
    return row;
  };

  /** seção recolhível: o painel fica curto e cada grupo abre quando o usuário precisa dele */
  const collapsible = (title: string, kids: HTMLElement[], open = false): HTMLDetailsElement => {
    const sec = el("details", "layers-sec");
    sec.open = open;
    sec.append(el("summary", undefined, title), ...kids);
    return sec;
  };

  const section = (title: string, rows: (HTMLElement | null)[], open = false): HTMLElement | null => {
    const kids = rows.filter((r): r is HTMLElement => r !== null);
    if (!kids.length) return null;
    return collapsible(title, kids, open);
  };

  // ---------------------------------------------------------------- superfície
  const surface = textured
    ? section(t("vw.sec.surface"), [
        layerRow("renderModes", t("vw.l.renderModes")),
        layerRow("scroll", t("vw.l.scroll")),
        layerRow("texAnim", t("vw.l.texAnim")),
        layerRow("texAlt", t("vw.l.texAlt"), t("vw.l.texAltHint")),
        layerRow("lightAnim", t("vw.l.lightAnim")),
      ], true)
    : null;

  // ---------------------------------------------------------------- hulls
  const hullBtns = el("div", "hull-btns");
  const hullStatus = el("div", "dim small hull-status");
  hullStatus.setAttribute("aria-live", "polite");
  const hullInfos = detail.extra?.hulls ?? [];
  const hullState = new Map<number, { loaded: boolean; on: boolean; pager: HullPager | null }>();
  const hullProgressBar = el("progress", "hull-progress");
  hullProgressBar.hidden = true;
  hullProgressBar.max = 1;

  const setHull = async (hull: number, on: boolean, btn: HTMLButtonElement) => {
    const st = hullState.get(hull) ?? { loaded: false, on: false, pager: null };
    hullState.set(hull, st);
    st.on = on;
    btn.setAttribute("aria-pressed", String(on));
    if (!on) {
      st.pager?.cancel();
      viewer.setHullVisible(hull, false);
      return;
    }
    if (st.loaded) {
      viewer.setHullVisible(hull, true);
      return;
    }
    if (st.pager) return; // já carregando
    const pager = new HullPager();
    st.pager = pager;
    hullProgressBar.hidden = false;
    try {
      for (let req = pager.next(); req && !dead && st.on; req = pager.next()) {
        const win = await invoke<HullMesh>("map_hulls", { path, hull, model: 0, offset: req.offset, limit: req.limit });
        if (dead || !st.on) break;
        viewer.addHullWindow(hull, win);
        pager.accept(win);
        const p = pager.progress();
        hullProgressBar.value = p.fraction;
        hullStatus.textContent = t("vw.hull.progress", hull, p.loaded.toLocaleString(t("locale")), p.total.toLocaleString(t("locale")), p.windows, Math.max(p.planned, p.windows));
      }
      if (dead) return;
      const p = pager.progress();
      if (st.on && p.done) {
        st.loaded = true;
        hullStatus.textContent =
          (p.capped ? t("vw.hull.capped", hull, p.loaded.toLocaleString(t("locale"))) : t("vw.hull.done", hull, p.loaded.toLocaleString(t("locale")))) +
          (p.truncated ? ` · ${t("vw.hull.truncated")}` : "");
      } else if (!st.on) {
        // desligado no meio da carga: descarta o que chegou, a próxima vez recomeça limpo
        viewer.clearHull(hull);
        hullStatus.textContent = "";
      }
    } catch (err) {
      if (!dead) {
        viewer.clearHull(hull);
        hullStatus.textContent = t("vw.hull.error", hull, String(err));
        btn.setAttribute("aria-pressed", "false");
        st.on = false;
      }
    } finally {
      st.pager = null;
      if (!dead) hullProgressBar.hidden = true;
    }
  };

  for (const h of [1, 2, 3] as const) {
    const info = hullInfos.find((x) => x.hull === h);
    const btn = el("button", "ghost2 hull-btn", t("vw.hull.btn", h));
    btn.type = "button";
    btn.setAttribute("aria-pressed", "false");
    if (info) {
      const w = Math.round(info.maxs[0] - info.mins[0]);
      const d = Math.round(info.maxs[1] - info.mins[1]);
      const z = Math.round(info.maxs[2] - info.mins[2]);
      btn.title = t("vw.hull.size", t(`vw.hull.n${h}`), `${w}×${d}×${z}`);
      if (!info.available) {
        btn.disabled = true;
        btn.title = t("vw.hull.unavailable");
      }
    }
    btn.addEventListener("click", () => void setHull(h, btn.getAttribute("aria-pressed") !== "true", btn));
    hullBtns.append(btn);
  }
  const hullSec = collapsible(t("vw.sec.collision"), [hullBtns, hullProgressBar, hullStatus]);

  // ---------------------------------------------------------------- volumes
  const volMaster = el("label", "chk");
  const volBox = el("input");
  volBox.type = "checkbox";
  volMaster.append(volBox, document.createTextNode(` ${t("vw.l.volumes")}`));
  const volLegend = el("div", "vol-legend");
  const volStatus = el("div", "dim small");
  const volSec = collapsible(t("vw.sec.volumes"), [volMaster, volLegend, volStatus]);
  let volumesLoaded = false;
  let volumesLoading = false;

  const renderLegend = () => {
    volLegend.replaceChildren();
    const cats = viewer.volumeCategories();
    if (!cats.length) {
      volStatus.textContent = t("vw.vol.empty");
      return;
    }
    for (const c of cats) {
      const chip = el("button", "vol-chip");
      chip.type = "button";
      chip.setAttribute("aria-pressed", String(c.on));
      chip.title = t("vw.vol.legendHint");
      const sw = el("span", "vol-sw");
      sw.style.background = c.color;
      chip.append(sw, document.createTextNode(`${c.key.startsWith("zone:") ? zoneLabel(c.key.slice(5), c.label) : c.label} `), el("span", "dim", String(c.count)));
      chip.addEventListener("click", () => {
        const on = chip.getAttribute("aria-pressed") !== "true";
        chip.setAttribute("aria-pressed", String(on));
        viewer.setVolumeCategory(c.key, on);
      });
      volLegend.append(chip);
    }
    volStatus.textContent = viewer.volumesTruncated() ? t("vw.vol.truncated") : "";
  };

  volBox.addEventListener("change", async () => {
    viewer.setLayer("volumes", volBox.checked);
    if (!volBox.checked || volumesLoaded || volumesLoading) return;
    volumesLoading = true;
    volStatus.textContent = t("common.loading");
    try {
      const report = await invoke<VolumeReport>("map_volumes", { path });
      if (dead) return;
      viewer.setVolumes(report);
      viewer.setLayer("volumes", volBox.checked);
      volumesLoaded = true;
      renderLegend();
    } catch (err) {
      if (!dead) volStatus.textContent = t("vw.vol.error", String(err));
    } finally {
      volumesLoading = false;
    }
  });

  // ---------------------------------------------------------------- cena
  const spriteStatus = el("div", "dim small");
  let spritesLoaded = false;
  const loadSprites = async () => {
    if (spritesLoaded) return;
    spritesLoaded = true;
    const models = viewer.spriteModels().slice(0, 48);
    const modDir = modDirOf(path);
    let done = 0;
    let missing = 0;
    for (const model of models) {
      if (dead) return;
      spriteStatus.textContent = t("vw.sprite.loading", done, models.length);
      try {
        const info = await invoke<SpriteInfo>("load_sprite", { path: joinPath(modDir, model) });
        if (dead) return;
        viewer.setSpriteInfo(model, info);
      } catch {
        missing++;
        viewer.setSpriteInfo(model, null);
      }
      done++;
    }
    spriteStatus.textContent = missing ? t("vw.sprite.missing", missing) : "";
  };

  const spriteRow = layerRow("sprites", t("vw.l.sprites"));
  spriteRow?.querySelector("input")?.addEventListener("change", (ev) => {
    if ((ev.target as HTMLInputElement).checked) void loadSprites();
  });
  // "Sons 3D": o painel (e o motor de áudio) só nascem na primeira vez que a camada liga
  let soundPanel: SoundPanel | null = null;
  const soundRow = layerRow("sound3d", t("vw.l.sound3d"), t("vw.l.sound3dHint"));
  soundRow?.querySelector("input")?.addEventListener("change", (ev) => {
    const on = (ev.target as HTMLInputElement).checked;
    if (on && !soundPanel && !dead) soundPanel = createSoundPanel({ holder, viewer, modDir: modDirOf(path) });
    soundPanel?.setOpen(on);
  });
  const scene = section(t("vw.sec.scene"), [
    layerRow("lights", t("vw.l.lights"), t("vw.l.lightsHint")),
    spriteRow,
    layerRow("sounds", t("vw.l.sounds"), t("vw.l.soundsHint")),
    soundRow,
    layerRow("decals", t("vw.l.decals")),
    layerRow("routes", t("vw.l.routes")),
    layerRow("movers", t("vw.l.movers")),
    layerRow("fog", t("vw.l.fog")),
    layerRow("sun", t("vw.l.sun")),
    layerRow("sky", t("vw.l.sky")),
  ]);
  if (scene) scene.append(spriteStatus);

  // sem dados de colisão/volumes no resumo do mapa, esses grupos nem aparecem
  const extra = detail.extra;
  const hasHulls = !extra?.hulls || extra.hulls.some((h) => h.available);
  const hasVolumes = !extra?.zones || Object.keys(extra.zones).length > 0 || extra.invisible_faces > 0;
  if (surface) panel.append(surface);
  if (hasHulls) panel.append(hullSec);
  if (hasVolumes) panel.append(volSec);
  if (scene) panel.append(scene);
  panel.append(el("p", "dim small layers-note", t("vw.note")));

  return {
    dispose() {
      dead = true;
      for (const st of hullState.values()) st.pager?.cancel();
      soundPanel?.dispose();
      soundPanel = null;
    },
  };
}
