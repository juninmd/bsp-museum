import { describe, expect, test } from "bun:test";
import { buildChunks, cellSizeFor, heightRgb, toWorld, visibleIndices, type MeshInput } from "./geometry.ts";

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
