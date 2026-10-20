import { describe, expect, test } from "bun:test";
import { buildChunks, cellSizeFor, heightRgb, isAnimatedFace, toWorld, visibleIndices, type MeshInput } from "./geometry.ts";
import { faceModelOf } from "./chunker.ts";
import type { MeshDetail } from "../types.ts";

/** dois triângulos: um perto da origem (textura 0) e outro longe (textura 0), mais um da textura 1 */
function input(): MeshInput {
  const tri = (x: number, y: number, z: number) => [x, y, z, x + 10, y, z, x, y + 10, z + 5];
  return {
    positions: [...tri(0, 0, 0), ...tri(5000, 0, 100), ...tri(0, 0, 0)],
    uvs: new Array(18).fill(0.5),
    lmUvs: [],
    texindex: [0, 0, 1],
    triFace: [7, 8, -1],
    zMin: 0,
    zMax: 100,
  };
}

describe("buildChunks", () => {
  test("separa por textura e por célula do mapa", () => {
    const chunks = buildChunks(input(), 1024);
    expect(chunks.length).toBe(3); // tex0 perto, tex0 longe, tex1
    expect(chunks.reduce((n, c) => n + c.triFace.length, 0)).toBe(3);
  });

  test("converte Z-up para Y-up", () => {
    expect(toWorld(1, 2, 3)).toEqual([1, 3, -2]);
    const near = buildChunks(input(), 1024).find((c) => c.textureIndex === 1)!;
    expect([...near.positions.slice(0, 3)]).toEqual([0, 0, -0]);
    expect(near.positions[7]).toBe(5); // z do mapa vira y
  });

  test("guarda a face de cada triângulo e -1 para prop", () => {
    const chunks = buildChunks(input(), 1024);
    expect(chunks.find((c) => c.textureIndex === 1)!.triFace[0]).toBe(-1);
    expect(chunks.flatMap((c) => [...c.triFace]).sort()).toEqual([-1, 7, 8].sort());
  });

  test("esfera envolvente contém os vértices", () => {
    for (const c of buildChunks(input(), 1024)) {
      for (let v = 0; v < c.positions.length; v += 3) {
        const d = Math.hypot(c.positions[v]! - c.center[0], c.positions[v + 1]! - c.center[1], c.positions[v + 2]! - c.center[2]);
        expect(d).toBeLessThanOrEqual(c.radius + 1e-3);
      }
    }
  });

  test("uv de lightmap só vai junto quando existe para todos os triângulos", () => {
    expect(buildChunks(input(), 1024)[0]!.lmUvs.length).toBe(0);
    const withLm = buildChunks({ ...input(), lmUvs: new Array(18).fill(0.25) }, 1024);
    expect(withLm.every((c) => c.lmUvs.length === c.triFace.length * 6)).toBe(true);
  });

  test("malha vazia não gera chunk", () => {
    expect(buildChunks({ ...input(), texindex: [], triFace: [], positions: [], uvs: [] }, 1024)).toEqual([]);
  });
});

describe("brush entities e luz animada", () => {
  test("brush entity vira chunk próprio (textura × modelo), sem entrar na grade do mundo", () => {
    // faces 7 e 8 são do mundo; a 9 é do modelo 2 e usa a mesma textura 0, na mesma célula
    const base = input();
    const withModel: MeshInput = { ...base, triFace: [7, 8, 9], texindex: [0, 0, 0], faceModel: [0, 0, 0, 0, 0, 0, 0, 0, 0, 2] };
    const chunks = buildChunks(withModel, 1024);
    const entity = chunks.filter((c) => c.model === 2);
    expect(entity).toHaveLength(1);
    expect(entity[0]!.triFace).toEqual(Int32Array.of(9));
    // o triângulo do mundo perto da origem não se mistura com o da entidade
    expect(chunks.filter((c) => c.model === 0).reduce((n, c) => n + c.triFace.length, 0)).toBe(2);
  });

  test("dois triângulos da mesma entidade e textura ficam juntos, longe um do outro", () => {
    const base = input();
    const chunks = buildChunks({ ...base, triFace: [3, 3, 3], texindex: [0, 0, 0], faceModel: [0, 0, 0, 5] }, 1024);
    expect(chunks.filter((c) => c.model === 5)).toHaveLength(1);
    expect(chunks[0]!.triFace.length).toBe(3);
  });

  test("prop (-1) e mapa sem faceModel ficam no mundo", () => {
    expect(buildChunks(input(), 1024).every((c) => c.model === 0)).toBe(true);
  });

  test("faceModelOf espalha a faixa de faces de cada brush entity", () => {
    const mesh = {
      tri_face: [0, 1, 4],
      brush_models: [
        { model: 3, first_face: 2, num_faces: 2 },
        { model: 4, first_face: 4, num_faces: 1 },
      ],
    } as unknown as MeshDetail;
    expect([...faceModelOf(mesh)!]).toEqual([0, 0, 3, 3, 4]);
    expect(faceModelOf({ tri_face: [0], brush_models: [] } as unknown as MeshDetail)).toBeUndefined();
  });

  test("só faces com estilo diferente do 0 (ou camadas extras) são animadas", () => {
    const styles = [0, 255, 255, 255, 0, 1, 255, 255, 3, 255, 255, 255, 255, 255, 255, 255];
    expect([0, 1, 2, 3, -1, 9].map((f) => isAnimatedFace(styles, f))).toEqual([false, true, true, false, false, false]);
  });

  test("chunk com luz animada traz 4 estilos por vértice; os outros não pagam o buffer", () => {
    const styles = [0, 255, 255, 255, 0, 1, 255, 255];
    const base: MeshInput = {
      positions: new Array(18).fill(0).map((_, i) => i),
      uvs: new Array(12).fill(0),
      lmUvs: [],
      texindex: [0, 1],
      triFace: [0, 1],
      zMin: 0,
      zMax: 1,
      faceStyles: styles,
    };
    const chunks = buildChunks(base, 1 << 20);
    const plain = chunks.find((c) => c.textureIndex === 0)!;
    const lit = chunks.find((c) => c.textureIndex === 1)!;
    expect(plain.styles.length).toBe(0);
    expect(lit.styles.length).toBe(12);
    expect([...lit.styles.slice(0, 4)]).toEqual([0, 1, 255, 255]);
    expect([...lit.styles.slice(8, 12)]).toEqual([0, 1, 255, 255]); // o 3º vértice repete
  });

  test("no chunk animado, triângulo sem estilo próprio (prop) usa só o estilo 0", () => {
    const base: MeshInput = {
      positions: new Array(18).fill(0).map((_, i) => i),
      uvs: new Array(12).fill(0),
      lmUvs: [],
      texindex: [0, 0],
      triFace: [0, -1],
      zMin: 0,
      zMax: 1,
      faceStyles: [2, 255, 255, 255],
    };
    const [chunk] = buildChunks(base, 1 << 20);
    expect([...chunk!.styles.slice(0, 4)]).toEqual([2, 255, 255, 255]);
    expect([...chunk!.styles.slice(12, 16)]).toEqual([0, 255, 255, 255]);
  });
});

describe("visibleIndices", () => {
  test("sem predicado liga todos os triângulos", () => {
    const out = new Uint32Array(9);
    expect(visibleIndices(Int32Array.of(1, 2, 3), null, out)).toBe(9);
    expect([...out]).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
  });
  test("predicado esconde faces, mas prop (-1) sempre fica", () => {
    const out = new Uint32Array(9);
    const n = visibleIndices(Int32Array.of(1, 2, -1), (f) => f === 2, out);
    expect(n).toBe(6);
    expect([...out.slice(0, n)]).toEqual([3, 4, 5, 6, 7, 8]);
  });
});

describe("auxiliares", () => {
  test("célula cresce com o mapa mas nunca abaixo de 1024", () => {
    expect(cellSizeFor(100)).toBe(1024);
    expect(cellSizeFor(12000)).toBe(2000);
  });
  test("paleta por altura vai de frio a quente e satura nas pontas", () => {
    const low = heightRgb(-5);
    const high = heightRgb(9);
    expect(low[2]).toBeGreaterThan(low[0]);
    expect(high[0]).toBeGreaterThan(high[2]);
  });
});
