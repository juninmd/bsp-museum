import { describe, expect, test } from "bun:test";
import {
  BoundedCache,
  blendAxes,
  blendValue,
  bodyChoices,
  controllerByte,
  controllerSliderMax,
  controllerValue,
  controllersArg,
  debounce,
  defaultBlendByte,
  formatControllerValue,
  framePercent,
  hitGroupColor,
  hueColor,
  lerpBoxes,
  materialPlan,
  restControllerBytes,
  seqCacheKey,
  sliderToControllerByte,
} from "./mdl-controls.ts";
import { blendByte, bodyChoice, bodyValue, type BodyPartInfo, type ControllerInfo, type SeqDetail, type TextureInfo } from "../types/mdl-extra.ts";

// Os mesmos controladores do modelo rico de `src-tauri/src/mdl/tests_extra.rs`.
const base: ControllerInfo = {
  index: 0, name: "controller0", bone: 0, kind: "x", wrap: false, start: 0, end: 10, rest: 0, input_max: 255, unit: "units", raw_type: 1,
};
const mouth: ControllerInfo = { ...base, index: 4, name: "mouth", bone: 1, kind: "zr", start: 0, end: 45, input_max: 64, unit: "deg", raw_type: 0x20 };
const wrap: ControllerInfo = { ...base, index: 1, name: "controller1", bone: 1, kind: "yr", wrap: true, start: 0, end: 360, unit: "deg", raw_type: 0x8010 };

describe("controladores", () => {
  test("byte -> valor, como o backend", () => {
    expect([controllerValue(base, 0), controllerValue(base, 255)]).toEqual([0, 10]);
    expect(controllerValue(base, 128)).toBeCloseTo(5.0196, 3);
    expect([controllerValue(mouth, 0), controllerValue(mouth, 64), controllerValue(mouth, 255)]).toEqual([0, 45, 45]);
    expect(controllerValue(mouth, 32)).toBe(22.5);
    expect([controllerValue(wrap, 0), controllerValue(wrap, 64), controllerValue(wrap, 128)]).toEqual([0, 90, 180]);
  });

  test("valor -> byte, como o backend", () => {
    expect([controllerByte(base, 0), controllerByte(base, 10), controllerByte(base, 99), controllerByte(base, -5)]).toEqual([0, 255, 255, 0]);
    expect([controllerByte(mouth, 45), controllerByte(mouth, 22.5), controllerByte(mouth, 0)]).toEqual([64, 32, 0]);
    expect([controllerByte(wrap, 90), controllerByte(wrap, -90), controllerByte(wrap, 360)]).toEqual([64, 192, 0]);
    for (const b of [0, 1, 50, 128, 200, 255]) expect(Math.abs(controllerByte(base, controllerValue(base, b)) - b)).toBeLessThanOrEqual(1);
    // faixa nula não divide por zero; faixa invertida (end < start) funciona
    expect(controllerByte({ ...base, start: 3, end: 3 }, 3)).toBe(0);
    const inv = { ...base, start: 10, end: 0 };
    expect([controllerValue(inv, 0), controllerValue(inv, 255), controllerByte(inv, 0)]).toEqual([10, 0, 255]);
  });

  test("slider: limita ao último byte que ainda muda algo", () => {
    expect(controllerSliderMax(base)).toBe(255);
    expect(controllerSliderMax(mouth)).toBe(64);
    expect(controllerSliderMax(wrap)).toBe(255);
    expect([sliderToControllerByte(mouth, 200), sliderToControllerByte(mouth, -3), sliderToControllerByte(base, 127.6)]).toEqual([64, 0, 128]);
  });

  test("rótulo com unidade", () => {
    expect(formatControllerValue(mouth, 64)).toBe("45°");
    expect(formatControllerValue(base, 255)).toBe("10");
    expect(formatControllerValue(wrap, 0)).toBe("0°");
    expect(formatControllerValue(base, 128)).toBe("5");
  });

  test("controllersArg só existe quando algo saiu do repouso", () => {
    const infos = [base, mouth, wrap];
    expect(restControllerBytes(infos)).toEqual([0, 0, 0, 0, 0]);
    expect(controllersArg(infos, [0, 0, 0, 0, 0])).toBeUndefined();
    expect(controllersArg(infos, [0, 0, 0, 0, 40])).toEqual([0, 0, 0, 0, 40]);
    // slot sem controlador não vai com lixo
    expect(controllersArg(infos, [0, 0, 77, 0, 40])).toEqual([0, 0, 0, 0, 40]);
    // repouso diferente de zero conta como "sem mudança"
    const shifted = [{ ...base, rest: 128 }];
    expect(controllersArg(shifted, [128, 0, 0, 0, 0])).toBeUndefined();
    expect(controllersArg(shifted, [10, 0, 0, 0, 0])).toEqual([10, 0, 0, 0, 0]);
  });
});

const detail = (over: Partial<SeqDetail> = {}): SeqDetail => ({
  flags: 0, activity: 0, act_weight: 0, motion_type: 0, motion_bone: 0, linear_movement: [0, 0, 0],
  blend_axis: ["yr", ""], blend_type: [0x10, 0], blend_start: [-45, 0], blend_end: [45, 0], blend_parent: 0, next_seq: 0, events: [], ...over,
});

describe("blends", () => {
  test("eixos: nenhum, um, dois e genérico", () => {
    expect(blendAxes(1, detail())).toEqual([]);
    expect(blendAxes(2, detail()).map((a) => a.which)).toEqual([0]);
    const two = detail({ blend_axis: ["zr", "yr"], blend_type: [0x20, 0x10], blend_start: [-30, -60], blend_end: [30, 60] });
    expect(blendAxes(9, two).map((a) => [a.which, a.axis])).toEqual([[0, "zr"], [1, "yr"]]);
    // vários blends sem eixo declarado: slider cru 0..255
    expect(blendAxes(4, detail({ blend_type: [0, 0] }))).toEqual([{ which: 0, axis: "", start: 0, end: 255 }]);
    expect(blendAxes(4, undefined)).toHaveLength(1);
  });

  test("byte inicial: centro quando a faixa cruza o zero", () => {
    const a = blendAxes(2, detail())[0]!;
    expect(defaultBlendByte(detail(), a)).toBe(127);
    expect(blendValue(a, 127)).toBeCloseTo(-0.18, 1);
    expect(blendValue(a, 0)).toBe(-45);
    expect(blendValue(a, 255)).toBe(45);
    const pos = blendAxes(2, detail({ blend_start: [0, 0], blend_end: [180, 0] }))[0]!;
    expect(defaultBlendByte(detail({ blend_start: [0, 0], blend_end: [180, 0] }), pos)).toBe(0);
  });

  test("blendByte do contrato continua batendo com o rótulo", () => {
    const d = detail();
    expect(blendByte(d, 0, -45)).toBe(0);
    expect(blendByte(d, 0, 45)).toBe(255);
    expect(blendByte(d, 0, 0)).toBe(127);
  });
});

describe("cache de quadros", () => {
  test("a chave separa sequência, blend, controladores e hitboxes", () => {
    const k = (...a: Parameters<typeof seqCacheKey>) => seqCacheKey(...a);
    expect(k(1, undefined, undefined, false)).toBe(k(1, [0, 0], undefined, false));
    const keys = new Set([
      k(1, [0, 0], undefined, false),
      k(2, [0, 0], undefined, false),
      k(1, [10, 0], undefined, false),
      k(1, [0, 10], undefined, false),
      k(1, [0, 0], [0, 0, 0, 0, 5], false),
      k(1, [0, 0], undefined, true),
    ]);
    expect(keys.size).toBe(6);
  });

  test("teto: descarta a entrada mais antiga", () => {
    const c = new BoundedCache<number>(2);
    c.set("a", 1);
    c.set("b", 2);
    c.set("a", 10); // reinserir renova
    c.set("c", 3);
    expect([c.get("a"), c.get("b"), c.get("c"), c.size]).toEqual([10, undefined, 3, 2]);
  });
});

describe("bodygroups", () => {
  const parts: BodyPartInfo[] = [
    { name: "body", base: 1, models: ["a", "b"] },
    { name: "gun", base: 2, models: ["g", "h", "i"] },
  ];
  test("valor <-> escolhas", () => {
    expect(bodyValue(parts, [0, 0])).toBe(0);
    expect(bodyValue(parts, [1, 0])).toBe(1);
    expect(bodyValue(parts, [1, 2])).toBe(5);
    expect(bodyValue(parts, [9, 9])).toBe(1 + 4); // limita à última opção
    for (let body = 0; body < 6; body++) expect(bodyValue(parts, bodyChoices(parts, body))).toBe(body);
    expect(bodyChoices(parts, 5)).toEqual([1, 2]);
    expect(bodyChoice(parts[1]!, 5)).toBe(2);
    expect(bodyChoices([{ base: 1, models: [] }], 3)).toEqual([0]);
  });
});

describe("material", () => {
  const info = (over: Partial<TextureInfo>): TextureInfo => ({
    name: "t", flags: 0, flatshade: false, chrome: false, fullbright: false, nomips: false, alpha: false, additive: false, masked: false,
    blend: "opaque", remap: null, ...over,
  });
  test("sem info: iluminado com máscara (como era antes)", () => {
    expect(materialPlan(undefined)).toEqual({ shade: "lit", flat: false, blend: "masked" });
  });
  test("cada flag escolhe um caminho", () => {
    expect(materialPlan(info({}))).toEqual({ shade: "lit", flat: false, blend: "opaque" });
    expect(materialPlan(info({ chrome: true })).shade).toBe("matcap");
    expect(materialPlan(info({ fullbright: true })).shade).toBe("basic");
    expect(materialPlan(info({ chrome: true, fullbright: true })).shade).toBe("matcap");
    expect(materialPlan(info({ flatshade: true })).flat).toBe(true);
    expect(materialPlan(info({ masked: true })).blend).toBe("masked");
    expect(materialPlan(info({ masked: true, additive: true })).blend).toBe("additive"); // aditivo vence máscara
  });
});

describe("cores e quadros", () => {
  test("amostra do matiz", () => {
    expect(hueColor(0)).toBe("hsl(0 85% 50%)");
    expect(hueColor(255)).toBe("hsl(360 85% 50%)");
    expect(hueColor(999)).toBe(hueColor(255));
  });
  test("cor do grupo de hitbox é estável e distinta para os 8 primeiros", () => {
    expect(new Set([0, 1, 2, 3, 4, 5, 6, 7].map(hitGroupColor)).size).toBe(8);
    expect(hitGroupColor(12)).toBe(hitGroupColor(12));
    expect(hitGroupColor(12)).toBeLessThanOrEqual(0xffffff);
  });
  test("marcador na barra do tempo", () => {
    expect([framePercent(0, 11), framePercent(5, 11), framePercent(10, 11), framePercent(3, 1), framePercent(99, 11)]).toEqual([0, 50, 100, 0, 100]);
  });
});

describe("caixas por quadro", () => {
  // 2 quadros, 1 caixa: x do centro 0 -> 10; rotação identidade -> 90° em Z
  const s = Math.SQRT1_2;
  const src = [0, 0, 0, 1, 1, 1, 0, 0, 0, 1, /* quadro 1 */ 10, 0, 0, 3, 1, 1, 0, 0, s, s];
  test("interpola centro, meias-arestas e quaternion", () => {
    const out = new Float32Array(10);
    lerpBoxes(src, 2, 1, 0.5, out);
    expect(out[0]).toBeCloseTo(5);
    expect(out[3]).toBeCloseTo(2);
    const q = [...out.slice(6)];
    expect(Math.hypot(...q)).toBeCloseTo(1);
    expect(q[2]).toBeGreaterThan(0.3); // meio caminho de 0 a 90° em Z
    lerpBoxes(src, 2, 1, 1, out);
    expect(out[0]).toBe(10);
    lerpBoxes(src, 2, 1, 7, out); // além do fim: último quadro
    expect(out[0]).toBe(10);
  });
  test("quaternion pelo lado mais curto", () => {
    const flip = [0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, -1];
    const out = new Float32Array(10);
    lerpBoxes(flip, 2, 1, 0.5, out);
    expect(Math.abs(out[9]!)).toBeCloseTo(1);
  });
  test("sem quadros ou caixas não mexe na saída", () => {
    const out = new Float32Array(10).fill(7);
    lerpBoxes([], 0, 1, 0, out);
    lerpBoxes(src, 2, 0, 0, out);
    expect(out[0]).toBe(7);
  });
});

describe("debounce", () => {
  test("só a última chamada dispara", async () => {
    const calls: number[] = [];
    const d = debounce((n: number) => calls.push(n), 15);
    d(1);
    d(2);
    d(3);
    await new Promise((r) => setTimeout(r, 50));
    expect(calls).toEqual([3]);
    d(4);
    d.cancel();
    await new Promise((r) => setTimeout(r, 40));
    expect(calls).toEqual([3]);
  });
});
