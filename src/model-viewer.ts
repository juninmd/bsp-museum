import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { t } from "./i18n.ts";
import { BOX_STRIDE, hitGroupColor } from "./lib/mdl-controls.ts";
import { MdlRig, addModelLights, decodeTextures, emptyTextures, toWorld, type Look, type Model, type TextureSet } from "./mdl-rig.ts";

export type { Model } from "./mdl-rig.ts";

/** Um lado da comparação: família de skin e, opcionalmente, outro modelo carregado (cores de time) de onde vêm as texturas. */
export interface LookSpec {
  /** `map[textura base] = textura da família` */
  map: readonly number[];
  /** modelo com as texturas deste lado (ex.: `load_model` com `top`/`bottom`); ausente = o do visualizador */
  model?: Model;
}

/**
 * Comparação de dois looks na mesma cena e com a mesma câmera (a malha é uma só; cada lado é um jogo de materiais):
 * `side` = dois quadros lado a lado; `split` = antes/depois com divisor arrastável.
 */
export interface CompareSpec {
  mode: "side" | "split";
  a: LookSpec;
  b: LookSpec;
  labels: [string, string];
  /** posição do divisor (0..1) no modo `split` */
  pos?: number;
}

/**
 * Cena 3D própria do visualizador avulso de `.mdl` (independente de
 * `viewer3d.ts`, que é do mapa): orbitável, com a malha dividida por textura
 * e os vértices reposicionáveis a cada quadro (skinning feito por quem chama).
 * Materiais seguem as flags do modelo (chrome, aditivo, máscara, fullbright,
 * flatshade); hitboxes e attachments são overlays opcionais. A geometria vem
 * de `MdlRig` e é reaproveitada por todas as skins.
 */
export interface ModelViewer {
  /** reposiciona os vértices: mesmo layout de `MdlSummary.positions` (9 floats por triângulo, espaço do jogo) */
  setVertexPositions(flat: ArrayLike<number>): void;
  /** enquadra a câmera na pose atual (deformada) e volta à vista 3/4 da frente */
  frame(): void;
  /** troca a textura de cada índice de `MdlSummary.textures` (família de skin): `map[base] = textura nova` */
  setTextureMap(map: number[]): void;
  /**
   * Troca o modelo (bodygroup, cor de time): decodifica as texturas novas e só então substitui a malha,
   * sem mexer na câmera. Quem chama reposiciona os vértices depois.
   */
  setModel(model: Model): Promise<void>;
  /** caixas de acerto do quadro: `n * 10` floats (centro, meias-arestas, quaternion xyzw); `null` esconde */
  setHitboxes(boxes: ArrayLike<number> | null, groups?: readonly number[]): void;
  /** origem de cada attachment (x,y,z em espaço do jogo); `null` esconde */
  setAttachments(points: ArrayLike<number> | null, names?: readonly string[]): void;
  /** liga/desliga a comparação de skins (`null` volta à vista única); resolve quando as texturas dos lados estão prontas */
  setCompare(spec: CompareSpec | null): Promise<void>;
  /** realça triângulos (índices de `texindex`) no modelo; lista vazia apaga */
  setHighlight(tris: readonly number[]): void;
  /** clique (sem arrastar) em um triângulo do modelo; devolve a função que remove o ouvinte */
  onPick(listener: (tri: number) => void): () => void;
  dispose(): void;
}

/** arestas de um cubo: pares de cantos (índice = bits sx|sy<<1|sz<<2) que diferem em um bit */
const BOX_EDGES: [number, number][] = (() => {
  const edges: [number, number][] = [];
  for (let a = 0; a < 8; a++) for (let bit = 1; bit < 8; bit <<= 1) if (!(a & bit)) edges.push([a, a | bit]);
  return edges;
})();

/** Quaternion xyzw aplicado a um vetor (espaço do jogo). */
function rotate(q: ArrayLike<number>, o: number, x: number, y: number, z: number): [number, number, number] {
  const qx = q[o]!;
  const qy = q[o + 1]!;
  const qz = q[o + 2]!;
  const qw = q[o + 3]!;
  const tx = 2 * (qy * z - qz * y);
  const ty = 2 * (qz * x - qx * z);
  const tz = 2 * (qx * y - qy * x);
  return [x + qw * tx + (qy * tz - qz * ty), y + qw * ty + (qz * tx - qx * tz), z + qw * tz + (qx * ty - qy * tx)];
}

/** Rótulo do attachment como sprite (texto numa canvas, sempre de frente pra câmera e por cima da malha). */
function labelSprite(text: string): THREE.Sprite {
  const canvas = document.createElement("canvas");
  canvas.width = 256;
  canvas.height = 64;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    ctx.font = "bold 34px system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.lineWidth = 6;
    ctx.strokeStyle = "rgba(13,17,23,0.9)";
    ctx.strokeText(text, 128, 32);
    ctx.fillStyle = "#ffd24a";
    ctx.fillText(text, 128, 32);
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: tex, depthTest: false, transparent: true }));
  sprite.renderOrder = 12;
  return sprite;
}

export function mountModelViewer3D(container: HTMLElement, initial: Model): ModelViewer {
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  container.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0d1117);
  addModelLights(scene);

  let rig = new MdlRig(initial);
  scene.add(rig.root);
  let baseTextures: TextureSet = emptyTextures(initial.textures.length);
  let primary: Look = rig.addLook([], baseTextures);
  let textureMap: number[] = [];
  let disposed = false;
  let swap = 0;
  let cmp: CompareSpec | null = null;
  let cmpLooks: Look[] = [];
  let cmpToken = 0;
  /** texturas de outros modelos (lados da comparação), por modelo */
  let extraSets = new Map<Model, Promise<TextureSet>>();
  const pickListeners = new Set<(tri: number) => void>();

  const setVertexPositions = (flat: ArrayLike<number>) => rig.setVertexPositions(flat);

  /** (re)cria o look principal a partir da família de skin atual */
  function refreshPrimary() {
    primary.dispose();
    primary = rig.addLook(textureMap, baseTextures);
    primary.group.visible = cmp === null;
  }

  const setTextureMap = (map: number[]) => {
    textureMap = map;
    refreshPrimary();
  };

  // Texturas do modelo inicial chegam depois do primeiro desenho.
  void decodeTextures(initial).then((set) => {
    if (disposed || rig.model !== initial) {
      set.dispose();
      return;
    }
    baseTextures = set;
    refreshPrimary();
    if (cmp) void applyCompare();
  });

  // Câmera: enquadrada na pose que está na tela (o bind pose fica com a origem nos pés, a
  // animação com a origem no quadril — enquadrar o bind deixava o modelo baixo e pequeno).
  const camera = new THREE.PerspectiveCamera(60, 1, 0.5, 100000);
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;

  /** centro/raio da pose atual; o enquadramento acompanha o modelo animado */
  const focus = { center: new THREE.Vector3(), radius: 40 };
  /** maior dimensão da pose atual (escala dos marcadores de attachment) */
  let maxDim = 80;
  /** direções padrão (mundo Y-up); no GoldSrc o modelo olha para +X (yaw 0) */
  const VIEW_DIRS = {
    hero: [1, 0.22, 0.55],
    front: [1, 0, 0],
    side: [0, 0, 1],
    back: [-1, 0, 0],
    top: [0, 1, 1e-4],
  } as const;
  type ViewName = keyof typeof VIEW_DIRS;

  function measure() {
    const box = rig.bounds();
    const size = box.getSize(new THREE.Vector3());
    focus.center.copy(box.getCenter(new THREE.Vector3()));
    focus.radius = Math.max(size.length() / 2, 1);
    maxDim = Math.max(size.x, size.y, size.z, 1);
    controls.minDistance = focus.radius * 0.35;
    controls.maxDistance = focus.radius * 12;
  }

  /** distância que faz a esfera do modelo caber no campo de visão, com 10% de folga */
  function fitDistance(): number {
    return (focus.radius / Math.sin((camera.fov * Math.PI) / 360)) * 1.1;
  }

  function lookFrom(dir: readonly number[], distance = fitDistance()) {
    const v = new THREE.Vector3(dir[0]!, dir[1]!, dir[2]!).normalize();
    camera.up.set(0, 1, 0);
    controls.target.copy(focus.center);
    camera.position.copy(focus.center).addScaledVector(v, Math.max(distance, focus.radius * 0.6));
    camera.near = Math.max(0.1, distance / 200);
    camera.far = Math.max(1000, distance * 40);
    camera.updateProjectionMatrix();
    controls.update();
  }

  measure();
  lookFrom(VIEW_DIRS.hero);

  // atalhos de vista (frente, lado, costas, topo) e "enquadrar"
  const views = document.createElement("div");
  views.className = "mdl-views";
  views.setAttribute("role", "group");
  views.setAttribute("aria-label", t("mdl.viewsAria"));
  const VIEW_BUTTONS: [ViewName | "fit", string][] = [
    ["hero", "mdl.view.hero"],
    ["front", "mdl.view.front"],
    ["side", "mdl.view.side"],
    ["back", "mdl.view.back"],
    ["top", "mdl.view.top"],
    ["fit", "mdl.view.fit"],
  ];
  for (const [id, key] of VIEW_BUTTONS) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "mdl-view-btn";
    b.dataset.view = id;
    b.textContent = t(key);
    b.title = t(`${key}.hint`);
    b.addEventListener("click", () => {
      if (id === "fit") frame();
      else lookFrom(VIEW_DIRS[id], id === "hero" ? fitDistance() : camera.position.distanceTo(controls.target));
    });
    views.append(b);
  }
  container.append(views);

  /** enquadra na pose atual e volta à vista 3/4 da frente */
  function frame() {
    measure();
    lookFrom(VIEW_DIRS.hero);
  }

  // ---- overlays: hitboxes (12 arestas por caixa, cor do grupo) e attachments (cruz + nome)
  const hitLines = new THREE.LineSegments(
    new THREE.BufferGeometry(),
    new THREE.LineBasicMaterial({ vertexColors: true, depthTest: false, transparent: true, opacity: 0.95 }),
  );
  hitLines.frustumCulled = false;
  hitLines.renderOrder = 10;
  hitLines.visible = false;
  hitLines.layers.enable(1);
  scene.add(hitLines);
  let hitCount = -1;
  let hitGroupsKey = "";

  const setHitboxes: ModelViewer["setHitboxes"] = (boxes, groups = []) => {
    if (!boxes || boxes.length < BOX_STRIDE) {
      hitLines.visible = false;
      return;
    }
    const n = Math.floor(boxes.length / BOX_STRIDE);
    const key = groups.join(",");
    const pos = new Float32Array(n * BOX_EDGES.length * 6);
    if (n !== hitCount || key !== hitGroupsKey) {
      const col = new Float32Array(n * BOX_EDGES.length * 6);
      const c = new THREE.Color();
      for (let i = 0; i < n; i++) {
        c.setHex(hitGroupColor(groups[i] ?? 0));
        for (let k = 0; k < BOX_EDGES.length * 2; k++) col.set([c.r, c.g, c.b], (i * BOX_EDGES.length * 2 + k) * 3);
      }
      hitLines.geometry.dispose();
      hitLines.geometry = new THREE.BufferGeometry();
      hitLines.geometry.setAttribute("color", new THREE.BufferAttribute(col, 3));
      hitCount = n;
      hitGroupsKey = key;
    }
    const corners: [number, number, number][] = new Array(8);
    for (let i = 0; i < n; i++) {
      const o = i * BOX_STRIDE;
      for (let k = 0; k < 8; k++) {
        const lx = (k & 1 ? 1 : -1) * boxes[o + 3]!;
        const ly = (k & 2 ? 1 : -1) * boxes[o + 4]!;
        const lz = (k & 4 ? 1 : -1) * boxes[o + 5]!;
        const [rx, ry, rz] = rotate(boxes, o + 6, lx, ly, lz);
        corners[k] = toWorld(boxes[o]! + rx, boxes[o + 1]! + ry, boxes[o + 2]! + rz);
      }
      BOX_EDGES.forEach(([a, b], e) => pos.set([...corners[a]!, ...corners[b]!], (i * BOX_EDGES.length + e) * 6));
    }
    hitLines.geometry.setAttribute("position", new THREE.BufferAttribute(pos, 3));
    hitLines.visible = true;
  };

  const attGroup = new THREE.Group();
  attGroup.visible = false;
  scene.add(attGroup);
  let attNames = "";

  /** meia-aresta da cruz do attachment, proporcional ao modelo */
  const armLen = () => maxDim * 0.025;

  const clearAttachments = () => {
    for (const child of [...attGroup.children]) {
      attGroup.remove(child);
      if (child instanceof THREE.Sprite) {
        child.material.map?.dispose();
        child.material.dispose();
      } else if (child instanceof THREE.LineSegments) {
        child.geometry.dispose();
        (child.material as THREE.Material).dispose();
      }
    }
  };

  const setAttachments: ModelViewer["setAttachments"] = (points, names = []) => {
    if (!points || points.length < 3) {
      attGroup.visible = false;
      return;
    }
    const n = Math.floor(points.length / 3);
    const key = names.slice(0, n).join("\u0000") + `#${n}`;
    if (key !== attNames) {
      clearAttachments();
      for (let i = 0; i < n; i++) {
        const a = armLen();
        const cross = new THREE.LineSegments(
          new THREE.BufferGeometry().setAttribute(
            "position",
            new THREE.Float32BufferAttribute([-a, 0, 0, a, 0, 0, 0, -a, 0, 0, a, 0, 0, 0, -a, 0, 0, a], 3),
          ),
          new THREE.LineBasicMaterial({ color: 0xffd24a, depthTest: false, transparent: true }),
        );
        cross.renderOrder = 11;
        cross.frustumCulled = false;
        const label = labelSprite(names[i] || `#${i}`);
        label.scale.set(maxDim * 0.16, maxDim * 0.04, 1);
        label.frustumCulled = false;
        cross.layers.enable(1);
        label.layers.enable(1);
        attGroup.add(cross, label);
      }
      attNames = key;
    }
    for (let i = 0; i < n; i++) {
      const [x, y, z] = toWorld(points[i * 3]!, points[i * 3 + 1]!, points[i * 3 + 2]!);
      attGroup.children[i * 2]!.position.set(x, y, z);
      attGroup.children[i * 2 + 1]!.position.set(x, y + maxDim * 0.04, z);
    }
    attGroup.visible = true;
  };

  // ---- comparação de skins: dois looks sobre a mesma geometria, cada um em uma camada; a câmera é uma só

  const overlay = document.createElement("div");
  overlay.className = "skin-cmp";
  overlay.hidden = true;
  const divider = document.createElement("div");
  divider.className = "skin-cmp-divider";
  const handle = document.createElement("div");
  handle.className = "skin-cmp-handle";
  handle.setAttribute("role", "separator");
  handle.tabIndex = 0;
  divider.append(handle);
  const tagA = document.createElement("span");
  const tagB = document.createElement("span");
  tagA.className = "skin-cmp-tag a";
  tagB.className = "skin-cmp-tag b";
  overlay.append(divider, tagA, tagB);
  container.appendChild(overlay);

  function layoutOverlay() {
    if (!cmp) return;
    const pos = cmp.mode === "split" ? (cmp.pos ?? 0.5) : 0.5;
    divider.style.left = `${pos * 100}%`;
    divider.classList.toggle("fixed", cmp.mode === "side");
    tagA.textContent = cmp.labels[0];
    tagB.textContent = cmp.labels[1];
    // lado a lado: cada rótulo no seu quadro; antes/depois: nas pontas
    tagA.style.left = "0.5rem";
    tagB.style.left = cmp.mode === "side" ? "calc(50% + 0.5rem)" : "";
    tagB.style.right = cmp.mode === "side" ? "" : "0.5rem";
    handle.setAttribute("aria-orientation", "vertical");
    handle.setAttribute("aria-valuenow", String(Math.round(pos * 100)));
  }

  const ctlEvents = new AbortController();
  const listen = { signal: ctlEvents.signal };
  let dragging = false;
  const moveDivider = (clientX: number) => {
    if (!cmp || cmp.mode !== "split") return;
    const r = container.getBoundingClientRect();
    cmp.pos = Math.min(0.95, Math.max(0.05, (clientX - r.left) / Math.max(1, r.width)));
    layoutOverlay();
  };
  handle.addEventListener(
    "pointerdown",
    (e) => {
      if (!cmp || cmp.mode !== "split") return;
      dragging = true;
      handle.setPointerCapture(e.pointerId);
      e.preventDefault();
    },
    listen,
  );
  handle.addEventListener("pointermove", (e) => dragging && moveDivider(e.clientX), listen);
  const endDrag = () => (dragging = false);
  handle.addEventListener("pointerup", endDrag, listen);
  handle.addEventListener("pointercancel", endDrag, listen);
  handle.addEventListener(
    "keydown",
    (e) => {
      if (!cmp || cmp.mode !== "split" || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
      cmp.pos = Math.min(0.95, Math.max(0.05, (cmp.pos ?? 0.5) + (e.key === "ArrowLeft" ? -0.03 : 0.03)));
      layoutOverlay();
      e.preventDefault();
    },
    listen,
  );

  function textureSetFor(spec: LookSpec): Promise<TextureSet> {
    if (!spec.model || spec.model === rig.model) return Promise.resolve(baseTextures);
    let p = extraSets.get(spec.model);
    if (!p) extraSets.set(spec.model, (p = decodeTextures(spec.model)));
    return p;
  }

  function dropCompareLooks() {
    for (const l of cmpLooks) l.dispose();
    cmpLooks = [];
  }

  /** descarta as texturas de modelos que nenhum lado usa mais */
  function pruneExtraSets(keep: readonly (Model | undefined)[]) {
    for (const [model, p] of [...extraSets]) {
      if (keep.includes(model)) continue;
      extraSets.delete(model);
      void p.then((s) => s.dispose());
    }
  }

  async function applyCompare() {
    const mine = ++cmpToken;
    const spec = cmp;
    if (!spec) return;
    const [setA, setB] = await Promise.all([textureSetFor(spec.a), textureSetFor(spec.b)]);
    if (disposed || mine !== cmpToken || cmp !== spec) return;
    dropCompareLooks();
    cmpLooks = [rig.addLook(spec.a.map, setA, 0), rig.addLook(spec.b.map, setB, 1)];
    primary.group.visible = false;
    overlay.hidden = false;
    layoutOverlay();
  }

  const setCompare: ModelViewer["setCompare"] = async (spec) => {
    cmp = spec ? { ...spec, pos: spec.pos ?? 0.5 } : null;
    if (!cmp) {
      cmpToken++;
      dropCompareLooks();
      pruneExtraSets([]);
      overlay.hidden = true;
      primary.group.visible = true;
      return;
    }
    pruneExtraSets([cmp.a.model, cmp.b.model]);
    await applyCompare();
  };

  // ---- seleção por clique: o raio sai do quadro (e da camada) em que o clique caiu
  let down: { x: number; y: number } | null = null;
  const canvas = renderer.domElement;
  canvas.addEventListener("pointerdown", (e) => (down = { x: e.clientX, y: e.clientY }), listen);
  canvas.addEventListener(
    "pointerup",
    (e) => {
      const start = down;
      down = null;
      if (!start || pickListeners.size === 0 || Math.hypot(e.clientX - start.x, e.clientY - start.y) > 4) return;
      const r = canvas.getBoundingClientRect();
      let nx = (e.clientX - r.left) / Math.max(1, r.width);
      const ny = (e.clientY - r.top) / Math.max(1, r.height);
      let layer = 0;
      let aspect = r.width / Math.max(1, r.height);
      if (cmp && cmpLooks.length === 2) {
        if (cmp.mode === "side") {
          layer = nx >= 0.5 ? 1 : 0;
          nx = (nx - layer * 0.5) * 2;
          aspect = r.width / 2 / Math.max(1, r.height); // a projeção do quadro, não a da tela inteira
        } else {
          layer = nx >= (cmp.pos ?? 0.5) ? 1 : 0;
        }
      }
      const savedAspect = camera.aspect;
      camera.aspect = aspect;
      camera.updateProjectionMatrix();
      const tri = rig.pick(nx * 2 - 1, -(ny * 2 - 1), camera, layer);
      camera.aspect = savedAspect;
      camera.updateProjectionMatrix();
      if (tri >= 0) for (const l of pickListeners) l(tri);
    },
    listen,
  );

  const setModel = async (model: Model) => {
    const mine = ++swap;
    const textures = await decodeTextures(model);
    if (disposed || mine !== swap) {
      textures.dispose();
      return;
    }
    const oldTextures = baseTextures;
    const oldExtras = extraSets;
    primary.dispose();
    dropCompareLooks();
    rig.dispose();
    rig = new MdlRig(model);
    scene.add(rig.root);
    measure(); // mantém o centro do enquadramento coerente com o novo bodygroup (sem mexer na câmera)
    baseTextures = textures;
    extraSets = new Map();
    primary = rig.addLook(textureMap, baseTextures);
    primary.group.visible = cmp === null;
    oldTextures.dispose();
    for (const p of oldExtras.values()) void p.then((s) => s.dispose());
    if (cmp) void applyCompare();
  };

  let width = 1;
  let height = 1;
  const resize = () => {
    width = container.clientWidth || 1;
    height = container.clientHeight || 1;
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  };
  resize();
  const ro = new ResizeObserver(resize);
  ro.observe(container);

  /** desenha uma região (px CSS) com a camada `layer` visível; o aspecto acompanha a região */
  function renderRegion(x: number, w: number, layer: number) {
    renderer.setViewport(x, 0, w, height);
    renderer.setScissor(x, 0, w, height);
    camera.layers.set(layer);
    camera.aspect = w / height;
    camera.updateProjectionMatrix();
    renderer.render(scene, camera);
  }

  const renderFrame = () => {
    if (!cmp || cmpLooks.length < 2) {
      renderer.setScissorTest(false);
      renderer.setViewport(0, 0, width, height);
      camera.layers.set(0);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      renderer.render(scene, camera);
      return;
    }
    renderer.setScissorTest(true);
    if (cmp.mode === "side") {
      const half = Math.floor(width / 2);
      renderRegion(0, half, 0);
      renderRegion(half, width - half, 1);
    } else {
      // antes/depois: a mesma imagem inteira, cortada pelo divisor
      const cut = Math.round(width * (cmp.pos ?? 0.5));
      renderer.setViewport(0, 0, width, height);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      camera.layers.set(0);
      renderer.setScissor(0, 0, cut, height);
      renderer.render(scene, camera);
      camera.layers.set(1);
      renderer.setScissor(cut, 0, width - cut, height);
      renderer.render(scene, camera);
    }
    renderer.setScissorTest(false);
    camera.layers.set(0);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  };

  let raf = 0;
  const animate = () => {
    raf = requestAnimationFrame(animate);
    controls.update();
    renderFrame();
  };
  animate();

  return {
    setVertexPositions,
    frame,
    setTextureMap,
    setModel,
    setHitboxes,
    setAttachments,
    setCompare,
    setHighlight: (tris) => rig.setHighlight(tris),
    onPick(listener) {
      pickListeners.add(listener);
      return () => pickListeners.delete(listener);
    },
    dispose() {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
      ctlEvents.abort();
      controls.dispose();
      cmpToken++;
      dropCompareLooks();
      primary.dispose();
      rig.dispose();
      baseTextures.dispose();
      pruneExtraSets([]);
      pickListeners.clear();
      overlay.remove();
      views.remove();
      hitLines.geometry.dispose();
      (hitLines.material as THREE.Material).dispose();
      clearAttachments();
      renderer.dispose();
      renderer.forceContextLoss();
      renderer.domElement.remove();
    },
  };
}
