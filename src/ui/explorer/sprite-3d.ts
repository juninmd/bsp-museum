// Visualização 3D de sprites `.spr` no Explorador: cena Three.js com chão em grade, eixos,
// órbita (arrastar/zoom) e o sprite posicionado como o motor faz (regras em lib/sprite3d.ts).
//
// Duas visões: "câmera do jogo" (a câmera de render É a câmera que orienta o billboard) e
// "observador" (um orbit separado enxerga a câmera do jogo desenhada como um frustum; é onde
// dá para ver o billboard reagir enquanto a câmera do mundo gira sozinha).
//
// Custo: um WebGLRenderer por player (criado só ao entrar no 3D), laço de render sob demanda
// (só roda contínuo com "girar automaticamente"), textura recriada só quando o frame muda
// (cache pequeno por frame/imagem) e `dispose()` solta renderer, geometrias e texturas.

import * as THREE from "three";
import { t } from "../../i18n.ts";
import type { SpriteSegment } from "../../lib/explorer.ts";
import {
  blendFor,
  cameraBasis,
  clampPitch,
  gridSpec,
  opacityOf,
  orbitEye,
  quadCorners,
  segmentKey,
  spriteAxes,
  type BlendOverride,
  type CameraBasis,
  type Vec3,
} from "../../lib/sprite3d.ts";
import type { SpriteInfo } from "../../types/formats.ts";
import { esc } from "./common.ts";

type Bg3d = "dark" | "light" | "sky";
type ViewMode = "game" | "observer";

export interface Sprite3DView {
  /** mostra/oculta (o laço de render só roda enquanto ativo) */
  setActive(on: boolean): void;
  /** segmento da animação exibido agora; só troca a textura se a chave mudou */
  setSegment(seg: SpriteSegment | undefined): void;
  setBlend(blend: BlendOverride): void;
  dispose(): void;
}

const GRID: Record<Bg3d, [number, number]> = {
  dark: [0x4a5a6c, 0x26313d],
  light: [0x6b7885, 0xb4bec8],
  sky: [0xeaf2fa, 0x9bb7d0],
};
const BG_COLOR: Record<Exclude<Bg3d, "sky">, number> = { dark: 0x0b1016, light: 0xe9edf1 };
const TEX_CACHE_MAX = 48;
const AUTO_ROTATE_DEG_PER_S = 24;

function skyTexture(): THREE.CanvasTexture {
  const c = document.createElement("canvas");
  c.width = 2;
  c.height = 256;
  const g = c.getContext("2d");
  if (g) {
    const grad = g.createLinearGradient(0, 0, 0, 256);
    grad.addColorStop(0, "#4f88c6");
    grad.addColorStop(1, "#d3e6f5");
    g.fillStyle = grad;
    g.fillRect(0, 0, 2, 256);
  }
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

function slider(cls: string, label: string, min: number, max: number, step: number, value: number): string {
  return `<label class="spr-ctl"><span>${esc(label)}</span>
    <input type="range" class="${cls}" min="${min}" max="${max}" step="${step}" value="${value}" aria-label="${esc(label)}" />
    <output class="${cls}-out">${value}</output></label>`;
}

function panelHtml(info: SpriteInfo): string {
  const type = info.sprite_type;
  const angles =
    type === "oriented"
      ? [t("exp.spr3d.pitch"), t("exp.spr3d.yaw"), t("exp.spr3d.roll")]
      : type === "parallel_oriented"
        ? [null, null, t("exp.spr3d.roll")]
        : null;
  const angleCtl = (i: number, label: string | null) =>
    label === null ? "" : slider(`s3d-angle s3d-angle${i}`, label, -180, 180, 1, 0);
  return `
    <div class="spr-controls s3d-row">
      <label class="spr-ctl"><span>${esc(t("exp.spr3d.view"))}</span>
        <select class="s3d-view">
          <option value="observer">${esc(t("exp.spr3d.view.observer"))}</option>
          <option value="game">${esc(t("exp.spr3d.view.game"))}</option>
        </select></label>
      <label class="spr-ctl chk"><input type="checkbox" class="s3d-auto" /> <span>${esc(t("exp.spr3d.autoRotate"))}</span></label>
      ${slider("s3d-roll", t("exp.spr3d.camRoll"), -180, 180, 1, 0)}
      <label class="spr-ctl chk"><input type="checkbox" class="s3d-showcam" checked /> <span>${esc(t("exp.spr3d.showCamera"))}</span></label>
      <label class="spr-ctl chk"><input type="checkbox" class="s3d-sphere" checked /> <span>${esc(t("exp.spr3d.showSphere"))}</span></label>
      <button type="button" class="s3d-reset">${esc(t("exp.spr3d.resetCamera"))}</button>
    </div>
    <div class="spr-controls s3d-row">
      ${slider("s3d-scale", t("exp.spr3d.scale"), 0.25, 4, 0.05, 1)}
      ${slider("s3d-amt", t("exp.spr3d.renderamt"), 0, 255, 1, 255)}
      <label class="spr-ctl"><span>${esc(t("exp.spr3d.rendercolor"))}</span>
        <input type="color" class="s3d-color" value="#ffffff" aria-label="${esc(t("exp.spr3d.rendercolor"))}" /></label>
      <label class="spr-ctl"><span>${esc(t("exp.spr3d.bg"))}</span>
        <select class="s3d-bg">
          <option value="dark">${esc(t("exp.spr3d.bg.dark"))}</option>
          <option value="light">${esc(t("exp.spr3d.bg.light"))}</option>
          <option value="sky">${esc(t("exp.spr3d.bg.sky"))}</option>
        </select></label>
      ${
        angles
          ? `<span class="spr-ctl s3d-angles" title="${esc(type === "oriented" ? t("exp.spr3d.angles") : t("exp.spr3d.anglesRollOnly"))}"><b>${esc(t("exp.spr3d.angles"))}</b>
          ${angles.map((l, i) => angleCtl(i, l)).join("")}</span>`
          : ""
      }
    </div>
    <p class="s3d-rule dim">${esc(t(`exp.spr3d.rule.${type}`))}</p>
    <p class="s3d-note dim">${esc(t("exp.spr3d.legend", String(info.bounding_radius), "1.00×"))}</p>
    <p class="s3d-warn" hidden>${esc(t("exp.spr3d.degenerate"))}</p>`;
}

/**
 * Monta a cena 3D. `stageHost` recebe o `<canvas>`; `panelHost`, os controles. `images` são os
 * `<img>` já decodificados pelo player 2D (mesmos de `info.frames[f].images[i]`).
 */
export function createSprite3D(
  stageHost: HTMLElement,
  panelHost: HTMLElement,
  info: SpriteInfo,
  images: readonly (readonly HTMLImageElement[])[],
  initialBlend: BlendOverride,
): Sprite3DView {
  const defaultBg: Bg3d = info.tex_format === "additive" ? "dark" : "sky";
  panelHost.innerHTML = panelHtml(info);
  const q = <E extends HTMLElement>(sel: string) => panelHost.querySelector<E>(sel);
  const need = <E extends HTMLElement>(sel: string) => q<E>(sel)!;

  let renderer: THREE.WebGLRenderer;
  try {
    renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "low-power" });
  } catch {
    stageHost.innerHTML = `<p class="dim">${esc(t("exp.spr3d.noWebgl"))}</p>`;
    return { setActive() {}, setSegment() {}, setBlend() {}, dispose() {} };
  }
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  const canvas = renderer.domElement;
  canvas.className = "s3d-canvas";
  canvas.tabIndex = 0;
  canvas.setAttribute("role", "img");
  canvas.setAttribute("aria-label", t("exp.spr3d.stageAria"));
  stageHost.replaceChildren(canvas);

  // ---------------------------------------------------------------- estado
  const maxDim = Math.max(1, info.max_width, info.max_height);
  const origin: Vec3 = [0, 0, Math.round(maxDim * 0.75)];
  const orbitDefaults = {
    game: { yaw: 210, pitch: 16, dist: maxDim * 2.6 },
    observer: { yaw: 168, pitch: 22, dist: maxDim * 4.2 },
  };
  const orbit = { game: { ...orbitDefaults.game }, observer: { ...orbitDefaults.observer } };
  const st = {
    view: "observer" as ViewMode,
    autoRotate: false,
    camRoll: 0,
    showCam: true,
    showSphere: true,
    scale: 1,
    amt: 255,
    color: "#ffffff",
    bg: defaultBg as Bg3d,
    angles: [0, 0, 0] as Vec3,
    blend: initialBlend,
  };
  need<HTMLSelectElement>(".s3d-bg").value = defaultBg;
  need<HTMLSelectElement>(".s3d-view").value = st.view;

  // ---------------------------------------------------------------- cena
  const scene = new THREE.Scene();
  let skyTex: THREE.CanvasTexture | null = null;
  const camera = new THREE.PerspectiveCamera(50, 1, 0.5, 60000);
  camera.up.set(0, 0, 1);

  let grid: THREE.GridHelper | null = null;
  function applyBackground() {
    if (st.bg === "sky") {
      skyTex ??= skyTexture();
      scene.background = skyTex;
    } else {
      scene.background = new THREE.Color(BG_COLOR[st.bg]);
    }
    if (grid) {
      scene.remove(grid);
      grid.geometry.dispose();
      (grid.material as THREE.Material).dispose();
    }
    const { size, divisions } = gridSpec(maxDim);
    grid = new THREE.GridHelper(size, divisions, GRID[st.bg][0], GRID[st.bg][1]);
    grid.rotation.x = Math.PI / 2; // GridHelper nasce no plano XZ; o chão do motor é XY
    scene.add(grid);
  }

  // eixos no ponto de origem do sprite e a linha até o chão
  const axes = new THREE.AxesHelper(maxDim * 0.6);
  axes.position.set(...origin);
  scene.add(axes);
  const dropGeom = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(origin[0], origin[1], 0), new THREE.Vector3(...origin)]);
  const dropMat = new THREE.LineBasicMaterial({ color: 0x8b949e, transparent: true, opacity: 0.6 });
  const drop = new THREE.Line(dropGeom, dropMat);
  scene.add(drop);

  // esfera do raio de colisão (bounding radius)
  const sphereGeom = new THREE.SphereGeometry(1, 16, 12);
  const sphereMat = new THREE.MeshBasicMaterial({ color: 0x58a6ff, wireframe: true, transparent: true, opacity: 0.22, depthWrite: false });
  const sphere = new THREE.Mesh(sphereGeom, sphereMat);
  sphere.position.set(...origin);
  sphere.renderOrder = 5;
  scene.add(sphere);

  // o quad do sprite: 4 vértices reescritos a cada render (billboard calculado na CPU)
  const quadPos = new Float32Array(12);
  const quadGeom = new THREE.BufferGeometry();
  quadGeom.setAttribute("position", new THREE.BufferAttribute(quadPos, 3).setUsage(THREE.DynamicDrawUsage));
  quadGeom.setAttribute("uv", new THREE.BufferAttribute(new Float32Array([0, 0, 1, 0, 1, 1, 0, 1]), 2));
  quadGeom.setIndex([0, 1, 2, 0, 2, 3]);
  const mat = new THREE.MeshBasicMaterial({ side: THREE.DoubleSide, toneMapped: false });
  const quad = new THREE.Mesh(quadGeom, mat);
  quad.frustumCulled = false;
  quad.renderOrder = 10;
  quad.visible = false;
  scene.add(quad);

  // a câmera "mundo" (a que orienta o billboard), desenhada na visão do observador
  const camPts = new Float32Array(24 * 3);
  const camGeom = new THREE.BufferGeometry();
  camGeom.setAttribute("position", new THREE.BufferAttribute(camPts, 3).setUsage(THREE.DynamicDrawUsage));
  const camMat = new THREE.LineBasicMaterial({ color: 0xf0883e });
  const camGizmo = new THREE.LineSegments(camGeom, camMat);
  camGizmo.frustumCulled = false;
  camGizmo.renderOrder = 6;
  scene.add(camGizmo);

  function updateGizmo(eye: Vec3, b: CameraBasis) {
    const s = maxDim * 0.35;
    const d = s * 1.2;
    const hw = s * 0.6;
    const hh = s * 0.45;
    const base = (sx: number, sy: number): Vec3 => [
      eye[0] + b.forward[0] * d + b.right[0] * hw * sx + b.up[0] * hh * sy,
      eye[1] + b.forward[1] * d + b.right[1] * hw * sx + b.up[1] * hh * sy,
      eye[2] + b.forward[2] * d + b.right[2] * hw * sx + b.up[2] * hh * sy,
    ];
    const c = [base(-1, -1), base(1, -1), base(1, 1), base(-1, 1)];
    const topMid = base(0, 1);
    const tip: Vec3 = [topMid[0] + b.up[0] * hh * 0.6, topMid[1] + b.up[1] * hh * 0.6, topMid[2] + b.up[2] * hh * 0.6];
    const tl = base(-0.35, 1);
    const tr = base(0.35, 1);
    const segs: Vec3[] = [
      eye, c[0]!, eye, c[1]!, eye, c[2]!, eye, c[3]!,
      c[0]!, c[1]!, c[1]!, c[2]!, c[2]!, c[3]!, c[3]!, c[0]!,
      tl, tip, tip, tr, tr, tl,
      eye, origin,
    ];
    segs.forEach((p, i) => camPts.set(p, i * 3));
    camGeom.attributes.position!.needsUpdate = true;
  }

  // ---------------------------------------------------------------- textura (só troca com o frame)
  const texCache = new Map<string, THREE.Texture>();
  let curSeg: SpriteSegment | undefined;
  let appliedKey = "";

  function textureFor(seg: SpriteSegment, key: string): THREE.Texture | null {
    const hit = texCache.get(key);
    if (hit) return hit;
    const img = images[seg.frame]?.[seg.image];
    if (!img || !img.complete || !img.naturalWidth) return null;
    const tex = new THREE.Texture(img);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.magFilter = THREE.NearestFilter;
    tex.minFilter = THREE.NearestFilter;
    tex.generateMipmaps = false;
    tex.needsUpdate = true;
    texCache.set(key, tex);
    if (texCache.size > TEX_CACHE_MAX) {
      for (const [k, v] of texCache) {
        if (k === key) continue;
        v.dispose();
        texCache.delete(k);
        break;
      }
    }
    return tex;
  }

  // ---------------------------------------------------------------- material
  let matKey = "";
  function applyMaterial() {
    const opacity = opacityOf(st.amt);
    const spec = blendFor(info.tex_format, st.blend, opacity);
    mat.opacity = opacity;
    mat.color.set(st.color);
    const key = `${spec.mode}|${spec.transparent}|${spec.depthWrite}|${spec.depthTest}|${spec.alphaTest}`;
    if (key !== matKey) {
      matKey = key;
      mat.transparent = spec.transparent;
      mat.blending = spec.additive ? THREE.AdditiveBlending : THREE.NormalBlending;
      mat.depthWrite = spec.depthWrite;
      mat.depthTest = spec.depthTest;
      mat.alphaTest = spec.alphaTest;
      mat.needsUpdate = true;
    }
  }

  // ---------------------------------------------------------------- render
  let active = false;
  let disposed = false;
  let raf = 0;
  let lastT = 0;
  const warn = need<HTMLElement>(".s3d-warn");
  const note = need<HTMLElement>(".s3d-note");
  let warnShown = false;

  function render() {
    if (disposed || !active) return;
    const eye = orbitEye(origin, orbit.game.yaw, orbit.game.pitch, orbit.game.dist);
    const basis = cameraBasis(eye, origin, st.camRoll);
    const ax = spriteAxes(info.sprite_type, { eye, basis }, origin, st.angles);
    const meta = curSeg ? info.frames[curSeg.frame]?.images[curSeg.image] : undefined;
    const ready = !!meta && !!mat.map;
    quad.visible = ready && ax.valid;
    if (ready && meta && ax.valid) {
      const corners = quadCorners(ax, origin, meta, st.scale);
      corners.forEach((p, i) => quadPos.set(p, i * 3));
      quadGeom.attributes.position!.needsUpdate = true;
    }
    if (warnShown === ax.valid) {
      warnShown = !ax.valid;
      warn.hidden = ax.valid;
    }

    sphere.visible = st.showSphere && info.bounding_radius > 0;
    sphere.scale.setScalar(Math.max(0.001, info.bounding_radius * st.scale));

    let camEye = eye;
    let camBasis = basis;
    if (st.view === "observer") {
      camEye = orbitEye(origin, orbit.observer.yaw, orbit.observer.pitch, orbit.observer.dist);
      camBasis = cameraBasis(camEye, origin);
      updateGizmo(eye, basis);
    }
    camGizmo.visible = st.view === "observer" && st.showCam;
    camera.position.set(...camEye);
    camera.up.set(...camBasis.up);
    camera.lookAt(origin[0], origin[1], origin[2]);
    renderer.render(scene, camera);
  }

  function frame(now: number) {
    raf = 0;
    if (disposed || !active) return;
    if (st.autoRotate) {
      const dt = lastT ? Math.min(0.1, (now - lastT) / 1000) : 0;
      lastT = now;
      orbit.game.yaw = (orbit.game.yaw + AUTO_ROTATE_DEG_PER_S * dt) % 360;
    } else {
      lastT = 0;
    }
    render();
    if (st.autoRotate) raf = requestAnimationFrame(frame);
  }

  function schedule() {
    if (raf || disposed || !active) return;
    raf = requestAnimationFrame(frame);
  }

  function resize() {
    const w = stageHost.clientWidth;
    const h = stageHost.clientHeight;
    if (w < 2 || h < 2) return;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    schedule();
  }
  const ro = new ResizeObserver(resize);
  ro.observe(stageHost);

  // ---------------------------------------------------------------- entrada (órbita)
  const cur = () => orbit[st.view];
  let drag: { id: number; x: number; y: number } | null = null;
  canvas.addEventListener("pointerdown", (e) => {
    drag = { id: e.pointerId, x: e.clientX, y: e.clientY };
    canvas.setPointerCapture(e.pointerId);
    canvas.focus({ preventScroll: true });
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!drag || drag.id !== e.pointerId) return;
    const o = cur();
    o.yaw -= (e.clientX - drag.x) * 0.4;
    o.pitch = clampPitch(o.pitch + (e.clientY - drag.y) * 0.4);
    drag.x = e.clientX;
    drag.y = e.clientY;
    schedule();
  });
  const endDrag = (e: PointerEvent) => {
    if (drag && drag.id === e.pointerId) drag = null;
  };
  canvas.addEventListener("pointerup", endDrag);
  canvas.addEventListener("pointercancel", endDrag);
  const zoom = (factor: number) => {
    const o = cur();
    o.dist = Math.max(maxDim * 0.5, Math.min(maxDim * 40, o.dist * factor));
    schedule();
  };
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      zoom(Math.exp(e.deltaY * 0.001));
    },
    { passive: false },
  );
  canvas.addEventListener("keydown", (e) => {
    const o = cur();
    const step = 5;
    if (e.key === "ArrowLeft") o.yaw += step;
    else if (e.key === "ArrowRight") o.yaw -= step;
    else if (e.key === "ArrowUp") o.pitch = clampPitch(o.pitch + step);
    else if (e.key === "ArrowDown") o.pitch = clampPitch(o.pitch - step);
    else if (e.key === "+" || e.key === "=") zoom(0.9);
    else if (e.key === "-") zoom(1.1);
    else return;
    e.preventDefault();
    schedule();
  });

  // ---------------------------------------------------------------- controles
  const bind = <E extends HTMLElement>(sel: string, ev: string, fn: (el: E) => void) => {
    const el = q<E>(sel);
    if (el) el.addEventListener(ev, () => fn(el));
  };
  const out = (sel: string, text: string) => {
    const el = q<HTMLElement>(`${sel}-out`);
    if (el) el.textContent = text;
  };
  bind<HTMLSelectElement>(".s3d-view", "change", (el) => {
    st.view = el.value as ViewMode;
    schedule();
  });
  bind<HTMLInputElement>(".s3d-auto", "change", (el) => {
    st.autoRotate = el.checked;
    lastT = 0;
    schedule();
  });
  bind<HTMLInputElement>(".s3d-roll", "input", (el) => {
    st.camRoll = Number(el.value);
    out(".s3d-roll", `${st.camRoll}°`);
    schedule();
  });
  bind<HTMLInputElement>(".s3d-showcam", "change", (el) => {
    st.showCam = el.checked;
    schedule();
  });
  bind<HTMLInputElement>(".s3d-sphere", "change", (el) => {
    st.showSphere = el.checked;
    schedule();
  });
  bind<HTMLButtonElement>(".s3d-reset", "click", () => {
    orbit.game = { ...orbitDefaults.game };
    orbit.observer = { ...orbitDefaults.observer };
    schedule();
  });
  bind<HTMLInputElement>(".s3d-scale", "input", (el) => {
    st.scale = Number(el.value);
    out(".s3d-scale", `${st.scale.toFixed(2)}×`);
    note.textContent = t("exp.spr3d.legend", String(info.bounding_radius), `${st.scale.toFixed(2)}×`);
    schedule();
  });
  bind<HTMLInputElement>(".s3d-amt", "input", (el) => {
    st.amt = Number(el.value);
    out(".s3d-amt", String(st.amt));
    applyMaterial();
    schedule();
  });
  bind<HTMLInputElement>(".s3d-color", "input", (el) => {
    st.color = el.value;
    applyMaterial();
    schedule();
  });
  bind<HTMLSelectElement>(".s3d-bg", "change", (el) => {
    st.bg = el.value as Bg3d;
    applyBackground();
    schedule();
  });
  for (let i = 0; i < 3; i++) {
    bind<HTMLInputElement>(`.s3d-angle${i}`, "input", (el) => {
      st.angles[i] = Number(el.value);
      out(`.s3d-angle${i}`, `${st.angles[i]}°`);
      schedule();
    });
  }
  out(".s3d-roll", "0°");
  out(".s3d-scale", "1.00×");
  for (let i = 0; i < 3; i++) out(`.s3d-angle${i}`, "0°");

  applyBackground();
  applyMaterial();

  return {
    setActive(on) {
      if (disposed || on === active) return;
      active = on;
      lastT = 0;
      if (on) {
        resize();
        schedule();
      } else if (raf) {
        cancelAnimationFrame(raf);
        raf = 0;
      }
    },
    setSegment(seg) {
      curSeg = seg;
      const key = segmentKey(seg);
      if (key !== "" && key === appliedKey) return; // mesmo frame: nada a recriar nem a redesenhar
      if (seg && key) {
        const tex = textureFor(seg, key);
        if (tex) {
          if (!mat.map) mat.needsUpdate = true; // primeira textura: o shader ganha `USE_MAP`
          mat.map = tex;
          appliedKey = key;
        }
      } else if (!seg) {
        appliedKey = "";
      }
      schedule();
    },
    setBlend(blend) {
      st.blend = blend;
      applyMaterial();
      schedule();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      if (raf) cancelAnimationFrame(raf);
      ro.disconnect();
      for (const tex of texCache.values()) tex.dispose();
      texCache.clear();
      skyTex?.dispose();
      if (grid) {
        grid.geometry.dispose();
        (grid.material as THREE.Material).dispose();
      }
      axes.dispose();
      for (const g of [dropGeom, sphereGeom, quadGeom, camGeom]) g.dispose();
      for (const m of [dropMat, sphereMat, mat, camMat]) m.dispose();
      renderer.dispose();
      renderer.forceContextLoss();
      canvas.remove();
    },
  };
}
