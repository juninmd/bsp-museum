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
    },
    cellSize: cellSizeFor(extent),
  };
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
