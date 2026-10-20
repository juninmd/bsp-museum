import { describe, expect, test } from "bun:test";
import {
  autoGain,
  baseName,
  buildTimeline,
  buildTree,
  classifyName,
  dirName,
  effectiveKind,
  entryKey,
  filterAndSort,
  filterEntries,
  flattenTree,
  formatBytes,
  formatDuration,
  hexDump,
  joinPath,
  loopSpans,
  modRootFromMapsDir,
  peakDb,
  reduceEnvelope,
  safeFileName,
  secondsToX,
  segmentAt,
  sortEntries,
  timelineDuration,
  totalsByKind,
  visibleRange,
  waveformPolyline,
  xToSeconds,
  EMPTY_FILE_FILTER,
} from "./explorer.ts";
import type { FileKind, Layer, ModFileEntry } from "../types/modfiles.ts";
import { DICT } from "./dict.ts";

const entry = (path: string, size = 100, layer: Layer = "mod", kind?: FileKind, pak: string | null = null): ModFileEntry => {
  const name = path.split("/").pop()!;
  return { path, name, kind: kind ?? classifyName(name), size, layer, origin: layer === "valve" ? "valve" : "mod", pak };
};

describe("formatação", () => {
  test("tamanhos", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024, "en-US")).toBe("1.0 KB");
    expect(formatBytes(3.5 * 1024, "en-US")).toBe("3.5 KB");
    expect(formatBytes(120 * 1024, "en-US")).toBe("120 KB");
    expect(formatBytes(1.25 * 1024 * 1024, "en-US")).toBe("1.3 MB");
    expect(formatBytes(2 * 1024 ** 3, "en-US")).toBe("2.0 GB");
    expect(formatBytes(-1)).toBe("–");
    expect(formatBytes(Number.NaN)).toBe("–");
  });
  test("tamanho respeita o idioma", () => {
    expect(formatBytes(1536, "pt-BR")).toBe("1,5 KB");
  });
  test("duração", () => {
    expect(formatDuration(0)).toBe("0:00.00");
    expect(formatDuration(75.5)).toBe("1:15.50");
    expect(formatDuration(3725)).toBe("1:02:05");
    expect(formatDuration(-1)).toBe("–");
  });
  test("nome seguro e pico em dB", () => {
    expect(safeFileName("{LAVA1")).toBe("_LAVA1");
    expect(safeFileName("!water/x")).toBe("_water_x");
    expect(safeFileName("")).toBe("arquivo");
    expect(peakDb(1)).toBe(0);
    expect(peakDb(0.5)!).toBeCloseTo(-6.02, 1);
    expect(peakDb(0)).toBeNull();
    expect(peakDb(null)).toBeNull();
  });
});

describe("caminhos", () => {
  test("pasta do mod a partir da pasta de mapas", () => {
    expect(modRootFromMapsDir("/jogos/cstrike/maps")).toBe("/jogos/cstrike");
    expect(modRootFromMapsDir("/jogos/cstrike/maps/")).toBe("/jogos/cstrike");
    expect(modRootFromMapsDir("C:\\hl\\cstrike\\Maps")).toBe("C:\\hl\\cstrike");
    expect(modRootFromMapsDir("/jogos/cstrike")).toBe("/jogos/cstrike");
    expect(modRootFromMapsDir("/maps")).toBe("/maps");
  });
  test("junta com o separador da raiz", () => {
    expect(joinPath("/a/b", "sprites/x.spr")).toBe("/a/b/sprites/x.spr");
    expect(joinPath("/a/b/", "/x.spr")).toBe("/a/b/x.spr");
    expect(joinPath("C:\\hl\\cstrike", "sprites/x.spr")).toBe("C:\\hl\\cstrike\\sprites\\x.spr");
  });
  test("nome e pasta", () => {
    expect(baseName("a/b/c.wav")).toBe("c.wav");
    expect(dirName("a/b/c.wav")).toBe("a/b");
    expect(dirName("c.wav")).toBe("");
  });
  test("classificação por extensão", () => {
    expect(classifyName("DE_DUST2.BSP")).toBe("map");
    expect(classifyName("a.WAV")).toBe("sound");
    expect(classifyName("gfx/x.lmp")).toBe("image");
    expect(classifyName("liblist.gam")).toBe("cfg");
    expect(classifyName("LICENSE")).toBe("other");
    expect(classifyName(".hidden")).toBe("other");
    expect(effectiveKind({ name: "x.lmp", kind: "other" })).toBe("image");
    expect(effectiveKind({ name: "x.lmp", kind: "text" })).toBe("text");
  });
});

describe("filtro e ordenação de arquivos", () => {
  const all = [
    entry("sound/ambience/wind.wav", 5000),
    entry("sound/weapons/ak47-1.wav", 12000),
    entry("sprites/muzzle.spr", 800),
    entry("maps/de_dust2.bsp", 9_000_000),
    entry("sound/ambience/wind.wav", 4000, "valve"),
    entry("sound/radio/go.wav", 300, "pak", "sound", "pak0.pak"),
    entry("gfx/logo.lmp", 70, "mod", "other"),
  ];

  test("busca por termos em qualquer ordem", () => {
    expect(filterEntries(all, { query: "wind sound", kind: "", layer: "" })).toHaveLength(2);
    expect(filterEntries(all, { query: "WIND", kind: "", layer: "" })).toHaveLength(2);
    expect(filterEntries(all, { query: "   ", kind: "", layer: "" })).toHaveLength(all.length);
  });
  test("busca enxerga o nome do pak", () => {
    expect(filterEntries(all, { query: "pak0 go", kind: "", layer: "" })).toHaveLength(1);
  });
  test("filtra por tipo e camada", () => {
    expect(filterEntries(all, { query: "", kind: "sound", layer: "" })).toHaveLength(4);
    expect(filterEntries(all, { query: "", kind: "sound", layer: "valve" })).toHaveLength(1);
    expect(filterEntries(all, { query: "", kind: "", layer: "pak" })).toHaveLength(1);
    expect(filterEntries(all, { query: "", kind: "image", layer: "" })).toHaveLength(1); // .lmp
    expect(filterEntries(all, { query: "zzz", kind: "", layer: "" })).toHaveLength(0);
  });
  test("ordenação por caminho é natural e desempata pela camada", () => {
    const sorted = sortEntries(all, "path");
    expect(sorted.map((e) => e.path)).toEqual([
      "gfx/logo.lmp",
      "maps/de_dust2.bsp",
      "sound/ambience/wind.wav",
      "sound/ambience/wind.wav",
      "sound/radio/go.wav",
      "sound/weapons/ak47-1.wav",
      "sprites/muzzle.spr",
    ]);
    expect(sorted[2]!.layer).toBe("mod");
    expect(sorted[3]!.layer).toBe("valve");
  });
  test("ordenação por tamanho, nome e tipo; decrescente inverte", () => {
    expect(sortEntries(all, "size")[0]!.size).toBe(70);
    expect(sortEntries(all, "size", true)[0]!.size).toBe(9_000_000);
    expect(sortEntries(all, "name")[0]!.name).toBe("ak47-1.wav");
    expect(sortEntries(all, "kind")[0]!.name).toBe("logo.lmp");
  });
  test("numeração natural", () => {
    const e = [entry("a10.wav"), entry("a2.wav"), entry("a1.wav")];
    expect(sortEntries(e, "name").map((x) => x.name)).toEqual(["a1.wav", "a2.wav", "a10.wav"]);
  });
  test("não muta a entrada", () => {
    const copy = [...all];
    filterAndSort(all, { ...EMPTY_FILE_FILTER, sort: "size", desc: true });
    expect(all).toEqual(copy);
  });
  test("totais por tipo", () => {
    const t = totalsByKind(all);
    expect(t.get("sound")).toEqual({ count: 4, bytes: 5000 + 12000 + 4000 + 300 });
    expect(t.get("image")?.count).toBe(1);
  });
  test("chave única por camada", () => {
    expect(entryKey(all[0]!)).not.toBe(entryKey(all[4]!));
  });
});

describe("árvore", () => {
  const entries = [
    entry("sound/a.wav", 10),
    entry("sound/ambience/b.wav", 20),
    entry("liblist.gam", 5),
    entry("sound/radio/c.wav", 7, "pak", "sound", "pak0.pak"),
    entry("pak0.pak", 1000, "mod", "pak"),
  ];
  const root = buildTree(entries);
  test("soma tamanho e contagem nas pastas", () => {
    expect(root.fileCount).toBe(5);
    expect(root.size).toBe(1042);
    const sound = root.children.find((c) => c.name === "sound")!;
    expect(sound.fileCount).toBe(2);
    expect(sound.size).toBe(30);
  });
  test("pastas primeiro e item de pak sob o nome do pak", () => {
    expect(root.children.filter((c) => c.isDir).map((c) => c.name)).toEqual(["pak0.pak", "sound"]);
    expect(root.children.findIndex((c) => !c.isDir)).toBeGreaterThan(root.children.findIndex((c) => c.isDir));
    const pakDir = root.children.find((c) => c.isDir && c.name === "pak0.pak")!;
    expect(pakDir.children[0]!.name).toBe("sound");
  });
  test("achatamento respeita as pastas abertas", () => {
    expect(flattenTree(root, new Set()).map((r) => r.node.name)).toEqual(["pak0.pak", "sound", "liblist.gam", "pak0.pak"]);
    const open = flattenTree(root, new Set(["sound"]));
    expect(open.map((r) => `${r.depth}:${r.node.name}`)).toContain("1:ambience");
    expect(open.map((r) => r.node.name)).not.toContain("b.wav");
    const all = flattenTree(root, new Set(), true);
    expect(all.some((r) => r.node.name === "b.wav" && r.depth === 2)).toBe(true);
    expect(all.some((r) => r.node.name === "c.wav" && r.depth === 3)).toBe(true);
  });
});

describe("lista virtual", () => {
  test("janela com folga e limites", () => {
    expect(visibleRange(0, 100, 20, 1000, 2)).toEqual([0, 7]);
    expect(visibleRange(400, 100, 20, 1000, 2)).toEqual([18, 27]);
    expect(visibleRange(99999, 100, 20, 30, 2)).toEqual([30, 30]);
    expect(visibleRange(0, 100, 20, 0)).toEqual([0, 0]);
    expect(visibleRange(0, 100, 20, 3, 6)).toEqual([0, 3]);
  });
});

describe("forma de onda", () => {
  test("polígono: máximos indo, mínimos voltando", () => {
    const env: [number, number][] = [
      [-0.5, 0.5],
      [-1, 1],
      [0, 0],
    ];
    const poly = waveformPolyline(env, 100, 100);
    expect(poly).toHaveLength(6);
    expect(poly[0]).toEqual([0, 25]);
    expect(poly[1]).toEqual([50, 0]);
    expect(poly[2]).toEqual([100, 50]);
    // volta: x decrescente, y espelhado
    expect(poly[3]).toEqual([100, 50]);
    expect(poly[4]).toEqual([50, 100]);
    expect(poly[5]).toEqual([0, 75]);
  });
  test("ganho amplia e satura em ±1", () => {
    const poly = waveformPolyline([[-0.25, 0.25]], 10, 100, 8);
    expect(poly[0]![1]).toBe(0);
    expect(poly[1]![1]).toBe(100);
  });
  test("vazio ou largura zero não gera pontos", () => {
    expect(waveformPolyline([], 100, 100)).toEqual([]);
    expect(waveformPolyline([[0, 0]], 0, 100)).toEqual([]);
  });
  test("reduz para no máximo uma coluna por pixel", () => {
    const env: [number, number][] = Array.from({ length: 1000 }, (_, i) => [-(i % 10) / 10, (i % 10) / 10]);
    const r = reduceEnvelope(env, 100);
    expect(r).toHaveLength(100);
    expect(r[0]).toEqual([-0.9, 0.9]); // cada coluna junta 10 trechos
    const few: [number, number][] = [[-1, 1]];
    expect(reduceEnvelope(few, 100)).toEqual(few);
  });
  test("ganho automático", () => {
    expect(autoGain(0.1)).toBe(8);
    expect(autoGain(0.9)).toBe(1);
    expect(autoGain(1)).toBe(1);
    expect(autoGain(null)).toBe(1);
    expect(autoGain(0.45)).toBeCloseTo(2);
  });
  test("tempo <-> x", () => {
    expect(secondsToX(5, 10, 200)).toBe(100);
    expect(secondsToX(50, 10, 200)).toBe(200);
    expect(secondsToX(1, 0, 200)).toBe(0);
    expect(xToSeconds(100, 10, 200)).toBe(5);
    expect(xToSeconds(-5, 10, 200)).toBe(0);
  });
  test("loops em segundos", () => {
    const spans = loopSpans(
      [
        { start_sample: 22050, end_sample: null, source: "cue" },
        { start_sample: 0, end_sample: 44100, source: "smpl" },
      ],
      44100,
      3,
    );
    expect(spans).toEqual([
      { start: 0.5, end: 3, source: "cue" },
      { start: 0, end: 1, source: "smpl" },
    ]);
    expect(loopSpans([{ start_sample: 1, end_sample: null, source: "cue" }], 0, 3)).toEqual([]);
  });
});

describe("sprites", () => {
  const img = { origin_x: 0, origin_y: 0, width: 1, height: 1, png: "" };
  const frames = [
    { kind: "single" as const, durations: [], images: [img] },
    { kind: "group" as const, durations: [0.2, 0.3], images: [img, img] },
    { kind: "single" as const, durations: [], images: [img] },
  ];
  test("linha do tempo mistura fps (simples) e tempos do grupo", () => {
    const tl = buildTimeline(frames, 10);
    expect(tl).toHaveLength(4);
    expect(tl.map((s) => [s.frame, s.image])).toEqual([[0, 0], [1, 0], [1, 1], [2, 0]]);
    expect(tl[1]!.start).toBeCloseTo(0.1);
    expect(tl[2]!.start).toBeCloseTo(0.3);
    expect(tl[3]!.start).toBeCloseTo(0.6);
    expect(timelineDuration(tl)).toBeCloseTo(0.7);
  });
  test("grupo sem duração válida usa o passo padrão", () => {
    const tl = buildTimeline([{ kind: "group", durations: [0], images: [img, img] }], 10);
    expect(tl[0]!.duration).toBeCloseTo(0.1);
    expect(tl[1]!.duration).toBeCloseTo(0.1);
  });
  test("segmento por tempo (com volta e sem)", () => {
    const tl = buildTimeline(frames, 10);
    expect(segmentAt(tl, 0)).toBe(0);
    expect(segmentAt(tl, 0.099)).toBe(0);
    expect(segmentAt(tl, 0.11)).toBe(1);
    expect(segmentAt(tl, 0.35)).toBe(2);
    expect(segmentAt(tl, 0.65)).toBe(3);
    expect(segmentAt(tl, 0.71)).toBe(0); // deu a volta
    expect(segmentAt(tl, 1.55, true)).toBe(segmentAt(tl, 0.85, true));
    expect(segmentAt(tl, 99, false)).toBe(3);
    expect(segmentAt(tl, -0.01, true)).toBe(3);
    expect(segmentAt([], 1)).toBe(-1);
  });
  test("fps diferente muda só os frames simples", () => {
    const tl = buildTimeline(frames, 20);
    expect(tl[0]!.duration).toBeCloseTo(0.05);
    expect(tl[1]!.duration).toBeCloseTo(0.2);
  });
});

describe("hex", () => {
  test("despejo com ASCII e preenchimento", () => {
    const text = hexDump(new TextEncoder().encode("Half-Life\u0001\u0002 ok!"), 8);
    const lines = text.split("\n");
    expect(lines).toHaveLength(2);
    expect(lines[0]).toBe("00000000  48 61 6c 66  2d 4c 69 66  |Half-Lif|");
    expect(lines[1]!.startsWith("00000008  65 01 02 20")).toBe(true);
    expect(lines[1]!.endsWith("|e.. ok!|")).toBe(true);
    expect(hexDump(new Uint8Array(0))).toBe("");
  });
});

describe("chaves de i18n montadas dinamicamente", () => {
  const families: Record<string, string[]> = {
    "exp.kind": ["map", "wad", "model", "sprite", "sound", "image", "nav", "demo", "text", "cfg", "pak", "dll", "other"],
    "exp.layer": ["mod", "valve", "pak"],
    "exp.sort": ["path", "name", "size", "kind"],
    "exp.tab": ["files", "wad", "sprites", "audio"],
    "exp.wad.k": ["miptex", "decal", "qpic", "font", "console", "palette", "other"],
    "exp.spr.type": ["parallel_upright", "facing_upright", "parallel", "oriented", "parallel_oriented"],
    "exp.spr.tex": ["normal", "additive", "index_alpha", "alpha_test"],
    "exp.spr.sync": ["sync", "random"],
    "exp.demo.kind": ["loading", "playback", "desconhecido"],
    "exp.img.palette": ["none", "embedded", "grayscale"],
    "exp.sprite": ["listAria", "search", "pick", "count", "none"],
    "exp.sound": ["listAria", "search", "pick", "count", "none"],
  };
  for (const [prefix, names] of Object.entries(families)) {
    test(prefix, () => {
      for (const n of names) expect(`${prefix}.${n}` in DICT, `${prefix}.${n}`).toBe(true);
    });
  }
  test("todo tipo de arquivo do backend tem etiqueta", async () => {
    const rs = await Bun.file("src-tauri/src/modfiles.rs").text();
    const block = /pub enum FileKind \{([^}]*)\}/.exec(rs)![1]!;
    const kinds = [...block.matchAll(/^\s*([A-Z]\w*),/gm)].map((m) => m[1]!.toLowerCase());
    expect(kinds.length).toBeGreaterThan(10);
    for (const k of kinds) expect(`exp.kind.${k}` in DICT, k).toBe(true);
  });
});
