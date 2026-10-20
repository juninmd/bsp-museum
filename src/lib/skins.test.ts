import { describe, expect, test } from "bun:test";
import {
  alphaToGray,
  changedTextures,
  clampScale,
  distinctFamilies,
  familyMap,
  fitView,
  flagKeys,
  hasTransparency,
  hitTestUv,
  hitTestUvAll,
  identityMap,
  modelKey,
  panBy,
  pixelToUv,
  skinTexindex,
  textureFileName,
  trianglesByTexture,
  triTexture,
  uvInTriangle,
  uvTolerance,
  uvToPixel,
  zoomAt,
  UV_MAX_SCALE,
  UV_MIN_SCALE,
} from "./skins.ts";
import type { TextureInfo } from "../types/mdl-extra.ts";

// Mesma tabela do modelo rico (src-tauri/src/mdl/tests_extra.rs): família 0 = [0,1], família 1 = [1,0].
const families = [
  [0, 1],
  [1, 0],
];

describe("família de skin -> textura", () => {
  test("a tabela da família escolhe a textura de cada triângulo", () => {
    const texindex = [0, 1, 1, 0];
    expect([...skinTexindex(texindex, families[0]!)]).toEqual([0, 1, 1, 0]);
    expect([...skinTexindex(texindex, families[1]!)]).toEqual([1, 0, 0, 1]);
    expect(triTexture(texindex, families[1]!, 0)).toBe(1);
    expect(triTexture(texindex, families[1]!, 99)).toBe(-1);
  });

  test("índice fora da tabela mantém a textura de origem", () => {
    expect([...skinTexindex([5, 0], [3])]).toEqual([5, 3]);
  });

  test("família fora da faixa vale 0 e sem tabela vale a identidade", () => {
    expect(familyMap(families, 1, 2)).toEqual([1, 0]);
    expect(familyMap(families, 7, 2)).toEqual([0, 1]);
    expect(familyMap(families, -1, 2)).toEqual([0, 1]);
    expect(familyMap([], 0, 3)).toEqual([0, 1, 2]);
    expect(identityMap(0)).toEqual([]);
  });

  test("agrupa os triângulos por textura efetiva, em ordem de textura", () => {
    const texindex = [0, 1, 1, 0, 1];
    expect([...trianglesByTexture(texindex, families[0]!)]).toEqual([
      [0, [0, 3]],
      [1, [1, 2, 4]],
    ]);
    expect([...trianglesByTexture(texindex, families[1]!)]).toEqual([
      [0, [1, 2, 4]],
      [1, [0, 3]],
    ]);
  });

  test("famílias repetidas não ganham miniatura própria", () => {
    expect(distinctFamilies([[0, 1], [1, 0], [0, 1], [1, 0], [1, 1]])).toEqual([0, 1, 4]);
    expect(distinctFamilies([])).toEqual([]);
  });

  test("lista as texturas que a família troca em relação à 0", () => {
    expect(changedTextures(families, 0)).toEqual([]);
    expect(changedTextures(families, 1)).toEqual([0, 1]);
    expect(changedTextures([[0, 1, 2], [0, 3, 2]], 1)).toEqual([1]);
  });
});

describe("flags de material", () => {
  const info = (over: Partial<TextureInfo>): TextureInfo => ({
    name: "t", flags: 0, flatshade: false, chrome: false, fullbright: false, nomips: false, alpha: false, additive: false, masked: false,
    blend: "opaque", remap: null, ...over,
  });

  test("ordem fixa, só as ligadas", () => {
    expect(flagKeys(undefined)).toEqual([]);
    expect(flagKeys(info({}))).toEqual([]);
    expect(flagKeys(info({ masked: true, chrome: true, flatshade: true }))).toEqual(["chrome", "masked", "flatshade"]);
    expect(flagKeys(info({ remap: { top_start: 160, top_end: 191, bottom_end: 223 }, additive: true }))).toEqual(["additive", "remap"]);
  });
});

describe("chave de cache do modelo", () => {
  test("distingue corpo e cores de time", () => {
    expect(modelKey(0, null, null)).toBe("0|-|-");
    expect(modelKey(1, 140, 0)).toBe("1|140|0");
    expect(modelKey(0, 0, null)).not.toBe(modelKey(0, null, 0));
  });
});

describe("projeção UV <-> pixel", () => {
  const view = { ox: 10, oy: 20, scale: 2 };

  test("UV -> pixel considera o tamanho da textura e o zoom", () => {
    expect(uvToPixel(0, 0, 64, 32, view)).toEqual([10, 20]);
    expect(uvToPixel(1, 1, 64, 32, view)).toEqual([138, 84]);
    expect(uvToPixel(0.5, 0.25, 64, 32, view)).toEqual([74, 36]);
  });

  test("pixel -> UV é o inverso", () => {
    const [px, py] = uvToPixel(0.3, 0.7, 128, 64, view);
    const [u, v] = pixelToUv(px, py, 128, 64, view);
    expect(u).toBeCloseTo(0.3, 9);
    expect(v).toBeCloseTo(0.7, 9);
  });

  test("enquadra a textura inteira, centrada", () => {
    const fit = fitView(100, 50, 400, 300, 0);
    expect(fit.scale).toBe(4);
    expect(fit.ox).toBe(0);
    expect(fit.oy).toBe(50);
    const small = fitView(64, 64, 200, 100, 10);
    expect(small.scale).toBeCloseTo(80 / 64, 9);
    expect(small.ox + 64 * small.scale / 2).toBeCloseTo(100, 9);
    expect(small.oy + 64 * small.scale / 2).toBeCloseTo(50, 9);
  });

  test("o zoom mantém parado o ponto sob o cursor", () => {
    const px = 70;
    const py = 50;
    const before = pixelToUv(px, py, 64, 64, view);
    const zoomed = zoomAt(view, 1.5, px, py);
    expect(zoomed.scale).toBeCloseTo(3, 9);
    const after = pixelToUv(px, py, 64, 64, zoomed);
    expect(after[0]).toBeCloseTo(before[0], 9);
    expect(after[1]).toBeCloseTo(before[1], 9);
  });

  test("zoom tem limites e o pan só desloca", () => {
    expect(zoomAt(view, 1e9, 0, 0).scale).toBe(UV_MAX_SCALE);
    expect(zoomAt(view, 1e-9, 0, 0).scale).toBe(UV_MIN_SCALE);
    expect(clampScale(1)).toBe(1);
    expect(panBy(view, 5, -3)).toEqual({ ox: 15, oy: 17, scale: 2 });
  });

  test("tolerância em UV encolhe com o zoom", () => {
    expect(uvTolerance(4, 64, 128, { ox: 0, oy: 0, scale: 1 })).toBeCloseTo(4 / 64, 9);
    expect(uvTolerance(4, 64, 128, { ox: 0, oy: 0, scale: 4 })).toBeCloseTo(1 / 64, 9);
  });
});

describe("acerto de triângulo no mapa UV", () => {
  // tri 0: canto superior esquerdo; tri 1: o outro lado do quadrado; tri 2 degenerado
  const uvs = [0, 0, 1, 0, 0, 1, /* 1 */ 1, 1, 0, 1, 1, 0, /* 2 */ 0.5, 0.5, 0.5, 0.5, 0.5, 0.5];

  test("ponto dentro, fora e nas bordas", () => {
    expect(uvInTriangle(uvs, 0, 0.2, 0.2)).toBe(true);
    expect(uvInTriangle(uvs, 0, 0.8, 0.8)).toBe(false);
    expect(uvInTriangle(uvs, 1, 0.8, 0.8)).toBe(true);
    expect(uvInTriangle(uvs, 0, 0, 0)).toBe(true); // vértice
    expect(uvInTriangle(uvs, 0, 0.5, 0.5)).toBe(true); // hipotenusa
    expect(uvInTriangle(uvs, 0, 0.51, 0.51)).toBe(false);
    expect(uvInTriangle(uvs, 0, 0.51, 0.51, 0.05)).toBe(true); // com folga
  });

  test("triângulo degenerado e índice inexistente nunca acertam", () => {
    expect(uvInTriangle(uvs, 2, 0.5, 0.5)).toBe(false);
    expect(uvInTriangle(uvs, 9, 0.5, 0.5)).toBe(false);
  });

  test("hit-test entre candidatos: só os da lista, o último vence", () => {
    expect(hitTestUv(uvs, [0, 1], 0.2, 0.2)).toBe(0);
    expect(hitTestUv(uvs, [0, 1], 0.8, 0.8)).toBe(1);
    expect(hitTestUv(uvs, [1], 0.2, 0.2)).toBe(-1);
    expect(hitTestUv(uvs, [], 0.2, 0.2)).toBe(-1);
    expect(hitTestUv(uvs, [0, 1], 0.5, 0.5)).toBe(1); // aresta compartilhada: o de cima
    expect(hitTestUvAll(uvs, [0, 1, 2], 0.5, 0.5)).toEqual([0, 1]);
    expect(hitTestUvAll(uvs, [0, 1], 2, 2)).toEqual([]);
  });

  test("clique em pixel passa pela projeção e acerta o triângulo", () => {
    const view = fitView(64, 64, 256, 256, 0);
    const [px, py] = uvToPixel(0.25, 0.25, 64, 64, view);
    const [u, v] = pixelToUv(px, py, 64, 64, view);
    expect(hitTestUv(uvs, [0, 1], u, v)).toBe(0);
  });
});

describe("canal alpha e exportação", () => {
  test("alpha vira cinza opaco", () => {
    const px = new Uint8ClampedArray([10, 20, 30, 200, 0, 0, 0, 0]);
    alphaToGray(px);
    expect([...px]).toEqual([200, 200, 200, 255, 0, 0, 0, 255]);
  });

  test("detecta transparência", () => {
    expect(hasTransparency(new Uint8ClampedArray([1, 2, 3, 255, 4, 5, 6, 255]))).toBe(false);
    expect(hasTransparency(new Uint8ClampedArray([1, 2, 3, 255, 4, 5, 6, 0]))).toBe(true);
    expect(hasTransparency(new Uint8ClampedArray([]))).toBe(false);
  });

  test("nome de arquivo seguro", () => {
    expect(textureFileName("Remap1_160_191_223.bmp")).toBe("Remap1_160_191_223.png");
    expect(textureFileName("a/b c:d*.BMP")).toBe("a_b_c_d.png");
    expect(textureFileName("   ")).toBe("textura.png");
    expect(textureFileName("", "x")).toBe("x.png");
    expect(textureFileName("pele.verde")).toBe("pele.verde.png");
  });
});
