import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import type { MeshDetail, SkyBox } from "./types.ts";

export interface Viewer3D {
  setTextured(on: boolean): void;
  setTransparent(on: boolean): void;
  enterFirstPerson(): void;
  exitFirstPerson(): void;
  setFullscreen(on: boolean): void;
  dispose(): void;
}

/** vira Z-up (GoldSrc) em Y-up (Three.js): (x,y,z) -> (x, z, -y). */
function toWorld(x: number, y: number, z: number): [number, number, number] {
  return [x, z, -y];
}

function clamp01(t: number): number {
  return t < 0 ? 0 : t > 1 ? 1 : t;
}

/** Mesma paleta por altura da planta baixa: frio embaixo, quente no topo. */
function heightRgb(t: number): [number, number, number] {
  const low: [number, number, number] = [26, 42, 71];
  const mid: [number, number, number] = [31, 122, 140];
  const high: [number, number, number] = [242, 193, 78];
  const lerp = (a: [number, number, number], b: [number, number, number], k: number): [number, number, number] => [
    a[0] + (b[0] - a[0]) * k,
    a[1] + (b[1] - a[1]) * k,
    a[2] + (b[2] - a[2]) * k,
  ];
  const rgb = t < 0.5 ? lerp(low, mid, t * 2) : lerp(mid, high, (t - 0.5) * 2);
  return rgb.map((v) => v / 255) as [number, number, number];
}

function buildTextures(mesh: MeshDetail): (THREE.Texture | null)[] {
  return mesh.textures.map((t) => {
    if (!t.png) return null;
    const tex = new THREE.TextureLoader().load(t.png);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.wrapS = THREE.RepeatWrapping;
    tex.wrapT = THREE.RepeatWrapping;
    tex.minFilter = THREE.LinearMipmapLinearFilter;
    return tex;
  });
}

interface Bucket {
  positions: number[];
  uvs: number[];
  colors: number[];
  textureIndex: number;
}

/** Separa os triângulos por textura e gera uma malha + UV + cor por vértice. */
function makeBuckets(mesh: MeshDetail): { buckets: Bucket[]; zMin: number; zMax: number } {
  const zMin = mesh.bounds ? mesh.bounds.mins[2] : 0;
  const zMax = mesh.bounds ? mesh.bounds.maxs[2] : 1;
  const zRange = Math.max(1, zMax - zMin);

  const order: number[] = [];
  const seen = new Set<number>();
  for (const tex of mesh.texindex) {
    const id = tex;
    if (!seen.has(id)) {
      seen.add(id);
      order.push(id);
    }
  }
  const bucketOf = new Map<number, Bucket>();
  for (const id of order) {
    bucketOf.set(id, { positions: [], uvs: [], colors: [], textureIndex: id });
  }

  const flat = mesh.positions;
  const uvFlat = mesh.uvs;
  for (let tri = 0; tri < mesh.texindex.length; tri++) {
    const b = bucketOf.get(mesh.texindex[tri]!);
    if (!b) continue;
    const p0 = tri * 9;
    const u0 = tri * 6;
    for (let v = 0; v < 3; v++) {
      const x = flat[p0 + v * 3]!;
      const y = flat[p0 + v * 3 + 1]!;
      const z = flat[p0 + v * 3 + 2]!;
      const [wx, wy, wz] = toWorld(x, y, z);
      b.positions.push(wx, wy, wz);
      b.uvs.push(uvFlat[u0 + v * 2]!, uvFlat[u0 + v * 2 + 1]!);
      const rgb = heightRgb(clamp01((z - zMin) / zRange));
      b.colors.push(rgb[0], rgb[1], rgb[2]);
    }
  }

  return { buckets: order.map((id) => bucketOf.get(id)!), zMin, zMax };
}

/**
 * Monta a cena 3D orbitável dentro de `container`.
 * `textured` liga as texturas reais do BSP; desligado mostra a cor por altura.
 */
export function mount3D(container: HTMLElement, mesh: MeshDetail, textured: boolean): Viewer3D {
  const { buckets } = makeBuckets(mesh);

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
  // um vulto escuro sobre o fundo escuro e some de vista.
  scene.add(new THREE.AmbientLight(0xffffff, 0.6));
  const hemi = new THREE.HemisphereLight(0xbfd9ff, 0x33373d, 0.6);
  scene.add(hemi);
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
  const entries: { mesh: THREE.Mesh; texMaterial: THREE.MeshStandardMaterial }[] = [];

  for (const bucket of buckets) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(bucket.positions, 3));
    geometry.setAttribute("uv", new THREE.Float32BufferAttribute(bucket.uvs, 2));
    geometry.setAttribute("color", new THREE.Float32BufferAttribute(bucket.colors, 3));
    geometry.computeVertexNormals();

    const tex = bucket.textureIndex < textures.length ? textures[bucket.textureIndex] : undefined;
    const texMaterial = new THREE.MeshStandardMaterial({
      color: tex ? 0xffffff : 0x8b98a5,
      roughness: 0.9,
      metalness: 0,
      side: THREE.DoubleSide,
      map: tex ?? null,
    });

    const meshObj = new THREE.Mesh(geometry, flatMaterial);
    scene.add(meshObj);
    entries.push({ mesh: meshObj, texMaterial });
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

  function onKeyDown(e: KeyboardEvent) {
    if (mode !== "fps") return;
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
    if (camera.position.y < 1) camera.position.y = 1;
    camera.rotation.order = "YXZ";
    camera.rotation.set(pitch, yaw, 0);
  }

  function enterFirstPerson() {
    if (mode === "fps") return;
    mode = "fps";
    controls.enabled = false;
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

  let raf = 0;
  const animate = () => {
    raf = requestAnimationFrame(animate);
    const dt = Math.min(clock.getDelta(), 0.1);
    if (mode === "fps") {
      stepFps(dt);
    } else {
      controls.update();
    }
    renderer.render(scene, camera);
  };
  animate();

  const applyMode = (on: boolean) => {
    for (const e of entries) {
      e.mesh.material = on ? e.texMaterial : flatMaterial;
      if (on) e.texMaterial.needsUpdate = true;
    }
  };
  applyMode(textured);

  const applyTransparent = (on: boolean) => {
    for (const e of entries) {
      const mat = e.texMaterial;
      if (on) {
        mat.transparent = true;
        mat.depthWrite = false;
        mat.alphaTest = 0;
      } else {
        mat.transparent = false;
        mat.depthWrite = true;
        mat.alphaTest = 0;
      }
      mat.needsUpdate = true;
    }
  };

  return {
    setTextured: applyMode,
    setTransparent: applyTransparent,
    enterFirstPerson,
    exitFirstPerson,
    setFullscreen,
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
      flatMaterial.dispose();
      scene.remove(sky);
      disposeSky(sky);
      for (const s of spawnGroup.children) {
        (s as THREE.Mesh).geometry.dispose();
        ((s as THREE.Mesh).material as THREE.Material).dispose();
      }
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
          map: new THREE.TextureLoader().load(url),
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
