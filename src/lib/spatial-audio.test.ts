import { describe, expect, test } from "bun:test";
import { toWorld } from "./geometry.ts";
import {
  audibleInMix,
  BufferCache,
  dataUrlToBytes,
  distanceBetween,
  distanceGain,
  effectiveGain,
  gainToDb,
  inRange,
  loopRegion,
  meterFill,
  pannerParams,
  rmsOfBytes,
  selectNearest,
  soundHue,
  soundLabel,
  soundRel,
  sourcePosition,
  type MixState,
} from "./spatial-audio.ts";

const mix = (o: Partial<MixState> = {}): MixState => ({ muted: new Set(), solo: new Set(), preview: null, muteAll: false, ...o });

describe("coordenadas", () => {
  test("GoldSrc Z-up vira cena Y-up: (x, y, z) -> (x, z, -y)", () => {
    expect(sourcePosition({ origin: [128, 64, 32] })).toEqual([128, 32, -64]);
    expect(sourcePosition({ origin: [0, 0, 0] })).toEqual(toWorld(0, 0, 0));
  });

  test("a conversão preserva distâncias (rotação, não escala)", () => {
    const a: [number, number, number] = [10, -20, 30];
    const b: [number, number, number] = [-40, 5, 100];
    const d0 = distanceBetween(a, b);
    const d1 = distanceBetween(sourcePosition({ origin: a }), sourcePosition({ origin: b }));
    expect(d1).toBeCloseTo(d0, 9);
  });

  test("subir no mapa (+Z) sobe na cena (+Y); o norte do mapa (+Y) é -Z da cena", () => {
    expect(sourcePosition({ origin: [0, 0, 100] })[1]).toBe(100);
    expect(sourcePosition({ origin: [0, 100, 0] })[2]).toBe(-100);
  });
});

describe("atenuação por distância", () => {
  const p = pannerParams(1000);

  test("platô perto do emissor, zero no raio, linear no meio", () => {
    expect(distanceGain(0, p)).toBe(1);
    expect(distanceGain(p.refDistance, p)).toBe(1);
    expect(distanceGain(1000, p)).toBe(0);
    expect(distanceGain(5000, p)).toBe(0);
    const mid = (p.refDistance + p.maxDistance) / 2;
    expect(distanceGain(mid, p)).toBeCloseTo(0.5, 9);
  });

  test("nunca aumenta com a distância", () => {
    let prev = 2;
    for (let d = 0; d <= 1200; d += 25) {
      const g = distanceGain(d, p);
      expect(g).toBeLessThanOrEqual(prev);
      expect(g).toBeGreaterThanOrEqual(0);
      prev = g;
    }
  });

  test("o raio do backend vira maxDistance; refDistance fica abaixo e nunca é zero", () => {
    for (const r of [1, 5, 500, 1250]) {
      const q = pannerParams(r);
      expect(q.maxDistance).toBeGreaterThanOrEqual(2);
      expect(q.refDistance).toBeGreaterThanOrEqual(1);
      expect(q.refDistance).toBeLessThan(q.maxDistance);
    }
    expect(pannerParams(800).maxDistance).toBe(800);
  });

  test("alcance: sem raio toca em todo o mapa; com raio é aberto no limite", () => {
    expect(inRange(1e9, null)).toBe(true);
    expect(inRange(799, 800)).toBe(true);
    expect(inRange(800, 800)).toBe(false);
  });
});

describe("mapeamento de volume", () => {
  test("volume x atenuação x mestre", () => {
    const p = pannerParams(800);
    const g = effectiveGain({ volume: 0.5, distance: 400, radius: 800, master: 0.8 });
    expect(g).toBeCloseTo(0.5 * distanceGain(400, p) * 0.8, 9);
  });

  test("sem raio não atenua; silenciado zera; valores fora de 0..1 são presos", () => {
    expect(effectiveGain({ volume: 1, distance: 9999, radius: null, master: 1 })).toBe(1);
    expect(effectiveGain({ volume: 1, distance: 0, radius: 500, master: 1, silenced: true })).toBe(0);
    expect(effectiveGain({ volume: 3, distance: 0, radius: null, master: 2 })).toBe(1);
    expect(effectiveGain({ volume: Number.NaN, distance: 0, radius: null, master: 1 })).toBe(0);
  });

  test("decibéis e medidor", () => {
    expect(gainToDb(1)).toBe(0);
    expect(gainToDb(0.5)).toBeCloseTo(-6.02, 1);
    expect(gainToDb(0)).toBe(-120);
    expect(meterFill(0)).toBe(0);
    expect(meterFill(1)).toBe(1);
    expect(meterFill(0.001)).toBe(0); // -60 dB = piso
    expect(meterFill(0.1)).toBeCloseTo(2 / 3, 5); // -20 dB
  });

  test("RMS do bloco do analisador (128 = silêncio)", () => {
    expect(rmsOfBytes(new Uint8Array(64).fill(128))).toBe(0);
    expect(rmsOfBytes([])).toBe(0);
    const square = Uint8Array.from({ length: 64 }, (_, i) => (i % 2 ? 255 : 1));
    expect(rmsOfBytes(square)).toBeCloseTo(127 / 128, 5);
  });
});

describe("seleção dos mais próximos", () => {
  const cands = [
    { id: 4, distance: 900 },
    { id: 1, distance: 50 },
    { id: 7, distance: 300 },
    { id: 2, distance: 300 },
    { id: 9, distance: 10 },
  ];

  test("devolve os N mais próximos em ordem de distância, empate pelo menor id", () => {
    expect(selectNearest(cands, 3)).toEqual([9, 1, 2]);
    expect(selectNearest(cands, 4)).toEqual([9, 1, 2, 7]);
  });

  test("limite maior que a lista devolve tudo; limite zero, nada; não altera a entrada", () => {
    const copy = JSON.stringify(cands);
    expect(selectNearest(cands, 99)).toHaveLength(5);
    expect(selectNearest(cands, 0)).toEqual([]);
    expect(selectNearest([], 16)).toEqual([]);
    expect(JSON.stringify(cands)).toBe(copy);
  });

  test("com 40 sons e limite 16, só os 16 mais próximos", () => {
    const many = Array.from({ length: 40 }, (_, i) => ({ id: i, distance: (i * 37) % 500 }));
    const chosen = selectNearest(many, 16);
    expect(chosen).toHaveLength(16);
    const worstChosen = Math.max(...chosen.map((id) => many[id]!.distance));
    for (const c of many) if (!chosen.includes(c.id)) expect(c.distance).toBeGreaterThanOrEqual(worstChosen);
  });
});

describe("mixagem (mudo, solo, audição isolada)", () => {
  test("sem nada marcado, todos tocam", () => {
    expect(audibleInMix(1, mix())).toBe(true);
  });
  test("mudo tira só o som", () => {
    const m = mix({ muted: new Set([1]) });
    expect(audibleInMix(1, m)).toBe(false);
    expect(audibleInMix(2, m)).toBe(true);
  });
  test("solo deixa só os solos (e vence o mudo do próprio som)", () => {
    const m = mix({ solo: new Set([2]), muted: new Set([2, 3]) });
    expect(audibleInMix(2, m)).toBe(true);
    expect(audibleInMix(3, m)).toBe(false);
    expect(audibleInMix(4, m)).toBe(false);
  });
  test("mudo global cala tudo; a audição isolada é um pedido explícito e passa", () => {
    expect(audibleInMix(1, mix({ muteAll: true }))).toBe(false);
    expect(audibleInMix(1, mix({ muteAll: true, preview: 1 }))).toBe(true);
    expect(audibleInMix(2, mix({ preview: 1 }))).toBe(false);
  });
});

describe("arquivo e laço", () => {
  test("caminho dentro do mod: message é relativo a sound/", () => {
    expect(soundRel("ambience/wind.wav")).toEqual({ ok: true, rel: "sound/ambience/wind.wav" });
    expect(soundRel("sound/ambience/wind.wav")).toEqual({ ok: true, rel: "sound/ambience/wind.wav" });
    expect(soundRel("\\ambience\\wind.wav")).toEqual({ ok: true, rel: "sound/ambience/wind.wav" });
    expect(soundRel("  ")).toEqual({ ok: false, reason: "noFile" });
    expect(soundRel(null)).toEqual({ ok: false, reason: "noFile" });
    expect(soundRel("!HG_ALERT")).toEqual({ ok: false, reason: "sentence" });
    expect(soundRel("#alerta")).toEqual({ ok: false, reason: "sentence" });
  });

  test("nome curto", () => {
    expect(soundLabel("ambience/wind.wav")).toBe("wind.wav");
    expect(soundLabel("x.wav")).toBe("x.wav");
    expect(soundLabel(null)).toBe("—");
  });

  test("região de laço: amostras da taxa do ARQUIVO viram segundos", () => {
    expect(loopRegion([{ start_sample: 11025, end_sample: null }], 22050, 2)).toEqual({ start: 0.5, end: 0 });
    expect(loopRegion([{ start_sample: 22050, end_sample: 44100 }], 22050, 3)).toEqual({ start: 1, end: 2 });
    expect(loopRegion([{ start_sample: 0, end_sample: 99999 }], 22050, 2)).toEqual({ start: 0, end: 2 });
  });

  test("sem laço, laço inválido ou fora do arquivo -> null", () => {
    expect(loopRegion([], 22050, 2)).toBeNull();
    expect(loopRegion([{ start_sample: 0, end_sample: null }], 0, 2)).toEqual(null);
    expect(loopRegion([{ start_sample: 99999, end_sample: null }], 22050, 2)).toBeNull();
    expect(loopRegion([{ start_sample: 100, end_sample: 50 }], 22050, 2)).toEqual({ start: 100 / 22050, end: 0 });
  });
});

describe("cache por memória", () => {
  test("remove o menos usado quando passa do teto, poupando o recém-guardado e os presos", () => {
    const c = new BufferCache<string>(100);
    c.put("a", "A", 40);
    c.put("b", "B", 40);
    c.get("a"); // a fica mais recente que b
    const removed = c.put("c", "C", 40);
    expect(removed).toEqual(["b"]);
    expect(c.has("a")).toBe(true);
    expect(c.has("c")).toBe(true);
    expect(c.bytes).toBe(80);
    // "a" tocando agora: não sai, mesmo que estoure
    const removed2 = c.put("d", "D", 60, (k) => k === "a");
    expect(removed2).toEqual(["c"]);
    expect(c.has("a")).toBe(true);
    expect(c.has("d")).toBe(true);
  });

  test("um item maior que o teto fica (é o que acabou de ser pedido)", () => {
    const c = new BufferCache<string>(10);
    expect(c.put("big", "X", 50)).toEqual([]);
    expect(c.has("big")).toBe(true);
  });

  test("clear esvazia", () => {
    const c = new BufferCache<number>(10);
    c.put("a", 1, 5);
    c.clear();
    expect(c.size).toBe(0);
    expect(c.bytes).toBe(0);
  });
});

describe("utilitários", () => {
  test("data URL base64 -> bytes", () => {
    const bytes = new Uint8Array(dataUrlToBytes("data:audio/wav;base64,UklGRg=="));
    expect([...bytes]).toEqual([0x52, 0x49, 0x46, 0x46]);
    expect(() => dataUrlToBytes("nada")).toThrow();
  });

  test("matizes dos alcances diferem entre vizinhos", () => {
    const hues = Array.from({ length: 8 }, (_, i) => soundHue(i));
    expect(new Set(hues.map(Math.round)).size).toBe(8);
    for (let i = 1; i < hues.length; i++) expect(Math.abs(hues[i]! - hues[i - 1]!)).toBeGreaterThan(30);
  });
});
