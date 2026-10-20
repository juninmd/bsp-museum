import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import {
  fillStyleTable,
  frameIndex,
  moverPose,
  renderFxFactor,
  routePosition,
  scrollOffset,
  STYLE_SLOTS,
  TEXTURE_FRAME_INTERVAL,
} from "./lib/anim.ts";
import { buildChunks, toWorld, visibleIndices, type Chunk } from "./lib/geometry.ts";
import { inputFromMesh } from "./lib/chunker.ts";
import { Pvs } from "./lib/pvs.ts";
import { Overlays, rgb01, type OverlayKey, type PickInfo, type SoundVisual, type VolumeCategory } from "./viewer-overlays.ts";
import type {
  AmbientSound,
  BrushModelInfo,
  EnvFog,
  HullMesh,
  LightEnvironment,
  Mover,
  RenderProps,
  ScrollInfo,
  TrainRoute,
  Vec3,
  VolumeReport,
} from "./types/bsp-extra.ts";
import type { SpriteInfo } from "./types/formats.ts";
import type { MeshDetail, SkyBox } from "./types.ts";

export interface ViewerStats {
  /** triângulos desenhados no último quadro */
  drawn: number;
  /** triângulos da malha inteira */
  total: number;
  /** folha do BSP em que a câmera está (-1 = sem árvore) */
  leaf: number;
  pvsActive: boolean;
}

/** Camadas que o painel "Camadas" liga e desliga. */
export type LayerName =
  | "renderModes"
  | "scroll"
  | "texAnim"
  | "texAlt"
  | "lightAnim"
  | "movers"
  | "fog"
  | "sun"
  | "sky"
  | "sound3d"
  | OverlayKey;

export interface LayerStat {
  /** quantos itens a camada tem neste mapa (0 = a interface esconde o controle) */
  count: number;
  on: boolean;
}

/** Pose da câmera para o ouvinte do áudio espacial: coordenadas da cena (Y-up), direção unitária. */
export interface ListenerPose {
  pos: [number, number, number];
  forward: [number, number, number];
  up: [number, number, number];
}

export interface Viewer3D {
  setTextured(on: boolean): void;
  setTransparent(on: boolean): void;
  /** liga/desliga a luz pré-calculada (lightmaps) no modo texturizado */
  setLightmaps(on: boolean): void;
  /** liga/desliga o corte por visibilidade (PVS) */
  setPvs(on: boolean): void;
  /** leva a câmera até um ponto (coordenadas do mapa, Z-up) e marca o lugar */
  focusOn(position: [number, number, number]): void;
  enterFirstPerson(): void;
  exitFirstPerson(): void;
  setFullscreen(on: boolean): void;
  onStats(listener: (s: ViewerStats) => void): void;

  /** liga/desliga uma camada do painel "Camadas" */
  setLayer(name: LayerName, on: boolean): void;
  /** quantidade e estado de cada camada */
  layers(): Record<LayerName, LayerStat>;
  /** janela de um hull de colisão (1..3); várias janelas se somam até `clearHull` */
  addHullWindow(hull: number, win: HullMesh): void;
  clearHull(hull: number): void;
  /** mostra/esconde um hull já carregado sem descartar a geometria */
  setHullVisible(hull: number, on: boolean): void;
  setVolumes(report: VolumeReport): void;
  volumeCategories(): VolumeCategory[];
  setVolumeCategory(key: string, on: boolean): void;
  volumesTruncated(): boolean;
  /** `.spr` que as entidades de sprite pedem */
  spriteModels(): string[];
  setSpriteInfo(model: string, info: SpriteInfo | null): void;
  /** `ambient_generic` do mapa, na ordem que numera os sons da camada "Sons 3D" */
  soundSources(): readonly AmbientSound[];
  /** estado visual de cada som (índice = id), aplicado a cada quadro: pulso, alcance, destaque */
  setSoundVisuals(visuals: readonly (SoundVisual | undefined)[]): void;
  /** chamado a cada quadro com a pose da câmera (órbita ou primeira pessoa); `null` remove */
  onFrame(listener: ((pose: ListenerPose, nowMs: number) => void) | null): void;
  dispose(): void;
}

export interface Viewer3DOptions {
  textured: boolean;
  /** chunks já montados (no Web Worker); se ausente, monta aqui mesmo */
  chunks?: Chunk[];
}

/**
 * Lightmap do GoldSrc é "overbright": 128 é neutro, então a luz multiplica por 2.
 * O `MeshBasicMaterial` do Three.js ainda divide o lightmap por π (convenção de
 * irradiância), então o fator precisa de `π` para o texel 128 sair como 1,0.
 */
const LIGHTMAP_INTENSITY = 2 * Math.PI;

function configureTexture(tex: THREE.Texture): void {
  tex.colorSpace = THREE.SRGBColorSpace;
  // No GoldSrc o `t` do texinfo cresce para baixo: v=0 é a primeira linha do bitmap.
  // O padrão do Three.js (flipY) espelharia a textura na vertical.
  tex.flipY = false;
  tex.wrapS = THREE.RepeatWrapping;
  tex.wrapT = THREE.RepeatWrapping;
  tex.minFilter = THREE.LinearMipmapLinearFilter;
}

function buildTextures(mesh: MeshDetail): (THREE.Texture | null)[] {
  return mesh.textures.map((tx) => {
    if (!tx.png) return null;
    const tex = new THREE.TextureLoader().load(tx.png);
    configureTexture(tex);
    return tex;
  });
}

/** Camada do lightmap: valores crus (128 -> 0,5), sem mipmap, na uv1 do atlas. */
function lightmapTexture(png: string): THREE.Texture {
  const tex = new THREE.TextureLoader().load(png);
  tex.colorSpace = THREE.NoColorSpace; // valores já em escala de exibição
  tex.flipY = false; // mesma convenção da textura: linha 0 do atlas em v=0
  tex.channel = 1; // uv1
  tex.minFilter = THREE.LinearFilter;
  tex.magFilter = THREE.LinearFilter;
  tex.generateMipmaps = false;
  return tex;
}

/** Uniforms compartilhados por todos os materiais com luz animada (um objeto, uma atualização). */
interface LightStyleUniforms {
  uStyleVal: { value: Float32Array };
  uLayer1: { value: THREE.Texture };
  uLayer2: { value: THREE.Texture };
  uLayer3: { value: THREE.Texture };
  uAnimOn: { value: number };
}

/**
 * Troca a leitura do lightmap do `MeshBasicMaterial` por
 *   luz = atlas * mult(estilo0) + Σ camada[k] * mult(estilo k)
 * com `mult` vindo de uma tabela `id -> valor` atualizada a 10 Hz. Estilo 0 vale sempre 1,
 * então um chunk só com estilo 0 sai idêntico ao material comum.
 */
function patchLightStyles(material: THREE.MeshBasicMaterial, uniforms: LightStyleUniforms): void {
  material.customProgramCacheKey = () => "bsp-lightstyles";
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, uniforms);
    shader.vertexShader = shader.vertexShader
      .replace("#include <uv_pars_vertex>", "#include <uv_pars_vertex>\nattribute vec4 lmStyles;\nvarying vec4 vLmStyles;")
      .replace("#include <uv_vertex>", "#include <uv_vertex>\n\tvLmStyles = lmStyles;");
    shader.fragmentShader = shader.fragmentShader
      .replace(
        "#include <lightmap_pars_fragment>",
        `#include <lightmap_pars_fragment>
#ifdef USE_LIGHTMAP
varying vec4 vLmStyles;
uniform float uStyleVal[${STYLE_SLOTS}];
uniform sampler2D uLayer1;
uniform sampler2D uLayer2;
uniform sampler2D uLayer3;
uniform float uAnimOn;
float styleMult( float id ) {
	int i = int( id + 0.5 );
	return i < ${STYLE_SLOTS} ? uStyleVal[ i ] : 0.0;
}
vec4 styledLightmap( vec2 uv, vec4 st ) {
	vec3 c = texture2D( lightMap, uv ).rgb * ( st.x > 254.5 ? 1.0 : styleMult( st.x ) );
	if ( uAnimOn > 0.5 ) {
		if ( st.y < 254.5 ) c += texture2D( uLayer1, uv ).rgb * styleMult( st.y );
		if ( st.z < 254.5 ) c += texture2D( uLayer2, uv ).rgb * styleMult( st.z );
		if ( st.w < 254.5 ) c += texture2D( uLayer3, uv ).rgb * styleMult( st.w );
	}
	return vec4( c, 1.0 );
}
#endif`,
      )
      .replace("vec4 lightMapTexel = texture2D( lightMap, vLightMapUv );", "vec4 lightMapTexel = styledLightmap( vLightMapUv, vLmStyles );");
  };
}

/** Grupo de textura animada (`+0nome`…): quadros prontos para trocar no material. */
interface AnimGroup {
  frames: THREE.Texture[];
  alt: THREE.Texture[];
  interval: number;
}

interface Entry {
  mesh: THREE.Mesh;
  chunk: Chunk;
  /** índice dinâmico (PVS): só os triângulos visíveis */
  indices: Uint32Array;
  /** triângulos ligados pelo PVS neste chunk (os que o frustum pode ainda descartar) */
  drawn: number;
  texMaterial: THREE.MeshBasicMaterial;
  /** textura da face (null = só cor) */
  baseMap: THREE.Texture | null;
  /** propriedades de render da brush entity dona (null = geometria comum) */
  render: RenderProps | null;
  /** rolagem das texturas `scroll*` desta brush entity */
  scroll: ScrollInfo | null;
  /** clone da textura com offset próprio (criado quando a imagem carrega) */
  scrollTex: THREE.Texture | null;
  anim: AnimGroup | null;
  /** o chunk tem faces com estilo de luz animado (material com shader de estilos) */
  styled: boolean;
  /** opacidade do modo de render, antes do fator do `renderfx` */
  baseOpacity: number;
}

/** Brush entity: grupo com origem/pivô próprios, para mover e girar sem tocar nos buffers. */
interface ModelNode {
  model: number;
  /** posição do mapa (e do movimento) */
  outer: THREE.Group;
  /** leva o pivô para a origem do grupo */
  holder: THREE.Group;
  /** `origin` do mapa: o modelo é compilado relativo a ele (brush com origin brush) */
  origin: Vec3;
  /** pivô da rotação, no espaço do modelo */
  pivot: Vec3;
  mover: Mover | null;
  route: TrainRoute | null;
}

const ROTATING_KINDS = new Set(["rotating", "door_rotating", "rot_button"]);
const isZero = (v: readonly number[]) => !v[0] && !v[1] && !v[2];

/** Rotação (pitch, yaw, roll) em graus, no espaço do mapa, como quaternion do Three.js. */
function rotationOf(rot: Vec3, out: THREE.Quaternion): THREE.Quaternion {
  const d = Math.PI / 180;
  // eixos do mapa no mundo Y-up: X -> X, Y -> -Z, Z -> Y
  const qz = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), rot[1] * d);
  const qy = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 0, -1), rot[0] * d);
  const qx = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), rot[2] * d);
  return out.copy(qz).multiply(qy).multiply(qx);
}

/**
 * Monta a cena 3D orbitável dentro de `container`.
 * `textured` liga as texturas reais; desligado mostra a cor por altura.
 */
export function mount3D(
  container: HTMLElement,
  mesh: MeshDetail,
  options: boolean | Viewer3DOptions,
): Viewer3D {
  const opts: Viewer3DOptions = typeof options === "boolean" ? { textured: options } : options;
  const chunks = opts.chunks ?? (() => {
    const { input, cellSize } = inputFromMesh(mesh);
    return buildChunks(input, cellSize);
  })();

  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  container.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0d1117);

  const camera = new THREE.PerspectiveCamera(60, 1, 0.5, 100000);
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;

  // Luz ambiente alta + hemisférica: sem isso o chão (mais baixo, cor fria) vira
  // um vulto escuro sobre o fundo escuro e some de vista. Só afeta o modo "cor por altura".
  const ambient = new THREE.AmbientLight(0xffffff, 0.6);
  const hemi = new THREE.HemisphereLight(0xbfd9ff, 0x33373d, 0.6);
  scene.add(ambient, hemi);
  const sun = new THREE.DirectionalLight(0xffffff, 0.9);
  sun.position.set(1, 2, 1.4);
  scene.add(sun);
  const fill = new THREE.DirectionalLight(0x8899bb, 0.25);
  fill.position.set(-1, -0.5, -1);
  scene.add(fill);

  const flatMaterial = new THREE.MeshStandardMaterial({
    vertexColors: true,
    roughness: 0.95,
    metalness: 0,
    flatShading: true,
    side: THREE.DoubleSide,
  });

  const textures = buildTextures(mesh);
  const owned: THREE.Texture[] = [...textures.filter((x): x is THREE.Texture => x !== null)];
  let lightmapTex: THREE.Texture | null = null;
  if (mesh.lightmap && mesh.lm_uvs.length) {
    lightmapTex = lightmapTexture(mesh.lightmap);
    owned.push(lightmapTex);
  }

  // ---- luz animada: camadas dos slots 1..3 + tabela de multiplicadores por estilo
  const styleTable = new Float32Array(STYLE_SLOTS).fill(1);
  let blackTex: THREE.DataTexture | null = null;
  const layerTex = (slot: number): THREE.Texture => {
    const layer = mesh.lightmap_layers.find((l) => l.slot === slot);
    if (layer) {
      const tx = lightmapTexture(layer.png);
      owned.push(tx);
      return tx;
    }
    if (!blackTex) {
      blackTex = new THREE.DataTexture(new Uint8Array([0, 0, 0, 255]), 1, 1);
      blackTex.needsUpdate = true;
      owned.push(blackTex);
    }
    return blackTex;
  };
  const styleUniforms: LightStyleUniforms | null = lightmapTex && chunks.some((c) => c.styles.length)
    ? {
        uStyleVal: { value: styleTable },
        uLayer1: { value: layerTex(1) },
        uLayer2: { value: layerTex(2) },
        uLayer3: { value: layerTex(3) },
        uAnimOn: { value: 1 },
      }
    : null;

  // ---- tabelas por modelo / textura
  const bmByModel = new Map<number, BrushModelInfo>();
  for (const bm of mesh.brush_models) bmByModel.set(bm.model, bm);
  const moverByModel = new Map<number, Mover>();
  for (const e of mesh.scene.entities) if (e.type === "mover" && e.model !== null && !moverByModel.has(e.model)) moverByModel.set(e.model, e);
  const routeByTrain = new Map<number, TrainRoute>();
  for (const r of mesh.scene.routes) routeByTrain.set(r.train, r);

  const animBySlot = new Map<number, AnimGroup>();
  for (const a of mesh.texture_anims) {
    const pick = (slots: number[]) => slots.filter((s) => s >= 0 && textures[s]).map((s) => textures[s]!);
    const group: AnimGroup = { frames: pick(a.frame_slots), alt: pick(a.alt_slots), interval: a.interval > 0 ? a.interval : TEXTURE_FRAME_INTERVAL };
    if (group.frames.length < 2 && group.alt.length < 2) continue;
    for (const s of [...a.frame_slots, ...a.alt_slots]) if (s >= 0) animBySlot.set(s, group);
  }

  const world = new THREE.Group();
  scene.add(world);
  const nodes = new Map<number, ModelNode>();
  const nodeFor = (model: number): ModelNode => {
    let node = nodes.get(model);
    if (node) return node;
    const bm = bmByModel.get(model);
    const mover = moverByModel.get(model) ?? null;
    const origin: Vec3 = bm?.origin ?? [0, 0, 0];
    // girar em torno do centro quando o mapeador esqueceu o origin brush
    let pivot: Vec3 = [0, 0, 0];
    if (bm && mover && ROTATING_KINDS.has(mover.kind) && isZero(origin)) {
      pivot = [(bm.mins[0] + bm.maxs[0]) / 2, (bm.mins[1] + bm.maxs[1]) / 2, (bm.mins[2] + bm.maxs[2]) / 2];
    }
    const outer = new THREE.Group();
    const holder = new THREE.Group();
    outer.add(holder);
    outer.position.set(...toWorld(origin[0] + pivot[0], origin[1] + pivot[1], origin[2] + pivot[2]));
    holder.position.set(...toWorld(-pivot[0], -pivot[1], -pivot[2]));
    scene.add(outer);
    node = { model, outer, holder, origin, pivot, mover, route: mover ? (routeByTrain.get(mover.entity) ?? null) : null };
    nodes.set(model, node);
    return node;
  };

  const entries: Entry[] = [];
  let totalTriangles = 0;
  for (const chunk of chunks) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(chunk.positions, 3));
    geometry.setAttribute("uv", new THREE.BufferAttribute(chunk.uvs, 2));
    if (chunk.lmUvs.length) geometry.setAttribute("uv1", new THREE.BufferAttribute(chunk.lmUvs, 2));
    geometry.setAttribute("color", new THREE.BufferAttribute(chunk.colors, 3));
    const styled = chunk.styles.length > 0 && styleUniforms !== null;
    if (styled) geometry.setAttribute("lmStyles", new THREE.BufferAttribute(chunk.styles, 4));
    const triCount = chunk.triFace.length;
    totalTriangles += triCount;
    const indices = new Uint32Array(triCount * 3);
    visibleIndices(chunk.triFace, null, indices);
    const index = new THREE.BufferAttribute(indices, 1);
    index.setUsage(THREE.DynamicDrawUsage);
    geometry.setIndex(index);
    geometry.boundingSphere = new THREE.Sphere(new THREE.Vector3(...chunk.center), chunk.radius);

    const baseMap = chunk.textureIndex < textures.length ? (textures[chunk.textureIndex] ?? null) : null;
    const texMaterial = new THREE.MeshBasicMaterial({
      color: baseMap ? 0xffffff : 0x8b98a5,
      side: THREE.DoubleSide,
      map: baseMap,
      lightMap: lightmapTex,
      lightMapIntensity: LIGHTMAP_INTENSITY,
    });
    if (styled) patchLightStyles(texMaterial, styleUniforms!);

    const bm = chunk.model > 0 ? bmByModel.get(chunk.model) : undefined;
    const texName = (mesh.textures[chunk.textureIndex]?.name ?? "").toLowerCase();
    const scroll = bm?.scroll && bm.scroll.textures.some((n) => n.toLowerCase() === texName) ? bm.scroll : null;

    const meshObj = new THREE.Mesh(geometry, flatMaterial);
    if (chunk.model > 0) nodeFor(chunk.model).holder.add(meshObj);
    else world.add(meshObj);
    entries.push({
      mesh: meshObj,
      chunk,
      indices,
      drawn: triCount,
      texMaterial,
      baseMap,
      render: bm?.render ?? null,
      scroll,
      scrollTex: null,
      anim: scroll ? null : (animBySlot.get(chunk.textureIndex) ?? null),
      styled,
      baseOpacity: 1,
    });
  }

  // Spawns como esferas: azul CT, laranja T, verde resto.
  const spawnGroup = new THREE.Group();
  for (const spawn of mesh.spawns) {
    const [x, y, z] = toWorld(spawn.position[0], spawn.position[1], spawn.position[2]);
    const color = spawn.team === "CT" ? 0x4c7cf3 : spawn.team === "T" ? 0xf0883e : 0x3fb950;
    const sphere = new THREE.Mesh(
      new THREE.SphereGeometry(spawnRadius(mesh), 10, 10),
      new THREE.MeshBasicMaterial({ color }),
    );
    sphere.position.set(x, y, z);
    spawnGroup.add(sphere);
  }
  scene.add(spawnGroup);

  // Enquadra a câmera no modelo 0.
  const { center, size } = fitTarget(mesh);
  // piso do no-clip: logo abaixo do mapa (o eixo Y do Three.js é o Z do mapa, que pode ser negativo)
  const floorY = center[1] - size[1] / 2 - 64;
  const maxDim = Math.max(size[0], size[1], size[2], 1);
  const dist = (maxDim / 2 / Math.tan((camera.fov * Math.PI) / 360)) * 1.4;
  camera.position.set(center[0] + dist * 0.8, center[1] + dist * 0.7, center[2] + dist);
  camera.near = Math.max(0.5, dist / 1000);
  camera.far = Math.max(10000, dist * 20);
  camera.updateProjectionMatrix();
  controls.target.set(center[0], center[1], center[2]);
  controls.update();
  controls.minDistance = Math.max(4, dist / 8);
  controls.maxDistance = dist * 6;

  // Céu: a caixa real do mapa (gfx/env) se existir; senão um domo gradiente.
  const sky = loadSky(center, maxDim, mesh.skybox);
  scene.add(sky);

  // Camadas por cima do mapa (hulls, volumes, luzes, sprites, sons, decalques, rotas).
  const markerUnit = Math.max(4, Math.min(16, maxDim / 400));
  const overlays = new Overlays(mesh, markerUnit);
  scene.add(overlays.group);

  // Marcador de "você está aqui" para o foco em entidade.
  const marker = new THREE.Group();
  const ring = new THREE.Mesh(
    new THREE.TorusGeometry(1, 0.08, 8, 40),
    new THREE.MeshBasicMaterial({ color: 0xf2c14e, depthTest: false, transparent: true }),
  );
  ring.rotation.x = Math.PI / 2;
  const pin = new THREE.Mesh(
    new THREE.ConeGeometry(0.35, 1.4, 12),
    new THREE.MeshBasicMaterial({ color: 0xf2c14e, depthTest: false, transparent: true }),
  );
  pin.rotation.x = Math.PI;
  pin.position.y = 1.6;
  marker.add(ring, pin);
  marker.visible = false;
  marker.renderOrder = 10;
  scene.add(marker);

  // -------------------------------------------------------------------------
  // PVS: só desenha o que as folhas visíveis a partir da câmera enxergam.
  const pvs = mesh.pvs ? new Pvs(mesh.pvs) : null;
  let pvsEnabled = pvs !== null;
  let lastLeaf = -2;
  let currentLeaf = -1;
  let pvsActive = false;

  const applyVisibility = (flags: Uint8Array | null) => {
    const predicate = pvs?.predicate(flags) ?? null;
    for (const e of entries) {
      const n = visibleIndices(e.chunk.triFace, predicate, e.indices);
      const geo = e.mesh.geometry;
      geo.setDrawRange(0, n);
      geo.index!.needsUpdate = true;
      e.drawn = n / 3;
      e.mesh.visible = n > 0;
    }
    pvsActive = flags !== null;
  };

  const refreshPvs = () => {
    if (!pvs) return;
    // câmera (Three, Y-up) -> mapa (GoldSrc, Z-up): (x, y, z) = (cx, -cz, cy)
    const leaf = pvsEnabled ? pvs.leafAt(camera.position.x, -camera.position.z, camera.position.y) : -1;
    currentLeaf = leaf;
    if (leaf === lastLeaf) return;
    lastLeaf = leaf;
    applyVisibility(pvsEnabled && leaf >= 0 ? pvs.facesVisibleFrom(leaf) : null);
  };

  // -------------------------------------------------------------------------
  // Estado do modo "primeira pessoa" (no-clip): mouse para olhar, WASD/fly.
  let mode: "orbit" | "fps" = "orbit";
  let yaw = 0;
  let pitch = 0;
  const fpKeys = new Set<string>();

  const sens = 0.0022;
  const clampPitch = (v: number) => Math.max(-1.55, Math.min(1.55, v));

  function onPointerLockChange() {
    const locked = document.pointerLockElement === renderer.domElement;
    if (!locked && mode === "fps") {
      mode = "orbit";
      controls.enabled = true;
      controls.target.set(center[0], center[1], center[2]);
      controls.update();
    }
  }

  function onMouseMove(e: MouseEvent) {
    if (mode !== "fps" || document.pointerLockElement !== renderer.domElement) return;
    yaw -= e.movementX * sens;
    pitch = clampPitch(pitch - e.movementY * sens);
  }

  const FPS_KEYS = new Set(["KeyW", "KeyA", "KeyS", "KeyD", "KeyC", "Space", "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight"]);
  function onKeyDown(e: KeyboardEvent) {
    if (mode !== "fps") return;
    // Space/Ctrl não podem acionar o botão focado nem rolar a página
    if (FPS_KEYS.has(e.code)) e.preventDefault();
    fpKeys.add(e.code);
  }
  function onKeyUp(e: KeyboardEvent) {
    fpKeys.delete(e.code);
  }

  document.addEventListener("pointerlockchange", onPointerLockChange);
  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("keydown", onKeyDown);
  document.addEventListener("keyup", onKeyUp);

  const clock = new THREE.Clock();
  const tmpFwd = new THREE.Vector3();
  const tmpRight = new THREE.Vector3();
  const tmpMove = new THREE.Vector3();

  function stepFps(dt: number) {
    const speed = 340;
    const fast = fpKeys.has("ShiftLeft") || fpKeys.has("ShiftRight");
    const v = speed * (fast ? 3.2 : 1) * dt;
    // Direção de visão (sem rolagem de câmera).
    tmpFwd.set(-Math.sin(yaw) * Math.cos(pitch), Math.sin(pitch), -Math.cos(yaw) * Math.cos(pitch));
    tmpRight.set(Math.cos(yaw), 0, -Math.sin(yaw));

    tmpMove.set(0, 0, 0);
    if (fpKeys.has("KeyW")) tmpMove.add(tmpFwd);
    if (fpKeys.has("KeyS")) tmpMove.sub(tmpFwd);
    if (fpKeys.has("KeyD")) tmpMove.add(tmpRight);
    if (fpKeys.has("KeyA")) tmpMove.sub(tmpRight);
    if (tmpMove.lengthSq() > 0) tmpMove.normalize().multiplyScalar(v);
    if (fpKeys.has("Space")) tmpMove.y += v;
    if (fpKeys.has("ControlLeft") || fpKeys.has("ControlRight") || fpKeys.has("KeyC")) tmpMove.y -= v;

    camera.position.add(tmpMove);
    if (camera.position.y < floorY) camera.position.y = floorY;
    camera.rotation.order = "YXZ";
    camera.rotation.set(pitch, yaw, 0);
  }

  function enterFirstPerson() {
    if (mode === "fps") return;
    mode = "fps";
    controls.enabled = false;
    hideTip();
    (document.activeElement as HTMLElement | null)?.blur();
    // Inicia em um ponto de spawn (ou no centro) sem colisão — no-clip.
    const spawn = mesh.spawns[0];
    const start: [number, number, number] = spawn
      ? toWorld(spawn.position[0]!, spawn.position[1]!, spawn.position[2]!)
      : [center[0], center[1], center[2]];
    camera.position.set(start[0], start[1], start[2]);
    camera.rotation.order = "YXZ";
    camera.lookAt(center[0], center[1], center[2]);
    yaw = camera.rotation.y;
    pitch = camera.rotation.x;
    renderer.domElement.requestPointerLock();
  }

  function exitFirstPerson() {
    if (mode !== "fps") return;
    mode = "orbit";
    fpKeys.clear();
    if (document.pointerLockElement === renderer.domElement) document.exitPointerLock();
    controls.enabled = true;
    controls.target.set(center[0], center[1], center[2]);
    controls.update();
  }

  function setFullscreen(on: boolean) {
    if (on) {
      container.requestFullscreen?.();
    } else if (document.fullscreenElement) {
      document.exitFullscreen?.();
    }
  }

  function spawnRadius(m: MeshDetail): number {
    const s: [number, number, number] = m.bounds ? m.bounds.size : [256, 256, 128];
    return Math.max(2, Math.min(s[0], s[1], s[2]) / 40);
  }

  function focusOn(position: [number, number, number]) {
    const [x, y, z] = toWorld(position[0], position[1], position[2]);
    // distância de observação: perto o bastante para ver o entorno, longe o bastante para dar contexto
    const reach = Math.min(700, Math.max(200, maxDim / 10));
    if (mode === "fps") exitFirstPerson();
    controls.target.set(x, y, z);
    camera.position.set(x + reach * 0.9, y + reach * 0.7, z + reach * 0.9);
    controls.update();
    marker.position.set(x, y, z);
    marker.scale.setScalar(Math.max(8, reach / 8));
    marker.visible = true;
    lastLeaf = -2; // reavalia o PVS a partir da nova posição
  }

  const resize = () => {
    const w = container.clientWidth || 1;
    const h = container.clientHeight || 1;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  };
  resize();
  const ro = new ResizeObserver(resize);
  ro.observe(container);

  // Triângulos que realmente vão para a GPU: ligados pelo PVS e dentro do frustum.
  const frustum = new THREE.Frustum();
  const projView = new THREE.Matrix4();
  const tmpSphere = new THREE.Sphere();
  const countDrawn = (): number => {
    camera.updateMatrixWorld();
    projView.multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
    frustum.setFromProjectionMatrix(projView);
    let n = 0;
    for (const e of entries) {
      if (!e.mesh.visible) continue;
      // brush entity se move: a esfera precisa da matriz do mundo
      const bs = e.chunk.model > 0 ? tmpSphere.copy(e.mesh.geometry.boundingSphere!).applyMatrix4(e.mesh.matrixWorld) : e.mesh.geometry.boundingSphere!;
      if (frustum.intersectsSphere(bs)) n += e.drawn;
    }
    return n;
  };

  // -------------------------------------------------------------------------
  // Estado das camadas e dos materiais.
  let texturedOn = opts.textured;
  let lightmapsOn = true;
  let transparentOn = false;
  const layerOn: Record<LayerName, boolean> = {
    renderModes: true,
    scroll: true,
    texAnim: true,
    texAlt: false,
    lightAnim: true,
    movers: false,
    fog: false,
    sun: false,
    sky: true,
    lights: false,
    sprites: false,
    sounds: false,
    sound3d: false,
    decals: false,
    routes: false,
    volumes: false,
  };
  /** estilos ligáveis apagados (começam os `start_off`; clicar na luz alterna) */
  const lightsOff = new Set<number>(mesh.lightstyles.filter((s) => s.switchable && s.start_off).map((s) => s.id));

  /** Aplica modo de render, transparência global, luz e textura ao material do chunk. */
  const configure = (e: Entry) => {
    const mat = e.texMaterial;
    const rp = layerOn.renderModes ? e.render : null;
    const rmode = rp?.mode ?? 0;
    mat.map = e.scrollTex ?? e.baseMap;
    mat.color.set(e.baseMap ? 0xffffff : 0x8b98a5);
    mat.transparent = transparentOn;
    mat.depthWrite = !transparentOn;
    mat.alphaTest = 0;
    mat.blending = THREE.NormalBlending;
    e.baseOpacity = 1;
    let lit = true;
    if (rp && rmode !== 0) {
      const alpha = Math.max(0, Math.min(1, rp.amount / 255));
      switch (rmode) {
        case 1: // color: cor do rendercolor, com translucidez
          mat.map = null;
          mat.color.copy(rgb01(rp.color));
          mat.transparent = true;
          mat.depthWrite = false;
          e.baseOpacity = alpha;
          lit = false;
          break;
        case 2: // texture / 3 glow (para brush vale como texture)
        case 3:
          mat.transparent = true;
          mat.depthWrite = false;
          e.baseOpacity = alpha;
          break;
        case 4: // solid: teste de alpha (texturas `{`)
          mat.alphaTest = 0.5;
          mat.transparent = false;
          mat.depthWrite = true;
          break;
        case 5: // additive
          mat.blending = THREE.AdditiveBlending;
          mat.transparent = true;
          mat.depthWrite = false;
          e.baseOpacity = alpha;
          break;
      }
    }
    mat.opacity = e.baseOpacity;
    mat.lightMap = lightmapsOn && lit ? lightmapTex : null;
    mat.needsUpdate = true;
    e.mesh.renderOrder = rp && rmode !== 0 && mat.transparent ? 3 : 0;
  };
  const refreshMaterials = () => {
    for (const e of entries) {
      e.mesh.material = texturedOn ? e.texMaterial : flatMaterial;
      configure(e);
    }
    forceTick();
  };

  // ---- relógio das animações
  let animTime = 0;
  let moverTime = 0;
  let lastTick = -1;
  let lastStyleTick = -1;
  const forceTick = () => {
    lastTick = -1;
    lastStyleTick = -1;
  };

  const FX_ANIMATED = new Set([1, 2, 3, 4, 9, 10, 11, 12, 13]);
  const fxEntries = entries.filter((e) => e.render && FX_ANIMATED.has(e.render.fx));
  const scrollEntries = entries.filter((e) => e.scroll);
  const animEntries = entries.filter((e) => e.anim);
  const styleDefs = mesh.lightstyles;

  const resetScroll = () => {
    for (const e of scrollEntries) if (e.scrollTex) e.scrollTex.offset.x = 0;
  };

  const tickScroll = () => {
    for (const e of scrollEntries) {
      if (!e.scrollTex) {
        // o clone só nasce quando a imagem existe: o Three.js avisaria de textura sem dados
        if (!e.baseMap?.image) continue;
        e.scrollTex = e.baseMap.clone();
        owned.push(e.scrollTex);
        if (e.render?.mode !== 1) e.texMaterial.map = e.scrollTex;
      }
      const img = e.baseMap?.image as { width?: number } | undefined;
      e.scrollTex.offset.x = scrollOffset(e.scroll!.speed, animTime, img?.width ?? 64);
    }
  };

  const tickTextures = () => {
    const tickNow = Math.floor(animTime / TEXTURE_FRAME_INTERVAL);
    if (tickNow === lastTick) return;
    lastTick = tickNow;
    for (const e of animEntries) {
      if (e.render?.mode === 1 && layerOn.renderModes) continue;
      const g = e.anim!;
      const seq = layerOn.texAlt && g.alt.length > 1 ? g.alt : g.frames;
      e.texMaterial.map = layerOn.texAnim && seq.length > 1 ? seq[frameIndex(seq.length, animTime, g.interval)]! : e.baseMap;
    }
  };

  const tickStyles = () => {
    const tickNow = Math.floor(animTime * 10);
    if (tickNow === lastStyleTick) return;
    lastStyleTick = tickNow;
    fillStyleTable(styleTable, styleDefs, animTime, layerOn.lightAnim, lightsOff);
    if (styleUniforms) styleUniforms.uAnimOn.value = layerOn.lightAnim ? 1 : 0;
  };

  const tickFx = () => {
    if (!layerOn.renderModes) return;
    for (const e of fxEntries) {
      const m = e.render!.mode;
      if (m === 0 || m === 4) continue;
      e.texMaterial.opacity = e.baseOpacity * renderFxFactor(e.render!.fx, animTime);
    }
  };

  // ---- movers
  const tmpQuat = new THREE.Quaternion();
  const placeNode = (n: ModelNode, offset: Vec3, rot: Vec3) => {
    n.outer.position.set(...toWorld(n.origin[0] + n.pivot[0] + offset[0], n.origin[1] + n.pivot[1] + offset[1], n.origin[2] + n.pivot[2] + offset[2]));
    n.outer.quaternion.copy(rotationOf(rot, tmpQuat));
  };
  const resetMovers = () => {
    for (const n of nodes.values()) placeNode(n, [0, 0, 0], [0, 0, 0]);
  };
  const tickMovers = () => {
    for (const n of nodes.values()) {
      const m = n.mover;
      if (!m) continue;
      if ((m.kind === "train" || m.kind === "track_train") && n.route && n.route.points.length > 0) {
        // o centro da caixa do trem passa pelos pontos; o modelo está no espaço local dele
        const c: Vec3 = [(m.mins[0] + m.maxs[0]) / 2, (m.mins[1] + m.maxs[1]) / 2, (m.mins[2] + m.maxs[2]) / 2];
        const p = routePosition(n.route, m.speed, moverTime);
        // posição absoluta = ponto - centro; como placeNode soma a origem do mapa, desconta ela
        placeNode(n, [p[0] - c[0] - n.origin[0], p[1] - c[1] - n.origin[1], p[2] - c[2] - n.origin[2]], [0, 0, 0]);
      } else {
        const pose = moverPose(m, moverTime);
        placeNode(n, pose.offset, pose.rot);
      }
    }
  };
  const moverCount = [...nodes.values()].filter((n) => n.mover).length;

  // ---- névoa, sol
  const fogEnt = mesh.scene.entities.find((e): e is EnvFog => e.type === "env_fog");
  const sunEnt = mesh.scene.entities.find((e): e is LightEnvironment => e.type === "light_environment");
  const bumpMaterials = () => {
    for (const e of entries) e.texMaterial.needsUpdate = true;
    flatMaterial.needsUpdate = true;
  };
  const setFog = (on: boolean) => {
    if (!fogEnt) return;
    scene.fog = on ? new THREE.Fog(rgb01(fogEnt.color), fogEnt.start, Math.max(fogEnt.end, fogEnt.start + 1)) : null;
    bumpMaterials();
  };
  let sunArrow: THREE.ArrowHelper | null = null;
  const setSun = (on: boolean) => {
    if (!sunEnt) return;
    const d = toWorld(sunEnt.direction[0], sunEnt.direction[1], sunEnt.direction[2]);
    const dir = new THREE.Vector3(...d).normalize();
    if (on) {
      sun.color.copy(rgb01(sunEnt.color));
      sun.position.copy(dir).multiplyScalar(-1); // a luz vem do lado oposto ao que ela viaja
      sun.intensity = 1.6;
      ambient.intensity = 0.25;
      hemi.intensity = 0.25;
      fill.intensity = 0.05;
      if (!sunArrow) {
        const len = Math.max(40, maxDim / 6);
        sunArrow = new THREE.ArrowHelper(dir, new THREE.Vector3(center[0], center[1] + size[1] / 2 + len, center[2]).addScaledVector(dir, -len), len, sun.color.getHex(), len * 0.3, len * 0.16);
        scene.add(sunArrow);
      }
    } else {
      sun.color.set(0xffffff);
      sun.position.set(1, 2, 1.4);
      sun.intensity = 0.9;
      ambient.intensity = 0.6;
      hemi.intensity = 0.6;
      fill.intensity = 0.25;
      if (sunArrow) {
        scene.remove(sunArrow);
        sunArrow.dispose();
        sunArrow = null;
      }
    }
  };

  // ---- balão de informação (hover/clique nos marcadores)
  const tip = document.createElement("div");
  tip.className = "v3-tip";
  tip.hidden = true;
  container.appendChild(tip);
  const showTip = (info: PickInfo, x: number, y: number, pinned: boolean) => {
    tip.replaceChildren();
    const h = document.createElement("strong");
    h.textContent = info.title;
    tip.append(h);
    for (const line of info.lines) {
      const p = document.createElement("div");
      p.textContent = line;
      tip.append(p);
    }
    tip.classList.toggle("pinned", pinned);
    tip.hidden = false;
    const box = container.getBoundingClientRect();
    const left = Math.min(Math.max(4, x - box.left + 14), Math.max(4, box.width - tip.offsetWidth - 4));
    const top = Math.min(Math.max(4, y - box.top + 14), Math.max(4, box.height - tip.offsetHeight - 4));
    tip.style.left = `${left}px`;
    tip.style.top = `${top}px`;
  };
  function hideTip() {
    tip.hidden = true;
  }

  const raycaster = new THREE.Raycaster();
  const ndc = new THREE.Vector2();
  let pointer: { x: number; y: number } | null = null;
  let pointerDirty = false;
  let down: { x: number; y: number; t: number } | null = null;
  let pinned = false;
  const pointOf = (ev: PointerEvent) => {
    const r = renderer.domElement.getBoundingClientRect();
    ndc.set(((ev.clientX - r.left) / r.width) * 2 - 1, -((ev.clientY - r.top) / r.height) * 2 + 1);
    raycaster.setFromCamera(ndc, camera);
  };
  const anyPickable = () => layerOn.lights || layerOn.sprites || layerOn.sounds || layerOn.sound3d || layerOn.decals || layerOn.volumes;
  const onPointerMove = (ev: PointerEvent) => {
    if (mode === "fps" || ev.buttons !== 0) {
      if (!pinned) hideTip();
      return;
    }
    pointer = { x: ev.clientX, y: ev.clientY };
    pointerDirty = true;
  };
  const onPointerLeave = () => {
    pointer = null;
    pointerDirty = false;
    if (!pinned) hideTip();
  };
  const onPointerDown = (ev: PointerEvent) => {
    down = { x: ev.clientX, y: ev.clientY, t: performance.now() };
  };
  const onPointerUp = (ev: PointerEvent) => {
    const d = down;
    down = null;
    if (!d || mode === "fps" || !anyPickable()) return;
    if (Math.hypot(ev.clientX - d.x, ev.clientY - d.y) > 5 || performance.now() - d.t > 600) return; // arrasto = órbita
    pointOf(ev);
    const hit = overlays.pick(raycaster);
    pinned = false;
    overlays.showRadius(null);
    if (!hit) {
      hideTip();
      return;
    }
    if (hit.kind === "light" && hit.light?.switchable) {
      const id = hit.light.style;
      if (lightsOff.has(id)) lightsOff.delete(id);
      else lightsOff.add(id);
      forceTick();
      overlays.invalidateLights();
    }
    if (hit.kind === "ambient" && hit.ambient) {
      overlays.showRadius(hit.ambient);
      pinned = true;
    }
    showTip(hit.info, ev.clientX, ev.clientY, pinned);
  };
  renderer.domElement.addEventListener("pointermove", onPointerMove);
  renderer.domElement.addEventListener("pointerleave", onPointerLeave);
  renderer.domElement.addEventListener("pointerdown", onPointerDown);
  renderer.domElement.addEventListener("pointerup", onPointerUp);

  let statsListener: ((s: ViewerStats) => void) | null = null;
  let lastStats = 0;

  let raf = 0;
  let soundVisuals: readonly (SoundVisual | undefined)[] = [];
  let frameListener: ((pose: ListenerPose, nowMs: number) => void) | null = null;
  const listenFwd = new THREE.Vector3();
  const listenUp = new THREE.Vector3();
  const animate = () => {
    raf = requestAnimationFrame(animate);
    const dt = Math.min(clock.getDelta(), 0.1);
    animTime += dt;
    if (mode === "fps") {
      stepFps(dt);
    } else {
      controls.update();
    }
    if (marker.visible) {
      ring.rotation.z += dt * 1.5;
      pin.position.y = 1.6 + Math.sin(performance.now() / 300) * 0.25;
    }
    if (scrollEntries.length && layerOn.scroll && texturedOn) tickScroll();
    if (animEntries.length && texturedOn) tickTextures();
    if (styleUniforms) tickStyles();
    if (fxEntries.length && texturedOn) tickFx();
    if (layerOn.movers && nodes.size) {
      moverTime += dt;
      tickMovers();
    }
    overlays.update(animTime, styleDefs, layerOn.lightAnim, lightsOff);
    if (layerOn.sound3d) overlays.updateSoundVisuals(soundVisuals, animTime);
    if (frameListener) {
      camera.updateMatrixWorld();
      camera.getWorldDirection(listenFwd);
      listenUp.set(0, 1, 0).applyQuaternion(camera.quaternion);
      frameListener(
        {
          pos: [camera.position.x, camera.position.y, camera.position.z],
          forward: [listenFwd.x, listenFwd.y, listenFwd.z],
          up: [listenUp.x, listenUp.y, listenUp.z],
        },
        performance.now(),
      );
    }
    if (pointerDirty && pointer && !pinned && anyPickable()) {
      pointerDirty = false;
      const r = renderer.domElement.getBoundingClientRect();
      ndc.set(((pointer.x - r.left) / r.width) * 2 - 1, -((pointer.y - r.top) / r.height) * 2 + 1);
      raycaster.setFromCamera(ndc, camera);
      const hit = overlays.pick(raycaster);
      if (hit) showTip(hit.info, pointer.x, pointer.y, false);
      else hideTip();
    }
    refreshPvs();
    renderer.render(scene, camera);
    const now = performance.now();
    if (statsListener && now - lastStats > 400) {
      lastStats = now;
      statsListener({ drawn: countDrawn(), total: totalTriangles, leaf: currentLeaf, pvsActive });
    }
  };
  refreshMaterials();
  animate();

  const hasLightAnim = styleUniforms !== null;
  const stat = (count: number, name: LayerName): LayerStat => ({ count, on: layerOn[name] });
  const distinct = <T,>(xs: Iterable<T>) => new Set(xs).size;

  return {
    setTextured(on) {
      texturedOn = on;
      refreshMaterials();
    },
    setTransparent(on) {
      transparentOn = on;
      refreshMaterials();
    },
    setLightmaps(on) {
      lightmapsOn = on;
      refreshMaterials();
    },
    setPvs(on) {
      pvsEnabled = on && pvs !== null;
      lastLeaf = -2;
    },
    focusOn,
    enterFirstPerson,
    exitFirstPerson,
    setFullscreen,
    onStats(listener) {
      statsListener = listener;
    },

    setLayer(name, on) {
      layerOn[name] = on;
      switch (name) {
        case "renderModes":
        case "texAnim":
        case "texAlt":
          refreshMaterials();
          break;
        case "scroll":
          if (!on) resetScroll();
          break;
        case "lightAnim":
          forceTick();
          break;
        case "movers":
          if (!on) {
            moverTime = 0;
            resetMovers();
          }
          break;
        case "fog":
          setFog(on);
          break;
        case "sun":
          setSun(on);
          break;
        case "sky":
          sky.visible = on;
          break;
        case "sound3d":
          overlays.setSoundVisuals(on);
          if (!on && !anyPickable()) {
            hideTip();
            pinned = false;
          }
          break;
        default:
          overlays.setVisible(name, on);
          if (!on && !anyPickable()) {
            hideTip();
            pinned = false;
          }
      }
    },
    layers() {
      return {
        renderModes: stat(distinct(entries.filter((e) => e.render).map((e) => e.chunk.model)), "renderModes"),
        scroll: stat(distinct(scrollEntries.map((e) => e.chunk.model)), "scroll"),
        texAnim: stat(distinct(animEntries.map((e) => e.anim)), "texAnim"),
        texAlt: stat(distinct(animEntries.filter((e) => e.anim!.alt.length > 1).map((e) => e.anim)), "texAlt"),
        lightAnim: stat(hasLightAnim ? mesh.lightstyles.filter((s) => s.id !== 0).length : 0, "lightAnim"),
        movers: stat(moverCount, "movers"),
        fog: stat(fogEnt ? 1 : 0, "fog"),
        sun: stat(sunEnt ? 1 : 0, "sun"),
        sky: stat(1, "sky"),
        lights: stat(overlays.counts.lights, "lights"),
        sprites: stat(overlays.counts.sprites, "sprites"),
        sounds: stat(overlays.counts.sounds, "sounds"),
        sound3d: stat(overlays.ambientSources().length, "sound3d"),
        decals: stat(overlays.counts.decals, "decals"),
        routes: stat(overlays.counts.routes, "routes"),
        volumes: stat(overlays.counts.volumes, "volumes"),
      };
    },
    addHullWindow: (hull, win) => overlays.addHullWindow(hull, win),
    clearHull: (hull) => overlays.clearHull(hull),
    setHullVisible: (hull, on) => overlays.setHullVisible(hull, on),
    setVolumes: (report) => overlays.setVolumes(report),
    volumeCategories: () => overlays.volumeCategories(),
    setVolumeCategory: (key, on) => overlays.setVolumeCategory(key, on),
    volumesTruncated: () => overlays.volumesWereTruncated,
    spriteModels: () => overlays.spriteModels(),
    setSpriteInfo: (model, info) => overlays.setSpriteInfo(model, info),
    soundSources: () => overlays.ambientSources(),
    setSoundVisuals(visuals) {
      soundVisuals = visuals;
    },
    onFrame(listener) {
      frameListener = listener;
    },

    dispose() {
      cancelAnimationFrame(raf);
      frameListener = null;
      soundVisuals = [];
      ro.disconnect();
      document.removeEventListener("pointerlockchange", onPointerLockChange);
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("keyup", onKeyUp);
      renderer.domElement.removeEventListener("pointermove", onPointerMove);
      renderer.domElement.removeEventListener("pointerleave", onPointerLeave);
      renderer.domElement.removeEventListener("pointerdown", onPointerDown);
      renderer.domElement.removeEventListener("pointerup", onPointerUp);
      tip.remove();
      if (document.pointerLockElement === renderer.domElement) document.exitPointerLock();
      controls.dispose();
      for (const e of entries) {
        e.mesh.removeFromParent();
        e.mesh.geometry.dispose();
        e.texMaterial.dispose();
      }
      for (const n of nodes.values()) n.outer.removeFromParent();
      for (const tx of owned) tx.dispose();
      owned.length = 0;
      overlays.dispose();
      if (sunArrow) {
        scene.remove(sunArrow);
        sunArrow.dispose();
        sunArrow = null;
      }
      flatMaterial.dispose();
      scene.remove(sky);
      disposeSky(sky);
      for (const s of spawnGroup.children) {
        (s as THREE.Mesh).geometry.dispose();
        ((s as THREE.Mesh).material as THREE.Material).dispose();
      }
      ring.geometry.dispose();
      pin.geometry.dispose();
      (ring.material as THREE.Material).dispose();
      (pin.material as THREE.Material).dispose();
      renderer.dispose();
      // devolve o contexto WebGL já: abrir e fechar a vista várias vezes não pode esgotar os contextos do navegador
      renderer.forceContextLoss();
      renderer.domElement.remove();
    },
  };
}

function fitTarget(mesh: MeshDetail): { center: [number, number, number]; size: [number, number, number] } {
  const b = mesh.bounds;
  if (!b) return { center: [0, 0, 0], size: [100, 100, 100] };
  const a = toWorld(b.mins[0], b.mins[1], b.mins[2]);
  const c = toWorld(b.maxs[0], b.maxs[1], b.maxs[2]);
  const minX = Math.min(a[0], c[0]);
  const maxX = Math.max(a[0], c[0]);
  const minY = Math.min(a[1], c[1]);
  const maxY = Math.max(a[1], c[1]);
  const minZ = Math.min(a[2], c[2]);
  const maxZ = Math.max(a[2], c[2]);
  return {
    center: [(minX + maxX) / 2, (minY + maxY) / 2, (minZ + maxZ) / 2],
    size: [maxX - minX, maxY - minY, maxZ - minZ],
  };
}

/** Céu do mapa: a caixa `gfx/env` de 6 lados quando existe, senão domo gradiente. */
function loadSky(
  center: [number, number, number],
  maxDim: number,
  skybox: SkyBox | null,
): THREE.Mesh {
  const radius = Math.max(maxDim * 12, 600);
  if (skybox) {
    // Ordem do BoxGeometry: [+x direita, -x esquerda, +y cima, -y baixo, +z frente, -z trás]
    const urls = [skybox.right, skybox.left, skybox.up, skybox.down, skybox.front, skybox.back];
    const materials = urls.map(
      (url) =>
        new THREE.MeshBasicMaterial({
          map: skyTexture(url),
          side: THREE.BackSide,
          depthWrite: false,
          fog: false,
        }),
    );
    const cube = new THREE.Mesh(new THREE.BoxGeometry(radius * 2, radius * 2, radius * 2), materials);
    cube.position.set(center[0], center[1], center[2]);
    cube.renderOrder = -1;
    cube.frustumCulled = false;
    cube.name = "skybox";
    return cube;
  }
  return makeSkyDome(center, radius);
}

function skyTexture(url: string): THREE.Texture {
  const tex = new THREE.TextureLoader().load(url);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

/** Domo celeste de reserva: esfera gigante com um gradiente vertical, vista por dentro. */
function makeSkyDome(center: [number, number, number], radius: number): THREE.Mesh {
  const canvas = document.createElement("canvas");
  canvas.width = 2;
  canvas.height = 256;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    const grad = ctx.createLinearGradient(0, 0, 0, 256);
    grad.addColorStop(0, "#3a6ea5");
    grad.addColorStop(0.55, "#9cc4e8");
    grad.addColorStop(1, "#e3edf6");
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, 2, 256);
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  const material = new THREE.MeshBasicMaterial({
    map: tex,
    side: THREE.BackSide,
    depthWrite: false,
    fog: false,
  });
  const dome = new THREE.Mesh(new THREE.SphereGeometry(radius, 32, 16), material);
  dome.position.set(center[0], center[1], center[2]);
  dome.renderOrder = -1;
  dome.frustumCulled = false;
  dome.name = "sky-dome";
  return dome;
}

/** Libera geometria/materiais/texturas do céu (caixa ou domo). */
function disposeSky(obj: THREE.Mesh): void {
  obj.geometry.dispose();
  const mats = Array.isArray(obj.material) ? obj.material : [obj.material];
  for (const m of mats) {
    const mat = m as THREE.MeshBasicMaterial;
    mat.map?.dispose();
    mat.dispose();
  }
}
