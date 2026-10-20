import type { MeshDetail } from "../types.ts";
import { buildChunks, cellSizeFor, type Chunk, type MeshInput } from "./geometry.ts";

/** Entrada do chunker a partir da malha que o backend entrega. */
export function inputFromMesh(mesh: MeshDetail): { input: MeshInput; cellSize: number } {
  const b = mesh.bounds;
  const zMin = b ? b.mins[2] : 0;
  const zMax = b ? b.maxs[2] : 1;
  const extent = b ? Math.max(b.size[0], b.size[1]) : 4096;
  return {
    input: {
      positions: mesh.positions,
      uvs: mesh.uvs,
      lmUvs: mesh.lm_uvs,
      texindex: mesh.texindex,
      triFace: mesh.tri_face,
      zMin,
      zMax,
      faceModel: faceModelOf(mesh),
      faceStyles: mesh.face_styles?.length ? Uint8Array.from(mesh.face_styles) : undefined,
    },
    cellSize: cellSizeFor(extent),
  };
}

/** Modelo (brush entity) de cada face do BSP: 0 = mundo. Vazio se o mapa não tem brush entity. */
export function faceModelOf(mesh: MeshDetail): Int32Array | undefined {
  const models = mesh.brush_models;
  if (!models?.length) return undefined;
  let maxFace = -1;
  for (const f of mesh.tri_face) if (f > maxFace) maxFace = f;
  for (const m of models) maxFace = Math.max(maxFace, m.first_face + m.num_faces - 1);
  const out = new Int32Array(maxFace + 1);
  for (const m of models) {
    const end = Math.min(m.first_face + m.num_faces, out.length);
    for (let f = Math.max(0, m.first_face); f < end; f++) out[f] = m.model;
  }
  return out;
}

let worker: Worker | null = null;
let nextId = 1;

function getWorker(): Worker | null {
  if (typeof Worker === "undefined") return null;
  if (worker) return worker;
  try {
    worker = new Worker(new URL("./geometry.worker.ts", import.meta.url), { type: "module" });
  } catch {
    worker = null;
  }
  return worker;
}

/**
 * Monta os chunks fora da thread da UI (Web Worker). Se o worker não existir ou
 * falhar (CSP, ambiente de teste), cai no cálculo síncrono — mesmo resultado.
 */
export function buildChunksAsync(mesh: MeshDetail): Promise<Chunk[]> {
  const { input, cellSize } = inputFromMesh(mesh);
  const w = getWorker();
  if (!w) return Promise.resolve(buildChunks(input, cellSize));

  return new Promise((resolve) => {
    const id = nextId++;
    const fallback = () => resolve(buildChunks(input, cellSize));
    const onMessage = (e: MessageEvent<{ id: number; chunks?: Chunk[]; error?: string }>) => {
      if (e.data.id !== id) return;
      w.removeEventListener("message", onMessage);
      w.removeEventListener("error", onError);
      if (e.data.chunks) resolve(e.data.chunks);
      else fallback();
    };
    const onError = () => {
      w.removeEventListener("message", onMessage);
      w.removeEventListener("error", onError);
      worker = null;
      fallback();
    };
    w.addEventListener("message", onMessage);
    w.addEventListener("error", onError);
    w.postMessage({ id, input, cellSize });
  });
}
