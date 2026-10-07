/**
 * PVS (potentially visible set): descobre em que folha da árvore BSP está a câmera
 * e quais faces as folhas visíveis a partir dela enxergam.
 *
 * Trabalha em coordenadas do GoldSrc (Z-up). Qualquer dado torto (índice fora da
 * faixa, ciclo, vis truncado) devolve `null` = "desenhe tudo" — nunca esconde
 * geometria por engano.
 */
import type { PvsData } from "../types.ts";

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Descomprime uma linha de PVS (RLE de zeros). `null` se a linha está truncada. */
export function decompressVis(vis: Uint8Array, offset: number, rowBytes: number): Uint8Array | null {
  const out = new Uint8Array(rowBytes);
  let n = 0;
  let at = offset;
  while (n < rowBytes) {
    const b = vis[at++];
    if (b === undefined) return null;
    if (b !== 0) {
      out[n++] = b;
      continue;
    }
    const run = vis[at++];
    if (run === undefined || run === 0) return null;
    n += Math.min(run, rowBytes - n); // já zerado
  }
  return out;
}

export class Pvs {
  private readonly vis: Uint8Array;
  private readonly leafCount: number;
  private readonly cache = new Map<number, Uint8Array | null>();

  constructor(private readonly d: PvsData) {
    this.vis = base64ToBytes(d.visibility);
    this.leafCount = Math.floor(d.leaves.length / 4);
  }

  /** Folha que contém o ponto (GoldSrc), ou -1 se a árvore não resolve. */
  leafAt(x: number, y: number, z: number): number {
    const { nodes, planes } = this.d;
    const nodeCount = Math.floor(nodes.length / 3);
    let at = this.d.headnode;
    for (let step = 0; step <= nodeCount; step++) {
      if (at < 0) {
        const leaf = -1 - at;
        return leaf < this.leafCount ? leaf : -1;
      }
      if (at >= nodeCount) return -1;
      const plane = nodes[at * 3]!;
      const p = plane * 4;
      if (p + 3 >= planes.length || plane < 0) return -1;
      const dist = planes[p]! * x + planes[p + 1]! * y + planes[p + 2]! * z - planes[p + 3]!;
      at = nodes[at * 3 + (dist < 0 ? 2 : 1)]!;
    }
    return -1;
  }

  /** `contents` da folha (-2 = sólido). */
  contentsOf(leaf: number): number {
    return this.d.leaves[leaf * 4] ?? -2;
  }

  /**
   * Flags por face do mundo (1 = visível) para quem está na folha `leaf`, ou `null`
   * quando não há vis para ela (tudo visível). Resultado memorizado por folha.
   */
  facesVisibleFrom(leaf: number): Uint8Array | null {
    if (leaf < 0) return null;
    if (this.cache.has(leaf)) return this.cache.get(leaf)!;
    const flags = this.compute(leaf);
    if (this.cache.size > 96) this.cache.delete(this.cache.keys().next().value as number);
    this.cache.set(leaf, flags);
    return flags;
  }

  private compute(leaf: number): Uint8Array | null {
    const { leaves, marksurfaces, visleafs, world_face_count: worldFaces, world_first_face: first } = this.d;
    const visofs = leaves[leaf * 4 + 1]!;
    if (leaf === 0 || visofs < 0 || visleafs <= 0 || this.vis.length === 0) return null;
    if (visleafs + 1 > this.leafCount) return null;
    const row = decompressVis(this.vis, visofs, Math.ceil(visleafs / 8));
    if (!row) return null;

    const flags = new Uint8Array(Math.max(0, worldFaces));
    const mark = (l: number) => {
      const start = leaves[l * 4 + 2]!;
      const count = leaves[l * 4 + 3]!;
      for (let i = 0; i < count; i++) {
        const face = marksurfaces[start + i];
        if (face === undefined) continue;
        const rel = face - first;
        if (rel >= 0 && rel < flags.length) flags[rel] = 1;
      }
    };
    mark(leaf);
    for (let i = 0; i < visleafs; i++) {
      if (row[i >> 3]! & (1 << (i & 7))) mark(i + 1);
    }
    return flags;
  }

  /** Predicado de visibilidade por face do BSP (faces fora do modelo 0 sempre visíveis). */
  predicate(flags: Uint8Array | null): ((face: number) => boolean) | null {
    if (!flags) return null;
    const first = this.d.world_first_face;
    return (face) => {
      const rel = face - first;
      return rel < 0 || rel >= flags.length || flags[rel] === 1;
    };
  }
}
