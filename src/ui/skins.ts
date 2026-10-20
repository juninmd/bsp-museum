import { t } from "../i18n.ts";
import type { CompareSpec, LookSpec, ModelViewer } from "../model-viewer.ts";
import { SkinStudio } from "../skin-studio.ts";
import { changedTextures, distinctFamilies, familyMap, modelKey, TEAM_PRESETS } from "../lib/skins.ts";
import type { MdlSummaryEx } from "../types/mdl-extra.ts";
import { mountUvMap, type UvMap } from "./uvmap.ts";

/**
 * Faixa de skins da aba Recursos: galeria de miniaturas 3D (um único `WebGLRenderer` compartilhado, em
 * `skin-studio.ts`, desenhando em canvases 2D), comparação de duas skins (lado a lado ou com divisor) com a
 * câmera do visualizador, prévia das equipes (cores de time top/bottom) e o mapa de UV das texturas.
 */
export interface SkinsHost {
  viewer: ModelViewer;
  /** onde o painel é inserido (o contêiner do visualizador) */
  anchor: HTMLElement;
  model(): MdlSummaryEx;
  /** família de skin aplicada no visualizador */
  skin(): number;
  /** aplica a família no visualizador (e no seletor da barra) */
  setSkin(index: number): void;
  /** `load_model` com as cores de time dadas (corpo atual preservado) */
  loadVariant(top: number, bottom: number): Promise<MdlSummaryEx>;
}

export interface Skins {
  /** o modelo foi recarregado (bodygroup, cor de time) */
  setModel(model: MdlSummaryEx): void;
  /** a família do visualizador mudou */
  setSkin(index: number): void;
  dispose(): void;
}

/** lado da comparação: família de skin + equipe ("model" = cores atuais do modelo, senão o id de `TEAM_PRESETS`) */
interface Side {
  skin: number;
  team: string;
}

const TILE_FPS = 24;
/** miniaturas redesenhadas por quadro de animação (as demais esperam a vez) */
const TILES_PER_FRAME = 6;
const SPIN = 0.9; // rad/s

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

interface Tile {
  button: HTMLButtonElement;
  canvas: HTMLCanvasElement;
  index: number;
  map: number[];
  visible: boolean;
}

export function mountSkins(host: SkinsHost): Skins {
  const ctl = new AbortController();
  const on = { signal: ctl.signal };
  const dock = el("section", "skins-dock");
  host.anchor.append(dock);

  let model = host.model();
  let active = host.skin();
  const hasRemap = () => (model.texture_info ?? []).some((i) => i.remap);
  const families = () => model.skin_families;

  let studio: SkinStudio | null = null;
  let studioReady = false;
  let tiles: Tile[] = [];
  let raf = 0;
  let lastDraw = 0;
  let cursor = 0;

  // ---- estado da comparação
  let compareOn = false;
  let mode: CompareSpec["mode"] = "side";
  let sideA: Side = { skin: 0, team: "model" };
  let sideB: Side = { skin: 0, team: "model" };
  let variants = new Map<string, Promise<MdlSummaryEx>>();
  let compareToken = 0;
  let status = "";

  let uv: UvMap | null = null;
  let uvOpen = false;

  const unPick = host.viewer.onPick((tri) => {
    if (uv && uvOpen) uv.selectTri(tri);
  });

  // ------------------------------------------------------------------ comparação

  function variant(teamId: string): Promise<MdlSummaryEx> | null {
    const preset = TEAM_PRESETS.find((p) => p.id === teamId);
    if (!preset) return null;
    const key = modelKey(model.selection?.body ?? 0, preset.top, preset.bottom);
    let p = variants.get(key);
    if (!p) variants.set(key, (p = host.loadVariant(preset.top, preset.bottom)));
    return p;
  }

  async function lookFor(side: Side): Promise<{ spec: LookSpec; label: string }> {
    const map = familyMap(families(), side.skin, model.textures.length);
    const pending = variant(side.team);
    let label = t("mdl.skinN", side.skin + 1);
    if (!pending) return { spec: { map }, label };
    const preset = TEAM_PRESETS.find((p) => p.id === side.team)!;
    label += ` · ${t(preset.label)}`;
    return { spec: { map, model: await pending }, label };
  }

  async function applyCompare() {
    const mine = ++compareToken;
    if (!compareOn) {
      void host.viewer.setCompare(null);
      return;
    }
    status = t("common.loading");
    renderStatus();
    try {
      const [a, b] = await Promise.all([lookFor(sideA), lookFor(sideB)]);
      if (mine !== compareToken) return;
      await host.viewer.setCompare({ mode, a: a.spec, b: b.spec, labels: [a.label, b.label] });
      if (mine !== compareToken) return;
      status = "";
    } catch (err) {
      if (mine !== compareToken) return;
      variants = new Map(); // tenta de novo na próxima vez
      status = t("skins.cmp.failed");
      statusEl.title = String(err);
    }
    renderStatus();
  }

  function setCompare(on: boolean) {
    compareOn = on;
    void applyCompare();
    build();
  }

  /** atalho: a skin atual vestindo as cores das duas equipes, lado a lado */
  function teamPreview() {
    const presets = TEAM_PRESETS;
    sideA = { skin: active, team: presets[0]!.id };
    sideB = { skin: active, team: presets[1]!.id };
    mode = "side";
    setCompare(true);
  }

  // ------------------------------------------------------------------ galeria (miniaturas 3D)

  function frame(now: number) {
    raf = requestAnimationFrame(frame);
    if (!studio || !studioReady || document.hidden || tiles.length === 0) return;
    if (now - lastDraw < 1000 / TILE_FPS) return;
    lastDraw = now;
    const angle = (now / 1000) * SPIN;
    let drawn = 0;
    for (let n = 0; n < tiles.length && drawn < TILES_PER_FRAME; n++) {
      const tile = tiles[(cursor + n) % tiles.length]!;
      if (!tile.visible) continue;
      studio.draw(tile.canvas, tile.map, angle);
      drawn++;
    }
    cursor = (cursor + Math.max(drawn, 1)) % Math.max(tiles.length, 1);
  }

  function ensureStudio() {
    if (studio || families().length < 2) return;
    studio = new SkinStudio(128);
    studioReady = false;
    void studio.setModel(model).then(() => (studioReady = !!studio?.ready));
  }

  // ------------------------------------------------------------------ interface

  const statusEl = el("span", "skins-status");
  function renderStatus() {
    statusEl.textContent = status;
    if (!status) statusEl.removeAttribute("title");
  }

  let observer: IntersectionObserver | null = null;

  function toggleButton(label: string, title: string, pressed: boolean, run: () => void): HTMLButtonElement {
    const b = el("button", "skins-toggle", label);
    b.type = "button";
    b.title = title;
    b.setAttribute("aria-pressed", String(pressed));
    b.addEventListener("click", run, on);
    return b;
  }

  function skinSelect(side: Side, change: () => void): HTMLSelectElement {
    const sel = el("select");
    families().forEach((_, i) => sel.append(new Option(t("mdl.skinN", i + 1), String(i))));
    sel.value = String(side.skin);
    sel.addEventListener(
      "change",
      () => {
        side.skin = Number(sel.value);
        change();
      },
      on,
    );
    return sel;
  }

  function teamSelect(side: Side, change: () => void): HTMLSelectElement {
    const sel = el("select");
    sel.append(new Option(t("skins.team.model"), "model"));
    for (const p of TEAM_PRESETS) sel.append(new Option(`${t(p.label)} (${p.top}/${p.bottom})`, p.id));
    sel.value = side.team;
    sel.title = t("skins.team.hint");
    sel.addEventListener(
      "change",
      () => {
        side.team = sel.value;
        change();
      },
      on,
    );
    return sel;
  }

  function buildGallery(): HTMLElement | null {
    observer?.disconnect();
    observer = null;
    tiles = [];
    const fam = families();
    if (fam.length < 2) return null;
    ensureStudio();
    const box = el("div", "skins-gallery");
    box.setAttribute("role", "group");
    box.setAttribute("aria-label", t("skins.gallery"));
    if (typeof IntersectionObserver !== "undefined") {
      observer = new IntersectionObserver(
        (entries) => {
          for (const e of entries) {
            const tile = tiles.find((x) => x.canvas === e.target);
            if (tile) tile.visible = e.isIntersecting;
          }
        },
        { root: box },
      );
    }
    for (const i of distinctFamilies(fam)) {
      const map = familyMap(fam, i, model.textures.length);
      const button = el("button", "skin-tile");
      button.type = "button";
      const activeRow = familyMap(fam, active, model.textures.length).join(",") === map.join(",");
      button.classList.toggle("on", activeRow);
      button.setAttribute("aria-pressed", String(activeRow));
      button.title = t("skins.tile.hint");
      const canvas = el("canvas", "skin-tile-canvas");
      canvas.width = canvas.height = 128;
      canvas.setAttribute("aria-hidden", "true");
      const changed = changedTextures(fam, i).length;
      button.append(
        canvas,
        el("span", "skin-tile-name", t("mdl.skinN", i + 1)),
        el("span", "skin-tile-sub", i === 0 ? t("skins.tile.base") : t("skins.tile.changed", changed)),
      );
      button.addEventListener(
        "click",
        (e) => {
          if (e.shiftKey) {
            // Shift+clique: define o lado B da comparação
            sideB = { ...sideB, skin: i };
            if (!compareOn) {
              sideA = { ...sideA, skin: active };
              compareOn = true;
            }
            void applyCompare();
            build();
            return;
          }
          if (compareOn) {
            sideA = { ...sideA, skin: i };
            void applyCompare();
          }
          host.setSkin(i);
        },
        on,
      );
      const tile: Tile = { button, canvas, index: i, map, visible: typeof IntersectionObserver === "undefined" };
      tiles.push(tile);
      observer?.observe(canvas);
      box.append(button);
    }
    return box;
  }

  function buildComparePanel(): HTMLElement | null {
    if (!compareOn) return null;
    const panel = el("div", "skins-compare");
    const side = (name: string, s: Side) => {
      const wrap = el("div", "skins-side");
      wrap.append(el("strong", undefined, name), skinSelect(s, () => void applyCompare()));
      if (hasRemap()) wrap.append(teamSelect(s, () => void applyCompare()));
      return wrap;
    };
    const modeSel = el("select");
    modeSel.append(new Option(t("skins.cmp.side"), "side"), new Option(t("skins.cmp.split"), "split"));
    modeSel.value = mode;
    modeSel.title = t("skins.cmp.modeHint");
    modeSel.addEventListener(
      "change",
      () => {
        mode = modeSel.value as CompareSpec["mode"];
        void applyCompare();
      },
      on,
    );
    const swap = el("button", "mdl-mini", "⇄");
    swap.type = "button";
    swap.title = t("skins.cmp.swap");
    swap.setAttribute("aria-label", t("skins.cmp.swap"));
    swap.addEventListener(
      "click",
      () => {
        [sideA, sideB] = [sideB, sideA];
        void applyCompare();
        build();
      },
      on,
    );
    panel.append(side("A", sideA), swap, side("B", sideB), modeSel, statusEl);
    return panel;
  }

  function buildUv(): HTMLElement | null {
    if (!uvOpen) return null;
    if (!uv) {
      uv = mountUvMap({
        model: () => model,
        map: () => familyMap(families(), active, model.textures.length),
        highlight: (tris) => host.viewer.setHighlight(tris),
      });
    }
    return uv.el;
  }

  /** (re)monta o painel inteiro a partir do estado; o mapa de UV é reaproveitado entre as montagens */
  function build() {
    if (uv) uv.el.remove();
    const head = el("div", "skins-head");
    head.append(el("h4", undefined, t("skins.title")));
    const tools = el("div", "skins-tools");
    tools.append(
      toggleButton(t("skins.cmp.title"), t("skins.cmp.hint"), compareOn, () => setCompare(!compareOn)),
      ...(hasRemap() ? [toggleButton(t("skins.teams"), t("skins.teamsHint"), false, teamPreview)] : []),
      toggleButton(t("skins.uv.title"), t("skins.uv.titleHint"), uvOpen, () => {
        uvOpen = !uvOpen;
        if (!uvOpen) host.viewer.setHighlight([]);
        build();
      }),
    );
    head.append(tools);
    const gallery = buildGallery();
    const compare = buildComparePanel();
    const uvEl = buildUv();
    dock.replaceChildren(head, ...[gallery, compare, uvEl].filter((x): x is HTMLElement => x !== null));
    renderStatus();
  }

  // Trocar o idioma refaz os textos (o estado fica).
  const langWatch = new MutationObserver(() => {
    build();
    uv?.refresh();
    if (compareOn) void applyCompare();
  });
  langWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["lang"] });

  build();
  raf = requestAnimationFrame(frame);

  return {
    setModel(next) {
      model = next;
      variants = new Map();
      active = Math.min(active, Math.max(0, next.skin_families.length - 1));
      sideA.skin = Math.min(sideA.skin, Math.max(0, next.skin_families.length - 1));
      sideB.skin = Math.min(sideB.skin, Math.max(0, next.skin_families.length - 1));
      if (studio) {
        studioReady = false;
        void studio.setModel(next).then(() => (studioReady = !!studio?.ready));
      }
      uv?.setModel();
      build();
      if (compareOn) void applyCompare();
    },
    setSkin(index) {
      active = index;
      if (!compareOn) sideA = { ...sideA, skin: index };
      uv?.setMap();
      build();
    },
    dispose() {
      ctl.abort();
      langWatch.disconnect();
      cancelAnimationFrame(raf);
      compareToken++;
      observer?.disconnect();
      unPick();
      uv?.dispose();
      uv = null;
      studio?.dispose();
      studio = null;
      dock.remove();
      void host.viewer.setCompare(null);
      host.viewer.setHighlight([]);
    },
  };
}
