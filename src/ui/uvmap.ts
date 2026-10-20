import { t } from "../i18n.ts";
import type { Model } from "../mdl-rig.ts";
import {
  alphaToGray,
  fitView,
  flagKeys,
  hasTransparency,
  hitTestUv,
  panBy,
  pixelToUv,
  textureFileName,
  trianglesByTexture,
  triTexture,
  uvToPixel,
  uvTolerance,
  zoomAt,
  type UvView,
} from "../lib/skins.ts";

/**
 * Mapa de UV: a textura escolhida com a malha UV por cima (wireframe), zoom/pan, fundo xadrez e canal alpha,
 * lista de texturas com tamanho e flags e exportação em PNG. Passar o mouse sobre a textura realça o triângulo
 * sob o cursor (aqui e no modelo 3D); clicar num triângulo do modelo seleciona a textura dele e o marca aqui.
 */
export interface UvMapHost {
  model(): Model;
  /** família de skin aplicada: `map[textura base] = textura` */
  map(): readonly number[];
  /** realça triângulos no modelo 3D (lista vazia apaga) */
  highlight(tris: readonly number[]): void;
}

export interface UvMap {
  el: HTMLElement;
  /** o modelo foi trocado (bodygroup, cor de time): recarrega a lista e as imagens */
  setModel(): void;
  /** a família de skin mudou */
  setMap(): void;
  /** triângulo clicado no modelo 3D: seleciona a textura dele e o marca */
  selectTri(tri: number): void;
  /** refaz os textos (troca de idioma) */
  refresh(): void;
  dispose(): void;
}

interface Loaded {
  img: HTMLImageElement;
  w: number;
  h: number;
  /** canal alpha em cinza (feito sob demanda) */
  alpha: HTMLCanvasElement | null;
  transparent: boolean;
}

const CHECKER = 8;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function mountUvMap(host: UvMapHost): UvMap {
  const ctl = new AbortController();
  const on = { signal: ctl.signal };
  const root = el("div", "uv");
  const list = el("ul", "uv-list");
  const stage = el("div", "uv-stage");
  const tools = el("div", "uv-tools");
  const wrap = el("div", "uv-canvas-wrap");
  const canvas = el("canvas", "uv-canvas");
  const info = el("div", "uv-info");
  wrap.append(canvas);
  stage.append(tools, wrap, info);
  root.append(list, stage);

  let selTex = -1;
  let loaded: Loaded | null = null;
  let loadToken = 0;
  let view: UvView = { ox: 0, oy: 0, scale: 1 };
  let touched = false; // o usuário mexeu no zoom/pan: não reenquadrar sozinho
  let showWire = true;
  let showChecker = true;
  let showAlpha = false;
  let hover = -1;
  let selected = -1;
  let tris: number[] = [];
  let cache = new Map<number, Promise<Loaded | null>>();
  let items: HTMLButtonElement[] = [];
  let width = 1;
  let height = 1;
  let dpr = 1;

  const checkerPattern = (() => {
    const c = el("canvas");
    c.width = c.height = CHECKER * 2;
    const g = c.getContext("2d");
    if (g) {
      g.fillStyle = "#3a424d";
      g.fillRect(0, 0, c.width, c.height);
      g.fillStyle = "#2a3039";
      g.fillRect(0, 0, CHECKER, CHECKER);
      g.fillRect(CHECKER, CHECKER, CHECKER, CHECKER);
    }
    return c;
  })();

  // ------------------------------------------------------------------ imagens

  function loadTexture(id: number): Promise<Loaded | null> {
    let p = cache.get(id);
    if (!p) {
      p = (async () => {
        const png = host.model().textures[id]?.png;
        if (!png) return null;
        const img = new Image();
        img.src = png;
        try {
          await img.decode();
        } catch {
          return null;
        }
        // transparência: um desenho único numa canvas auxiliar (sem canvas se o navegador não deixar)
        const probe = el("canvas");
        probe.width = img.naturalWidth;
        probe.height = img.naturalHeight;
        const g = probe.getContext("2d", { willReadFrequently: true });
        let transparent = false;
        if (g) {
          g.drawImage(img, 0, 0);
          transparent = hasTransparency(g.getImageData(0, 0, probe.width, probe.height).data);
        }
        return { img, w: img.naturalWidth, h: img.naturalHeight, alpha: null, transparent };
      })();
      cache.set(id, p);
    }
    return p;
  }

  function alphaCanvas(l: Loaded): HTMLCanvasElement | null {
    if (l.alpha) return l.alpha;
    const c = el("canvas");
    c.width = l.w;
    c.height = l.h;
    const g = c.getContext("2d", { willReadFrequently: true });
    if (!g) return null;
    g.drawImage(l.img, 0, 0);
    const data = g.getImageData(0, 0, l.w, l.h);
    alphaToGray(data.data);
    g.putImageData(data, 0, 0);
    return (l.alpha = c);
  }

  // ------------------------------------------------------------------ desenho

  function fit() {
    if (loaded) view = fitView(loaded.w, loaded.h, width, height);
    touched = false;
  }

  function path(ctx: CanvasRenderingContext2D, list: readonly number[], l: Loaded) {
    const uvs = host.model().uvs;
    for (const tri of list) {
      const o = tri * 6;
      const [ax, ay] = uvToPixel(uvs[o]!, uvs[o + 1]!, l.w, l.h, view);
      const [bx, by] = uvToPixel(uvs[o + 2]!, uvs[o + 3]!, l.w, l.h, view);
      const [cx, cy] = uvToPixel(uvs[o + 4]!, uvs[o + 5]!, l.w, l.h, view);
      ctx.moveTo(ax, ay);
      ctx.lineTo(bx, by);
      ctx.lineTo(cx, cy);
      ctx.closePath();
    }
  }

  function draw() {
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const l = loaded;
    if (!l) return;
    const x = view.ox;
    const y = view.oy;
    const w = l.w * view.scale;
    const h = l.h * view.scale;
    if (showChecker) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, w, h);
      ctx.clip();
      ctx.fillStyle = ctx.createPattern(checkerPattern, "repeat") ?? "#333";
      ctx.fillRect(x, y, w, h);
      ctx.restore();
    }
    ctx.imageSmoothingEnabled = view.scale < 1; // ampliado: pixels nítidos
    const src = showAlpha ? (alphaCanvas(l) ?? l.img) : l.img;
    ctx.drawImage(src, x, y, w, h);
    ctx.strokeStyle = "rgba(255,255,255,0.35)";
    ctx.lineWidth = 1;
    ctx.strokeRect(x - 0.5, y - 0.5, w + 1, h + 1);
    if (showWire && tris.length > 0) {
      ctx.beginPath();
      path(ctx, tris, l);
      ctx.strokeStyle = "rgba(76, 201, 240, 0.85)";
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    for (const [tri, fill, stroke] of [
      [selected, "rgba(255, 140, 40, 0.30)", "#ff8c28"],
      [hover, "rgba(255, 210, 74, 0.35)", "#ffd24a"],
    ] as const) {
      if (tri < 0) continue;
      ctx.beginPath();
      path(ctx, [tri], l);
      ctx.fillStyle = fill;
      ctx.fill();
      ctx.strokeStyle = stroke;
      ctx.lineWidth = 2;
      ctx.stroke();
    }
  }

  function resize() {
    width = wrap.clientWidth || 1;
    height = wrap.clientHeight || 1;
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    if (!touched) fit();
    draw();
  }
  const ro = new ResizeObserver(resize);
  ro.observe(wrap);

  // ------------------------------------------------------------------ seleção e lista

  function usage() {
    const m = host.model();
    return trianglesByTexture(m.texindex, host.map());
  }

  function syncHighlight() {
    const tri = hover >= 0 ? hover : selected;
    host.highlight(tri >= 0 ? [tri] : []);
  }

  async function select(id: number) {
    const mine = ++loadToken;
    selTex = id;
    hover = -1;
    selected = -1;
    tris = usage().get(id) ?? [];
    items.forEach((b, i) => b.classList.toggle("on", i === id));
    syncHighlight();
    const l = await loadTexture(id);
    if (mine !== loadToken) return;
    loaded = l;
    if (!l?.transparent) showAlpha = false;
    buildTools();
    buildInfo();
    fit();
    draw();
  }

  function buildList() {
    const m = host.model();
    const used = usage();
    items = [];
    list.replaceChildren();
    m.textures.forEach((tex, i) => {
      const count = used.get(i)?.length ?? 0;
      const li = el("li");
      const btn = el("button", "uv-item");
      btn.type = "button";
      btn.classList.toggle("unused", count === 0);
      btn.classList.toggle("on", i === selTex);
      const thumb = el("img", "uv-thumb");
      thumb.alt = "";
      if (tex.png) thumb.src = tex.png;
      const body = el("span", "uv-item-body");
      const name = el("span", "uv-name", tex.name);
      name.title = tex.name;
      const meta = el("span", "uv-meta");
      const size = el("span", "uv-size", "…");
      void loadTexture(i).then((l) => {
        size.textContent = l ? `${l.w}×${l.h}` : "–";
      });
      meta.append(size, el("span", "uv-tris", count > 0 ? t("skins.uv.tris", count) : t("skins.uv.unused")));
      body.append(name, meta);
      const flags = flagKeys(m.texture_info?.[i]);
      if (flags.length > 0) {
        const chips = el("span", "uv-flags");
        for (const k of flags) {
          const chip = el("span", `uv-flag f-${k}`, t(`skins.flag.${k}`));
          chip.title = t(`skins.flagHint.${k}`);
          chips.append(chip);
        }
        body.append(chips);
      }
      btn.append(thumb, body);
      btn.addEventListener("click", () => void select(i), on);
      // passar o mouse mostra no modelo onde a textura é usada
      btn.addEventListener("mouseenter", () => host.highlight(used.get(i) ?? []), on);
      btn.addEventListener("mouseleave", syncHighlight, on);
      btn.addEventListener("focus", () => host.highlight(used.get(i) ?? []), on);
      btn.addEventListener("blur", syncHighlight, on);
      li.append(btn);
      list.append(li);
      items.push(btn);
    });
  }

  function toggle(label: string, get: () => boolean, set: (v: boolean) => void, title?: string) {
    const row = el("label", "uv-check");
    if (title) row.title = title;
    const box = el("input");
    box.type = "checkbox";
    box.checked = get();
    box.addEventListener(
      "change",
      () => {
        set(box.checked);
        draw();
      },
      on,
    );
    row.append(box, el("span", undefined, label));
    return row;
  }

  function button(label: string, title: string, run: () => void) {
    const b = el("button", "mdl-mini", label);
    b.type = "button";
    b.title = title;
    b.setAttribute("aria-label", title);
    b.addEventListener("click", run, on);
    return b;
  }

  function zoomButton(factor: number) {
    touched = true;
    view = zoomAt(view, factor, width / 2, height / 2);
    draw();
  }

  function exportPng() {
    const tex = host.model().textures[selTex];
    if (!tex?.png) return;
    const a = document.createElement("a");
    a.href = tex.png;
    a.download = textureFileName(tex.name, `textura-${selTex}`);
    document.body.append(a);
    a.click();
    a.remove();
  }

  function buildTools() {
    tools.replaceChildren();
    if (selTex < 0) return;
    tools.append(
      toggle(t("skins.uv.wire"), () => showWire, (v) => (showWire = v)),
      toggle(t("skins.uv.checker"), () => showChecker, (v) => (showChecker = v)),
    );
    if (loaded?.transparent) tools.append(toggle(t("skins.uv.alpha"), () => showAlpha, (v) => (showAlpha = v), t("skins.uv.alphaHint")));
    tools.append(
      button("+", t("skins.uv.zoomIn"), () => zoomButton(1.25)),
      button("−", t("skins.uv.zoomOut"), () => zoomButton(0.8)),
      button(t("skins.uv.fit"), t("skins.uv.fitHint"), () => {
        fit();
        draw();
      }),
      button(t("skins.uv.export"), t("skins.uv.exportHint"), exportPng),
    );
  }

  function buildInfo() {
    const m = host.model();
    const tex = m.textures[selTex];
    if (!tex) {
      info.textContent = t("skins.uv.pick");
      return;
    }
    const size = loaded ? `${loaded.w}×${loaded.h}` : "–";
    info.textContent = `${tex.name} · ${size} · ${t("skins.uv.tris", tris.length)} · ${t("skins.uv.hint")}`;
  }

  // ------------------------------------------------------------------ mouse sobre o mapa

  function localPoint(e: MouseEvent): [number, number] {
    const r = canvas.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  }

  function trianglesAt(px: number, py: number): number {
    if (!loaded) return -1;
    const [u, v] = pixelToUv(px, py, loaded.w, loaded.h, view);
    return hitTestUv(host.model().uvs, tris, u, v, uvTolerance(2, loaded.w, loaded.h, view));
  }

  let drag: { x: number; y: number; moved: number } | null = null;
  canvas.addEventListener(
    "pointerdown",
    (e) => {
      drag = { x: e.clientX, y: e.clientY, moved: 0 };
      canvas.setPointerCapture(e.pointerId);
    },
    on,
  );
  canvas.addEventListener(
    "pointermove",
    (e) => {
      if (drag) {
        const dx = e.clientX - drag.x;
        const dy = e.clientY - drag.y;
        drag.moved += Math.abs(dx) + Math.abs(dy);
        drag.x = e.clientX;
        drag.y = e.clientY;
        if (drag.moved > 4) {
          touched = true;
          view = panBy(view, dx, dy);
          canvas.classList.add("panning");
          draw();
        }
        return;
      }
      const [px, py] = localPoint(e);
      const hit = trianglesAt(px, py);
      if (hit !== hover) {
        hover = hit;
        syncHighlight();
        draw();
      }
    },
    on,
  );
  const endDrag = (e: PointerEvent) => {
    const d = drag;
    drag = null;
    canvas.classList.remove("panning");
    if (d && d.moved <= 4 && e.type === "pointerup") {
      // clique sem arrastar: fixa o triângulo sob o cursor (ou limpa)
      const [px, py] = localPoint(e);
      selected = trianglesAt(px, py);
      syncHighlight();
      draw();
    }
  };
  canvas.addEventListener("pointerup", endDrag, on);
  canvas.addEventListener("pointercancel", endDrag, on);
  canvas.addEventListener(
    "pointerleave",
    () => {
      if (hover < 0) return;
      hover = -1;
      syncHighlight();
      draw();
    },
    on,
  );
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const [px, py] = localPoint(e);
      touched = true;
      view = zoomAt(view, Math.exp(-e.deltaY * 0.0015), px, py);
      draw();
    },
    { ...on, passive: false },
  );
  canvas.addEventListener("dblclick", () => {
    fit();
    draw();
  }, on);

  // ------------------------------------------------------------------ API

  /** primeira textura em uso (ou a 0) */
  function firstTexture(): number {
    const used = usage();
    for (const id of used.keys()) if (id < host.model().textures.length) return id;
    return host.model().textures.length > 0 ? 0 : -1;
  }

  function reset() {
    cache = new Map();
    loaded = null;
    const keep = selTex >= 0 && selTex < host.model().textures.length ? selTex : firstTexture();
    selTex = -1;
    buildList();
    if (keep >= 0) void select(keep);
    else {
      tools.replaceChildren();
      info.textContent = t("skins.uv.none");
      draw();
    }
  }

  reset();

  return {
    el: root,
    setModel: reset,
    setMap() {
      if (selTex < 0) return;
      buildList();
      const id = selTex;
      selTex = -1;
      void select(id);
    },
    selectTri(tri) {
      const m = host.model();
      const id = triTexture(m.texindex, host.map(), tri);
      if (id < 0 || id >= m.textures.length) return;
      void select(id).then(() => {
        selected = tri;
        syncHighlight();
        draw();
        items[id]?.scrollIntoView({ block: "nearest" });
      });
    },
    refresh() {
      buildList();
      buildTools();
      buildInfo();
    },
    dispose() {
      ctl.abort();
      ro.disconnect();
      loadToken++;
      host.highlight([]);
      root.remove();
    },
  };
}
