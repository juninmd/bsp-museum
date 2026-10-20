import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { BOX_STRIDE, hitGroupColor, materialPlan, type MaterialPlan } from "./lib/mdl-controls.ts";
import type { MdlSummary } from "./types.ts";
import type { MdlExtra } from "./types/mdl-extra.ts";

type Model = MdlSummary & MdlExtra;

/**
 * Cena 3D própria do visualizador avulso de `.mdl` (independente de
 * `viewer3d.ts`, que é do mapa): orbitável, com a malha dividida por textura
 * e os vértices reposicionáveis a cada quadro (skinning feito por quem chama).
 * Materiais seguem as flags do modelo (chrome, aditivo, máscara, fullbright,
 * flatshade); hitboxes e attachments são overlays opcionais.
 */
export interface ModelViewer {
  /** reposiciona os vértices: mesmo layout de `MdlSummary.positions` (9 floats por triângulo, espaço do jogo) */
  setVertexPositions(flat: ArrayLike<number>): void;
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
  dispose(): void;
}

/** vira Z-up (GoldSrc) em Y-up (Three.js): (x,y,z) -> (x, z, -y). */
function toWorld(x: number, y: number, z: number): [number, number, number] {
  return [x, z, -y];
}

/** arestas de um cubo: pares de cantos (índice = bits sx|sy<<1|sz<<2) que diferem em um bit */
const BOX_EDGES: [number, number][] = (() => {
  const edges: [number, number][] = [];
  for (let a = 0; a < 8; a++) for (let bit = 1; bit < 8; bit <<= 1) if (!(a & bit)) edges.push([a, a | bit]);
  return edges;
})();

/** ângulo máximo (cos) entre faces que ainda se suavizam; acima disso a aresta fica dura */
const SMOOTH_COS = 0.5;

interface Bucket {
  textureIndex: number;
  /** triângulo de origem de cada triângulo do bucket */
  tris: number[];
  mesh: THREE.Mesh;
  material: THREE.Material;
  /** normais da face (flatshade) em vez das suavizadas */
  flat: boolean;
}

interface Built {
  model: Model;
  textures: (THREE.Texture | null)[];
  buckets: Bucket[];
  /** por canto de triângulo: grupo de vértices na mesma posição de repouso (CSR em `groupStart`/`groupCorners`) */
  cornerGroup: Uint32Array;
  groupStart: Uint32Array;
  groupCorners: Uint32Array;
}

/** Agrupa os cantos de triângulo que coincidem na pose de repouso, pra suavizar as normais. */
function weldCorners(positions: ArrayLike<number>): Pick<Built, "cornerGroup" | "groupStart" | "groupCorners"> {
  const corners = Math.floor(positions.length / 3);
  const ids = new Map<string, number>();
  const cornerGroup = new Uint32Array(corners);
  const counts: number[] = [];
  for (let c = 0; c < corners; c++) {
    const key = `${Math.round(positions[c * 3]! * 50)},${Math.round(positions[c * 3 + 1]! * 50)},${Math.round(positions[c * 3 + 2]! * 50)}`;
    let g = ids.get(key);
    if (g === undefined) {
      g = counts.length;
      ids.set(key, g);
      counts.push(0);
    }
    cornerGroup[c] = g;
    counts[g]!++;
  }
  const groupStart = new Uint32Array(counts.length + 1);
  for (let g = 0; g < counts.length; g++) groupStart[g + 1] = groupStart[g]! + counts[g]!;
  const fill = groupStart.slice(0, counts.length);
  const groupCorners = new Uint32Array(corners);
  for (let c = 0; c < corners; c++) groupCorners[fill[cornerGroup[c]!]!++] = c;
  return { cornerGroup, groupStart, groupCorners };
}

/** Decodifica as texturas do modelo; imagem que falha vira `null` (o bucket usa cinza). */
async function loadTextures(model: Model): Promise<(THREE.Texture | null)[]> {
  return Promise.all(
    model.textures.map(async (t) => {
      if (!t.png) return null;
      const img = new Image();
      img.src = t.png;
      try {
        await img.decode();
      } catch {
        return null;
      }
      const tex = new THREE.Texture(img);
      tex.colorSpace = THREE.SRGBColorSpace;
      tex.wrapS = THREE.RepeatWrapping;
      tex.wrapT = THREE.RepeatWrapping;
      tex.needsUpdate = true;
      return tex;
    }),
  );
}

function makeMaterial(plan: MaterialPlan, tex: THREE.Texture | null): THREE.Material {
  let material: THREE.Material & { alphaTest: number };
  if (plan.shade === "matcap" && tex) {
    // chrome: a textura é o "espelho" (UV = reflexão da normal em relação à câmera)
    material = new THREE.MeshMatcapMaterial({ matcap: tex, side: THREE.DoubleSide });
  } else if (plan.shade === "basic") {
    material = new THREE.MeshBasicMaterial({ map: tex, color: tex ? 0xffffff : 0x8b98a5, side: THREE.DoubleSide });
  } else {
    material = new THREE.MeshStandardMaterial({ map: tex, color: tex ? 0xffffff : 0x8b98a5, roughness: 0.9, metalness: 0, side: THREE.DoubleSide });
  }
  if (plan.blend === "masked") {
    material.alphaTest = 0.5; // texturas mascaradas (grades, cabelo): índice 255 da paleta tem alfa 0
  } else if (plan.blend === "additive") {
    material.transparent = true;
    material.blending = THREE.AdditiveBlending;
    material.depthWrite = false;
  }
  return material;
}

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
  scene.add(new THREE.AmbientLight(0xffffff, 0.7));
  scene.add(new THREE.HemisphereLight(0xbfd9ff, 0x33373d, 0.6));
  const sun = new THREE.DirectionalLight(0xffffff, 0.9);
  sun.position.set(1, 2, 1.4);
  scene.add(sun);
  const fill = new THREE.DirectionalLight(0x8899bb, 0.3);
  fill.position.set(-1, -0.5, -1);
  scene.add(fill);

  let built: Built = build(initial, initial.textures.map(() => null));
  let textureMap: number[] = [];
  let normals = new Float32Array(0);
  let faceCross = new Float32Array(0);
  let faceUnit = new Float32Array(0);
  let disposed = false;
  let swap = 0;

  /** Um mesh por textura, na ordem em que aparecem; `tris` guarda de onde cada triângulo veio. */
  function build(model: Model, textures: (THREE.Texture | null)[]): Built {
    const byTexture = new Map<number, { tris: number[]; uvs: number[] }>();
    for (let tri = 0; tri < model.texindex.length; tri++) {
      const id = model.texindex[tri]!;
      let group = byTexture.get(id);
      if (!group) byTexture.set(id, (group = { tris: [], uvs: [] }));
      group.tris.push(tri);
      for (let k = 0; k < 6; k++) group.uvs.push(model.uvs[tri * 6 + k]!);
    }
    const buckets: Bucket[] = [];
    for (const [textureIndex, group] of byTexture) {
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(group.tris.length * 9), 3));
      geometry.setAttribute("normal", new THREE.BufferAttribute(new Float32Array(group.tris.length * 9), 3));
      geometry.setAttribute("uv", new THREE.Float32BufferAttribute(group.uvs, 2));
      const material = new THREE.MeshStandardMaterial();
      const mesh = new THREE.Mesh(geometry, material);
      mesh.frustumCulled = false; // a pose muda a cada quadro
      scene.add(mesh);
      buckets.push({ textureIndex, tris: group.tris, mesh, material, flat: false });
    }
    return { model, textures, buckets, ...weldCorners(model.positions) };
  }

  function discard(b: Built) {
    for (const bucket of b.buckets) {
      scene.remove(bucket.mesh);
      bucket.mesh.geometry.dispose();
      bucket.material.dispose();
    }
    for (const t of b.textures) t?.dispose();
  }

  /** (re)cria o material de cada bucket a partir da textura da família de skin e das flags dela */
  function applyMaterials() {
    for (const b of built.buckets) {
      const id = textureMap[b.textureIndex] ?? b.textureIndex;
      const tex = built.textures[id] ?? null;
      const plan = materialPlan(built.model.texture_info?.[id]);
      b.material.dispose();
      b.material = makeMaterial(plan, tex);
      b.mesh.material = b.material;
      b.mesh.renderOrder = plan.blend === "additive" ? 1 : 0;
      b.flat = plan.flat;
    }
  }

  const setTextureMap = (map: number[]) => {
    textureMap = map;
    applyMaterials();
  };

  const setVertexPositions = (flat: ArrayLike<number>) => {
    const model = built.model;
    const nTri = Math.floor(model.positions.length / 9);
    if (faceCross.length !== nTri * 3) {
      faceCross = new Float32Array(nTri * 3);
      faceUnit = new Float32Array(nTri * 3);
      normals = new Float32Array(nTri * 9);
    }
    // normal de cada face (e o vetor ponderado pela área)
    for (let t = 0; t < nTri; t++) {
      const s = t * 9;
      const ax = flat[s + 3]! - flat[s]!;
      const ay = flat[s + 4]! - flat[s + 1]!;
      const az = flat[s + 5]! - flat[s + 2]!;
      const bx = flat[s + 6]! - flat[s]!;
      const by = flat[s + 7]! - flat[s + 1]!;
      const bz = flat[s + 8]! - flat[s + 2]!;
      const cx = ay * bz - az * by;
      const cy = az * bx - ax * bz;
      const cz = ax * by - ay * bx;
      const len = Math.hypot(cx, cy, cz);
      faceCross[t * 3] = cx;
      faceCross[t * 3 + 1] = cy;
      faceCross[t * 3 + 2] = cz;
      faceUnit[t * 3] = len > 1e-9 ? cx / len : 0;
      faceUnit[t * 3 + 1] = len > 1e-9 ? cy / len : 0;
      faceUnit[t * 3 + 2] = len > 1e-9 ? cz / len : 1;
    }
    // normal suavizada: soma só as faces vizinhas no mesmo vértice com ângulo pequeno (aresta dura continua dura)
    const { cornerGroup, groupStart, groupCorners } = built;
    for (let c = 0; c < nTri * 3; c++) {
      const t = (c / 3) | 0;
      const fx = faceUnit[t * 3]!;
      const fy = faceUnit[t * 3 + 1]!;
      const fz = faceUnit[t * 3 + 2]!;
      const g = cornerGroup[c]!;
      let nx = 0;
      let ny = 0;
      let nz = 0;
      for (let k = groupStart[g]!; k < groupStart[g + 1]!; k++) {
        const o = (groupCorners[k]! / 3) | 0;
        if (fx * faceUnit[o * 3]! + fy * faceUnit[o * 3 + 1]! + fz * faceUnit[o * 3 + 2]! >= SMOOTH_COS) {
          nx += faceCross[o * 3]!;
          ny += faceCross[o * 3 + 1]!;
          nz += faceCross[o * 3 + 2]!;
        }
      }
      const len = Math.hypot(nx, ny, nz);
      normals[c * 3] = len > 1e-9 ? nx / len : fx;
      normals[c * 3 + 1] = len > 1e-9 ? ny / len : fy;
      normals[c * 3 + 2] = len > 1e-9 ? nz / len : fz;
    }
    for (const b of built.buckets) {
      const pos = b.mesh.geometry.getAttribute("position") as THREE.BufferAttribute;
      const nor = b.mesh.geometry.getAttribute("normal") as THREE.BufferAttribute;
      const src = b.flat ? faceUnit : null;
      for (let t = 0; t < b.tris.length; t++) {
        const tri = b.tris[t]!;
        for (let v = 0; v < 3; v++) {
          const s = tri * 9 + v * 3;
          const [x, y, z] = toWorld(flat[s]!, flat[s + 1]!, flat[s + 2]!);
          pos.setXYZ(t * 3 + v, x, y, z);
          const [nx, ny, nz] = src
            ? toWorld(src[tri * 3]!, src[tri * 3 + 1]!, src[tri * 3 + 2]!)
            : toWorld(normals[tri * 9 + v * 3]!, normals[tri * 9 + v * 3 + 1]!, normals[tri * 9 + v * 3 + 2]!);
          nor.setXYZ(t * 3 + v, nx, ny, nz);
        }
      }
      pos.needsUpdate = true;
      nor.needsUpdate = true;
    }
  };

  // Texturas do modelo inicial chegam depois do primeiro desenho.
  void loadTextures(initial).then((textures) => {
    if (disposed || built.model !== initial) {
      for (const t of textures) t?.dispose();
      return;
    }
    built.textures = textures;
    applyMaterials();
  });
  applyMaterials();
  setVertexPositions(initial.positions);

  // Enquadra a câmera na pose de repouso.
  const box = new THREE.Box3();
  for (const b of built.buckets) {
    b.mesh.geometry.computeBoundingBox();
    box.union(b.mesh.geometry.boundingBox!);
  }
  if (box.isEmpty()) box.set(new THREE.Vector3(-50, -50, -50), new THREE.Vector3(50, 50, 50));
  const center = box.getCenter(new THREE.Vector3());
  const maxDim = Math.max(...box.getSize(new THREE.Vector3()).toArray(), 1);

  const camera = new THREE.PerspectiveCamera(60, 1, 0.5, 100000);
  const dist = (maxDim / 2 / Math.tan((camera.fov * Math.PI) / 360)) * 1.6;
  camera.position.set(center.x + dist * 0.8, center.y + dist * 0.5, center.z + dist);
  camera.near = Math.max(0.1, dist / 1000);
  camera.far = Math.max(1000, dist * 20);
  camera.updateProjectionMatrix();
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.target.copy(center);
  controls.minDistance = Math.max(1, dist / 10);
  controls.maxDistance = dist * 6;
  controls.update();

  // ---- overlays: hitboxes (12 arestas por caixa, cor do grupo) e attachments (cruz + nome)
  const hitLines = new THREE.LineSegments(
    new THREE.BufferGeometry(),
    new THREE.LineBasicMaterial({ vertexColors: true, depthTest: false, transparent: true, opacity: 0.95 }),
  );
  hitLines.frustumCulled = false;
  hitLines.renderOrder = 10;
  hitLines.visible = false;
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
  const armLen = maxDim * 0.025;

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
        const cross = new THREE.LineSegments(
          new THREE.BufferGeometry().setAttribute(
            "position",
            new THREE.Float32BufferAttribute([-armLen, 0, 0, armLen, 0, 0, 0, -armLen, 0, 0, armLen, 0, 0, 0, -armLen, 0, 0, armLen], 3),
          ),
          new THREE.LineBasicMaterial({ color: 0xffd24a, depthTest: false, transparent: true }),
        );
        cross.renderOrder = 11;
        cross.frustumCulled = false;
        const label = labelSprite(names[i] || `#${i}`);
        label.scale.set(maxDim * 0.16, maxDim * 0.04, 1);
        label.frustumCulled = false;
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

  const setModel = async (model: Model) => {
    const mine = ++swap;
    const textures = await loadTextures(model);
    if (disposed || mine !== swap) {
      for (const t of textures) t?.dispose();
      return;
    }
    const old = built;
    built = build(model, textures);
    discard(old);
    applyMaterials();
    setVertexPositions(model.positions);
  };

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
    controls.update();
    renderer.render(scene, camera);
  };
  animate();

  return {
    setVertexPositions,
    setTextureMap,
    setModel,
    setHitboxes,
    setAttachments,
    dispose() {
      disposed = true;
      cancelAnimationFrame(raf);
      ro.disconnect();
      controls.dispose();
      discard(built);
      hitLines.geometry.dispose();
      (hitLines.material as THREE.Material).dispose();
      clearAttachments();
      renderer.dispose();
      renderer.domElement.remove();
    },
  };
}
