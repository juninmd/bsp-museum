import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { buildChunks, toWorld, visibleIndices, type Chunk } from "./lib/geometry.ts";
import { inputFromMesh } from "./lib/chunker.ts";
import { Pvs } from "./lib/pvs.ts";
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

function buildTextures(mesh: MeshDetail): (THREE.Texture | null)[] {
  return mesh.textures.map((t) => {
    if (!t.png) return null;
    const tex = new THREE.TextureLoader().load(t.png);
    tex.colorSpace = THREE.SRGBColorSpace;
    // No GoldSrc o `t` do texinfo cresce para baixo: v=0 é a primeira linha do bitmap.
    // O padrão do Three.js (flipY) espelharia a textura na vertical.
    tex.flipY = false;
    tex.wrapS = THREE.RepeatWrapping;
    tex.wrapT = THREE.RepeatWrapping;
    tex.minFilter = THREE.LinearMipmapLinearFilter;
    return tex;
  });
}

interface Entry {
  mesh: THREE.Mesh;
  chunk: Chunk;
  /** índice dinâmico (PVS): só os triângulos visíveis */
  indices: Uint32Array;
  /** triângulos ligados pelo PVS neste chunk (os que o frustum pode ainda descartar) */
  drawn: number;
  texMaterial: THREE.MeshBasicMaterial;
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
  scene.add(new THREE.AmbientLight(0xffffff, 0.6));
  scene.add(new THREE.HemisphereLight(0xbfd9ff, 0x33373d, 0.6));
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
  let lightmapTex: THREE.Texture | null = null;
  if (mesh.lightmap && mesh.lm_uvs.length) {
    lightmapTex = new THREE.TextureLoader().load(mesh.lightmap);
    // valores já em escala de exibição: lidos crus (128 -> 0,5), não decodificados como sRGB
    lightmapTex.colorSpace = THREE.NoColorSpace;
    lightmapTex.flipY = false; // mesma convenção da textura: linha 0 do atlas em v=0
    lightmapTex.channel = 1; // uv1
    lightmapTex.minFilter = THREE.LinearFilter;
    lightmapTex.magFilter = THREE.LinearFilter;
    lightmapTex.generateMipmaps = false;
  }

  const entries: Entry[] = [];
  let totalTriangles = 0;
  for (const chunk of chunks) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(chunk.positions, 3));
    geometry.setAttribute("uv", new THREE.BufferAttribute(chunk.uvs, 2));
    if (chunk.lmUvs.length) geometry.setAttribute("uv1", new THREE.BufferAttribute(chunk.lmUvs, 2));
    geometry.setAttribute("color", new THREE.BufferAttribute(chunk.colors, 3));
    const triCount = chunk.triFace.length;
    totalTriangles += triCount;
    const indices = new Uint32Array(triCount * 3);
    visibleIndices(chunk.triFace, null, indices);
    const index = new THREE.BufferAttribute(indices, 1);
    index.setUsage(THREE.DynamicDrawUsage);
    geometry.setIndex(index);
    geometry.boundingSphere = new THREE.Sphere(new THREE.Vector3(...chunk.center), chunk.radius);

    const tex = chunk.textureIndex < textures.length ? textures[chunk.textureIndex] : undefined;
    const texMaterial = new THREE.MeshBasicMaterial({
      color: tex ? 0xffffff : 0x8b98a5,
      side: THREE.DoubleSide,
      map: tex ?? null,
      lightMap: lightmapTex,
      lightMapIntensity: LIGHTMAP_INTENSITY,
    });

    const meshObj = new THREE.Mesh(geometry, flatMaterial);
    scene.add(meshObj);
    entries.push({ mesh: meshObj, chunk, indices, drawn: triCount, texMaterial });
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
  const countDrawn = (): number => {
    camera.updateMatrixWorld();
    projView.multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
    frustum.setFromProjectionMatrix(projView);
    let n = 0;
    for (const e of entries) {
      if (e.mesh.visible && frustum.intersectsSphere(e.mesh.geometry.boundingSphere!)) n += e.drawn;
    }
    return n;
  };

  let statsListener: ((s: ViewerStats) => void) | null = null;
  let lastStats = 0;

  let raf = 0;
  const animate = () => {
    raf = requestAnimationFrame(animate);
    const dt = Math.min(clock.getDelta(), 0.1);
    if (mode === "fps") {
      stepFps(dt);
    } else {
      controls.update();
    }
    if (marker.visible) {
      ring.rotation.z += dt * 1.5;
      pin.position.y = 1.6 + Math.sin(performance.now() / 300) * 0.25;
    }
    refreshPvs();
    renderer.render(scene, camera);
    const now = performance.now();
    if (statsListener && now - lastStats > 400) {
      lastStats = now;
      statsListener({ drawn: countDrawn(), total: totalTriangles, leaf: currentLeaf, pvsActive });
    }
  };
  animate();

  let texturedOn = opts.textured;
  let lightmapsOn = true;
  const refreshMaterials = () => {
    for (const e of entries) {
      e.mesh.material = texturedOn ? e.texMaterial : flatMaterial;
      e.texMaterial.lightMap = lightmapsOn ? lightmapTex : null;
      e.texMaterial.needsUpdate = true;
    }
  };
  refreshMaterials();

  const applyTransparent = (on: boolean) => {
    for (const e of entries) {
      const mat = e.texMaterial;
      mat.transparent = on;
      mat.depthWrite = !on;
      mat.alphaTest = 0;
      mat.needsUpdate = true;
    }
  };

  return {
    setTextured(on) {
      texturedOn = on;
      refreshMaterials();
    },
    setTransparent: applyTransparent,
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
    dispose() {
      cancelAnimationFrame(raf);
      ro.disconnect();
      document.removeEventListener("pointerlockchange", onPointerLockChange);
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("keyup", onKeyUp);
      if (document.pointerLockElement === renderer.domElement) document.exitPointerLock();
      controls.dispose();
      for (const e of entries) {
        scene.remove(e.mesh);
        e.mesh.geometry.dispose();
        e.texMaterial.dispose();
        (e.texMaterial.map as THREE.Texture | null)?.dispose();
      }
      lightmapTex?.dispose();
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
