/**
 * Geometria do viewer: separa a malha do BSP em "chunks" por (textura × célula do
 * mapa). Cada chunk vira um `Mesh`: o frustum culling do Three.js passa a descartar
 * pedaços inteiros do mapa fora da câmera, e o PVS liga/desliga triângulos por face.
 *
 * Código puro (sem DOM, sem Three.js): roda no Web Worker e nos testes.
 */

export interface MeshInput {
  positions: ArrayLike<number>;
  uvs: ArrayLike<number>;
  lmUvs: ArrayLike<number>;
  texindex: ArrayLike<number>;
  triFace: ArrayLike<number>;
  /** altura mínima/máxima do mapa (eixo Z do GoldSrc), para a cor por altura */
  zMin: number;
  zMax: number;
  /** modelo (0 = mundo, N = brush entity `*N`) de cada face do BSP; ausente = tudo no mundo */
  faceModel?: ArrayLike<number>;
  /** estilos de luz, 4 por face do BSP (255 = vazio); ausente/vazio = luz estática */
  faceStyles?: ArrayLike<number>;
}

export interface Chunk {
  textureIndex: number;
  /** xyz já em Y-up (Three.js) */
  positions: Float32Array;
  uvs: Float32Array;
  /** uv do lightmap; vazio quando o mapa não tem atlas */
  lmUvs: Float32Array;
  colors: Float32Array;
  /** face do BSP de cada triângulo (-1 = prop) */
  triFace: Int32Array;
  center: [number, number, number];
  radius: number;
  /** modelo do BSP: 0 = mundo/props; N = brush entity `*N` (um chunk por textura, sem grade) */
  model: number;
  /**
   * estilos de luz por vértice (4 bytes por vértice, 255 = vazio), só quando alguma face do
   * chunk usa estilo animado; vazio caso contrário (o chunk usa o material comum)
   */
  styles: Uint8Array;
}

/** Z-up (GoldSrc) -> Y-up (Three.js): (x, y, z) -> (x, z, -y). */
export function toWorld(x: number, y: number, z: number): [number, number, number] {
  return [x, z, -y];
}

const clamp01 = (t: number) => (t < 0 ? 0 : t > 1 ? 1 : t);

/** Mesma paleta por altura da planta baixa: frio embaixo, quente no topo. */
export function heightRgb(t: number): [number, number, number] {
  const low = [26, 42, 71];
  const mid = [31, 122, 140];
  const high = [242, 193, 78];
  const k = clamp01(t);
  const [a, b, f] = k < 0.5 ? [low, mid, k * 2] : [mid, high, (k - 0.5) * 2];
  return [(a[0]! + (b[0]! - a[0]!) * f) / 255, (a[1]! + (b[1]! - a[1]!) * f) / 255, (a[2]! + (b[2]! - a[2]!) * f) / 255];
}

/** Tamanho da célula da grade (em unidades do mapa): ~6 células no maior lado, no mínimo 1024. */
export function cellSizeFor(extent: number): number {
  return Math.max(1024, Math.ceil(extent / 6));
}

export function buildChunks(input: MeshInput, cellSize: number): Chunk[] {
  const tris = input.texindex.length;
  const hasLm = input.lmUvs.length === tris * 6 && tris > 0;
  const zRange = Math.max(1, input.zMax - input.zMin);

  // 1ª passada: em que chunk cai cada triângulo. Mundo: textura × célula (centroide no plano XY).
  // Brush entity: textura × modelo, sem grade — cada entidade precisa do próprio Mesh para
  // ter material (modo de render, rolagem) e movimento próprios.
  interface Bucket {
    tex: number;
    model: number;
    tris: number[];
  }
  const buckets = new Map<string, Bucket>();
  for (let t = 0; t < tris; t++) {
    const p = t * 9;
    const face = input.triFace[t] ?? -1;
    const model = face >= 0 ? (input.faceModel?.[face] ?? 0) : 0;
    let key: string;
    if (model > 0) {
      key = `${input.texindex[t]}|m${model}`;
    } else {
      const cx = (input.positions[p]! + input.positions[p + 3]! + input.positions[p + 6]!) / 3;
      const cy = (input.positions[p + 1]! + input.positions[p + 4]! + input.positions[p + 7]!) / 3;
      key = `${input.texindex[t]}|${Math.floor(cx / cellSize)}|${Math.floor(cy / cellSize)}`;
    }
    const bucket = buckets.get(key);
    if (bucket) bucket.tris.push(t);
    else buckets.set(key, { tex: input.texindex[t]!, model: Math.max(0, model), tris: [t] });
  }
  const styled = !!input.faceStyles && input.faceStyles.length >= 4;

  const chunks: Chunk[] = [];
  for (const { tex, model, tris: list } of buckets.values()) {
    const n = list.length;
    const positions = new Float32Array(n * 9);
    const uvs = new Float32Array(n * 6);
    const lmUvs = hasLm ? new Float32Array(n * 6) : new Float32Array(0);
    const colors = new Float32Array(n * 9);
    const triFace = new Int32Array(n);
    const animated = styled && list.some((t) => isAnimatedFace(input.faceStyles!, input.triFace[t] ?? -1));
    const styles = animated ? new Uint8Array(n * 12) : null;
    let minX = Infinity, minY = Infinity, minZ = Infinity;
    let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;

    list.forEach((t, i) => {
      const face = input.triFace[t] ?? -1;
      triFace[i] = face;
      if (styles) {
        // face sem estilo próprio (prop, ou face comum num chunk com luz animada): só o estilo 0
        const own = face >= 0;
        for (let v = 0; v < 3; v++) {
          styles[i * 12 + v * 4] = own ? input.faceStyles![face * 4]! : 0;
          for (let k = 1; k < 4; k++) styles[i * 12 + v * 4 + k] = own ? input.faceStyles![face * 4 + k]! : 255;
        }
      }
      for (let v = 0; v < 3; v++) {
        const s = t * 9 + v * 3;
        const x = input.positions[s]!;
        const y = input.positions[s + 1]!;
        const z = input.positions[s + 2]!;
        const [wx, wy, wz] = toWorld(x, y, z);
        const d = i * 9 + v * 3;
        positions[d] = wx;
        positions[d + 1] = wy;
        positions[d + 2] = wz;
        const rgb = heightRgb((z - input.zMin) / zRange);
        colors[d] = rgb[0];
        colors[d + 1] = rgb[1];
        colors[d + 2] = rgb[2];
        uvs[i * 6 + v * 2] = input.uvs[t * 6 + v * 2]!;
        uvs[i * 6 + v * 2 + 1] = input.uvs[t * 6 + v * 2 + 1]!;
        if (hasLm) {
          lmUvs[i * 6 + v * 2] = input.lmUvs[t * 6 + v * 2]!;
          lmUvs[i * 6 + v * 2 + 1] = input.lmUvs[t * 6 + v * 2 + 1]!;
        }
        minX = Math.min(minX, wx); maxX = Math.max(maxX, wx);
        minY = Math.min(minY, wy); maxY = Math.max(maxY, wy);
        minZ = Math.min(minZ, wz); maxZ = Math.max(maxZ, wz);
      }
    });

    const center: [number, number, number] = [(minX + maxX) / 2, (minY + maxY) / 2, (minZ + maxZ) / 2];
    const radius = Math.hypot(maxX - minX, maxY - minY, maxZ - minZ) / 2;
    chunks.push({ textureIndex: tex, positions, uvs, lmUvs, colors, triFace, center, radius, model, styles: styles ?? new Uint8Array(0) });
  }
  return chunks;
}

/**
 * A face precisa do shader de estilos? Sim quando o slot 0 não é o estilo 0 (luz normal)
 * ou há luz nos slots 1..3. Face fora do BSP (`-1`) ou sem lightmap (tudo 255) não precisa.
 */
export function isAnimatedFace(faceStyles: ArrayLike<number>, face: number): boolean {
  if (face < 0 || face * 4 + 3 >= faceStyles.length) return false;
  const b = face * 4;
  const s0 = faceStyles[b]!;
  if (s0 === 255) return false; // sem lightmap
  return s0 !== 0 || faceStyles[b + 1] !== 255 || faceStyles[b + 2] !== 255 || faceStyles[b + 3] !== 255;
}

/** Índices (3 por triângulo) dos triângulos de `triFace` visíveis segundo `visible`. */
export function visibleIndices(
  triFace: Int32Array,
  visible: ((face: number) => boolean) | null,
  out: Uint32Array,
): number {
  let n = 0;
  for (let t = 0; t < triFace.length; t++) {
    const face = triFace[t]!;
    if (visible && face >= 0 && !visible(face)) continue;
    const base = t * 3;
    out[n++] = base;
    out[n++] = base + 1;
    out[n++] = base + 2;
  }
  return n;
}
