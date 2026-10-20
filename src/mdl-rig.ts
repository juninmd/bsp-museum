import * as THREE from "three";
import { materialPlan, type MaterialPlan } from "./lib/mdl-controls.ts";
import type { MdlSummary } from "./types.ts";
import type { MdlExtra } from "./types/mdl-extra.ts";

export type Model = MdlSummary & MdlExtra;

/**
 * Malha do `.mdl` compartilhada entre as cenas (visualizador principal, comparação de skins, miniaturas):
 * a geometria é montada uma vez por modelo (um buffer por textura base, vértices reposicionáveis a cada
 * quadro) e cada "look" (família de skin + conjunto de texturas) é só um jogo de materiais sobre os mesmos
 * buffers — trocar de skin nunca recria geometria.
 */

/** vira Z-up (GoldSrc) em Y-up (Three.js): (x,y,z) -> (x, z, -y). */
export function toWorld(x: number, y: number, z: number): [number, number, number] {
  return [x, z, -y];
}

/** ângulo máximo (cos) entre faces que ainda se suavizam; acima disso a aresta fica dura */
const SMOOTH_COS = 0.5;

/** Texturas já decodificadas de um modelo (índice = `Model.textures`); `null` = imagem que falhou. */
export interface TextureSet {
  textures: (THREE.Texture | null)[];
  dispose(): void;
}

/** Conjunto vazio (antes de as imagens decodificarem): os materiais saem cinza. */
export function emptyTextures(count: number): TextureSet {
  return { textures: new Array<THREE.Texture | null>(count).fill(null), dispose() {} };
}

/** Decodifica as texturas do modelo; imagem que falha vira `null` (o bucket usa cinza). */
export async function decodeTextures(model: Model): Promise<TextureSet> {
  const textures = await Promise.all(
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
      tex.flipY = false; // v do .mdl é de cima pra baixo (linha 0 da imagem em v = 0); o padrão do Three.js espelharia
      tex.wrapS = THREE.RepeatWrapping;
      tex.wrapT = THREE.RepeatWrapping;
      tex.needsUpdate = true;
      return tex;
    }),
  );
  return {
    textures,
    dispose() {
      for (const t of textures) t?.dispose();
    },
  };
}

function makeMaterial(plan: MaterialPlan, tex: THREE.Texture | null): THREE.Material {
  let material: THREE.Material & { alphaTest: number };
  if (plan.shade === "matcap" && tex) {
    // chrome: a textura é o "espelho" (UV = reflexão da normal em relação à câmera)
    material = new THREE.MeshMatcapMaterial({ matcap: tex, side: THREE.DoubleSide, flatShading: plan.flat });
  } else if (plan.shade === "basic") {
    material = new THREE.MeshBasicMaterial({ map: tex, color: tex ? 0xffffff : 0x8b98a5, side: THREE.DoubleSide });
  } else {
    material = new THREE.MeshStandardMaterial({
      map: tex,
      color: tex ? 0xffffff : 0x8b98a5,
      roughness: 0.9,
      metalness: 0,
      side: THREE.DoubleSide,
      flatShading: plan.flat,
    });
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

/** Luzes do visualizador (ambiente, hemisfério, sol e contraluz), visíveis nas camadas 0 e 1. */
export function addModelLights(scene: THREE.Scene): void {
  const lights: THREE.Light[] = [
    new THREE.AmbientLight(0xffffff, 0.7),
    new THREE.HemisphereLight(0xbfd9ff, 0x33373d, 0.6),
    new THREE.DirectionalLight(0xffffff, 0.9),
    new THREE.DirectionalLight(0x8899bb, 0.3),
  ];
  lights[2]!.position.set(1, 2, 1.4);
  lights[3]!.position.set(-1, -0.5, -1);
  for (const light of lights) {
    light.layers.enable(1);
    scene.add(light);
  }
}

/** Caixa do modelo na pose de repouso (espaço Y-up da cena). */
export function restBounds(model: Model): THREE.Box3 {
  const box = new THREE.Box3();
  const p = model.positions;
  const v = new THREE.Vector3();
  for (let i = 0; i + 2 < p.length; i += 3) box.expandByPoint(v.set(...toWorld(p[i]!, p[i + 1]!, p[i + 2]!)));
  if (box.isEmpty()) box.set(new THREE.Vector3(-50, -50, -50), new THREE.Vector3(50, 50, 50));
  return box;
}

interface Bucket {
  textureIndex: number;
  /** triângulo de origem de cada triângulo do bucket */
  tris: number[];
  geometry: THREE.BufferGeometry;
}

/** Um jogo de materiais sobre a geometria do rig: uma família de skin com um conjunto de texturas. */
export interface Look {
  readonly group: THREE.Group;
  /** camada do Three.js (0 ou 1) em que a malha aparece; a câmera escolhe qual desenha */
  setLayer(layer: number): void;
  dispose(): void;
}

export class MdlRig {
  readonly root = new THREE.Group();
  private readonly buckets: Bucket[] = [];
  private readonly looks = new Set<Look>();
  /** por canto de triângulo: grupo de vértices na mesma posição de repouso (CSR em `groupStart`/`groupCorners`) */
  private readonly cornerGroup: Uint32Array;
  private readonly groupStart: Uint32Array;
  private readonly groupCorners: Uint32Array;
  private normals = new Float32Array(0);
  private faceCross = new Float32Array(0);
  private faceUnit = new Float32Array(0);
  private flat: ArrayLike<number>;
  private readonly highlight: THREE.Mesh;
  private highlighted: readonly number[] = [];
  private readonly raycaster = new THREE.Raycaster();

  constructor(readonly model: Model) {
    const byTexture = new Map<number, { tris: number[]; uvs: number[] }>();
    for (let tri = 0; tri < model.texindex.length; tri++) {
      const id = model.texindex[tri]!;
      let group = byTexture.get(id);
      if (!group) byTexture.set(id, (group = { tris: [], uvs: [] }));
      group.tris.push(tri);
      for (let k = 0; k < 6; k++) group.uvs.push(model.uvs[tri * 6 + k]!);
    }
    for (const [textureIndex, group] of byTexture) {
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(group.tris.length * 9), 3));
      geometry.setAttribute("normal", new THREE.BufferAttribute(new Float32Array(group.tris.length * 9), 3));
      geometry.setAttribute("uv", new THREE.Float32BufferAttribute(group.uvs, 2));
      this.buckets.push({ textureIndex, tris: group.tris, geometry });
    }
    ({ cornerGroup: this.cornerGroup, groupStart: this.groupStart, groupCorners: this.groupCorners } = weldCorners(
      model.positions,
    ));
    this.flat = model.positions;

    // realce de triângulos (mapa UV / seleção): fica por cima de tudo, nas duas camadas
    const hl = new THREE.BufferGeometry();
    hl.setAttribute("position", new THREE.BufferAttribute(new Float32Array(0), 3));
    this.highlight = new THREE.Mesh(
      hl,
      new THREE.MeshBasicMaterial({ color: 0xffd24a, transparent: true, opacity: 0.55, depthTest: false, side: THREE.DoubleSide }),
    );
    this.highlight.frustumCulled = false;
    this.highlight.renderOrder = 9;
    this.highlight.visible = false;
    this.highlight.layers.enable(1);
    this.root.add(this.highlight);

    this.setVertexPositions(model.positions);
  }

  /** Cria um look: a textura de cada bucket vem de `map[textura base]` dentro de `textures`. */
  addLook(map: readonly number[], textures: TextureSet, layer = 0): Look {
    const group = new THREE.Group();
    const materials: THREE.Material[] = [];
    for (const b of this.buckets) {
      const id = map[b.textureIndex] ?? b.textureIndex;
      const material = makeMaterial(materialPlan(this.model.texture_info?.[id]), textures.textures[id] ?? null);
      materials.push(material);
      const mesh = new THREE.Mesh(b.geometry, material);
      mesh.frustumCulled = false; // a pose muda a cada quadro
      mesh.renderOrder = (material as THREE.Material).blending === THREE.AdditiveBlending ? 1 : 0;
      mesh.userData.bucket = b;
      group.add(mesh);
    }
    const look: Look = {
      group,
      setLayer: (n) => group.children.forEach((c) => c.layers.set(n)),
      dispose: () => {
        if (!this.looks.delete(look)) return;
        this.root.remove(group);
        for (const m of materials) m.dispose();
      },
    };
    look.setLayer(layer);
    this.looks.add(look);
    this.root.add(group);
    return look;
  }

  /** Reposiciona os vértices: mesmo layout de `MdlSummary.positions` (9 floats por triângulo, espaço do jogo). */
  setVertexPositions(flat: ArrayLike<number>): void {
    this.flat = flat;
    const nTri = Math.floor(this.model.positions.length / 9);
    if (this.faceCross.length !== nTri * 3) {
      this.faceCross = new Float32Array(nTri * 3);
      this.faceUnit = new Float32Array(nTri * 3);
      this.normals = new Float32Array(nTri * 9);
    }
    const { faceCross, faceUnit, normals } = this;
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
    const { cornerGroup, groupStart, groupCorners } = this;
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
    for (const b of this.buckets) {
      const pos = b.geometry.getAttribute("position") as THREE.BufferAttribute;
      const nor = b.geometry.getAttribute("normal") as THREE.BufferAttribute;
      for (let t = 0; t < b.tris.length; t++) {
        const tri = b.tris[t]!;
        for (let v = 0; v < 3; v++) {
          const s = tri * 9 + v * 3;
          const [x, y, z] = toWorld(flat[s]!, flat[s + 1]!, flat[s + 2]!);
          pos.setXYZ(t * 3 + v, x, y, z);
          const [nx, ny, nz] = toWorld(normals[tri * 9 + v * 3]!, normals[tri * 9 + v * 3 + 1]!, normals[tri * 9 + v * 3 + 2]!);
          nor.setXYZ(t * 3 + v, nx, ny, nz);
        }
      }
      pos.needsUpdate = true;
      nor.needsUpdate = true;
      // a caixa envolvente mudou com a pose: o raycast recalcula quando precisar
      b.geometry.boundingSphere = null;
      b.geometry.boundingBox = null;
    }
    if (this.highlighted.length > 0) this.refreshHighlight();
  }

  /** Triângulo de origem (índice em `MdlSummary.texindex`) sob o raio, ou -1. `layer` = a camada do look visível. */
  pick(ndcX: number, ndcY: number, camera: THREE.Camera, layer = 0): number {
    this.raycaster.setFromCamera(new THREE.Vector2(ndcX, ndcY), camera);
    this.raycaster.layers.set(layer);
    const looks = [...this.looks].filter((l) => l.group.visible && l.group.children[0]?.layers.isEnabled(layer));
    const hits = this.raycaster.intersectObjects(
      looks.flatMap((l) => l.group.children),
      false,
    );
    for (const hit of hits) {
      const bucket = hit.object.userData.bucket as Bucket | undefined;
      if (bucket && hit.faceIndex != null) return bucket.tris[hit.faceIndex] ?? -1;
    }
    return -1;
  }

  /** Realça triângulos (índices de `texindex`); lista vazia apaga. */
  setHighlight(tris: readonly number[]): void {
    this.highlighted = tris;
    this.refreshHighlight();
  }

  private refreshHighlight() {
    const tris = this.highlighted;
    const n = tris.length;
    this.highlight.visible = n > 0;
    if (n === 0) return;
    const geo = this.highlight.geometry;
    let pos = geo.getAttribute("position") as THREE.BufferAttribute;
    if (pos.count < n * 3) {
      pos = new THREE.BufferAttribute(new Float32Array(Math.max(n, 16) * 9), 3);
      geo.setAttribute("position", pos);
    }
    let k = 0;
    for (const tri of tris) {
      for (let v = 0; v < 3; v++) {
        const s = tri * 9 + v * 3;
        const [x, y, z] = toWorld(this.flat[s] ?? 0, this.flat[s + 1] ?? 0, this.flat[s + 2] ?? 0);
        pos.setXYZ(k++, x, y, z);
      }
    }
    pos.needsUpdate = true;
    geo.setDrawRange(0, n * 3);
    geo.boundingSphere = null;
  }

  dispose(): void {
    for (const look of [...this.looks]) look.dispose();
    for (const b of this.buckets) b.geometry.dispose();
    this.highlight.geometry.dispose();
    (this.highlight.material as THREE.Material).dispose();
    this.root.removeFromParent();
  }
}

/** Agrupa os cantos de triângulo que coincidem na pose de repouso, pra suavizar as normais. */
function weldCorners(positions: ArrayLike<number>) {
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
