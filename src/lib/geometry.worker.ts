/// <reference lib="webworker" />
import { buildChunks, type Chunk, type MeshInput } from "./geometry.ts";

interface Request {
  id: number;
  input: MeshInput;
  cellSize: number;
}

self.onmessage = (event: MessageEvent<Request>) => {
  const { id, input, cellSize } = event.data;
  try {
    const chunks = buildChunks(input, cellSize);
    const transfer: Transferable[] = [];
    for (const c of chunks) {
      transfer.push(c.positions.buffer, c.uvs.buffer, c.colors.buffer, c.triFace.buffer);
      if (c.lmUvs.length) transfer.push(c.lmUvs.buffer);
    }
    (self as unknown as Worker).postMessage({ id, chunks }, transfer);
  } catch (error) {
    (self as unknown as Worker).postMessage({ id, error: String(error) });
  }
};

export type { Chunk };
