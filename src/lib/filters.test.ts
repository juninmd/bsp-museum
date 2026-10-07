import { describe, expect, test } from "bun:test";
import type { MapSummary } from "../types.ts";
import {
  allRules,
  allTags,
  annotationKey,
  EMPTY_FILTERS,
  filterAndSort,
  flagsOf,
  isEmptyAnnotation,
  normalizeTag,
  problemsOf,
} from "./filters.ts";

function map(name: string, over: Partial<MapSummary> = {}): MapSummary {
  return {
    path: `/cs/maps/${name}.bsp`,
    name,
    file_size: 1000,
    title: null,
    mode: "bomb",
    mode_label: "de_ · bomba",
    mode_by_entities: "bomb",
    ct_spawns: 16,
    t_spawns: 16,
    entity_count: 100,
    bounds: { mins: [0, 0, 0], maxs: [10, 10, 10], size: [10, 10, 10] },
    fullbright: false,
    error: null,
    problems: [],
    bsp_version: 30,
    ...over,
  };
}

const crit = (id: string) => ({ id, severity: "critical" as const });
const warn = (id: string) => ({ id, severity: "warn" as const });

const maps = [
  map("de_ok"),
  map("de_ruim", { problems: [crit("de-sem-bomb-target"), warn("sem-buyzone")], file_size: 5000 }),
  map("zm_escuro", { mode: "zombie", mode_label: "zombie plague", problems: [warn("fullbright")], title: "Casa Escura" }),
  map("cs_info", { problems: [{ id: "cs-sem-resgate", severity: "info" }] }),
];

describe("problemsOf", () => {
  test("poucos-spawns depende dos slots da UI", () => {
    const m = map("x", { ct_spawns: 6, t_spawns: 6 });
    expect(problemsOf(m, 12).map((p) => p.id)).toEqual([]);
    expect(problemsOf(m, 32).map((p) => p.id)).toEqual(["poucos-spawns"]);
  });

  test("mapa sem nenhum spawn não vira poucos-spawns (já é sem-spawn)", () => {
    expect(problemsOf(map("x", { ct_spawns: 0, t_spawns: 0 }), 32)).toEqual([]);
  });

  test("erro de leitura é crítico e vem primeiro", () => {
    const p = problemsOf(map("x", { error: "lixo", problems: [warn("fullbright")] }), 2);
    expect(p[0]).toEqual({ id: "erro-leitura", severity: "critical" });
  });

  test("info não aparece nas etiquetas do card", () => {
    expect(flagsOf(maps[3]!, 2)).toEqual([]);
    expect(flagsOf(maps[1]!, 2).length).toBe(2);
  });
});

describe("filterAndSort", () => {
  const run = (f: Partial<typeof EMPTY_FILTERS>, ann = {}) =>
    filterAndSort(maps, { ...EMPTY_FILTERS, ...f }, ann, 2).map((m) => m.name);

  test("sem filtro devolve tudo ordenado por nome", () => {
    expect(run({})).toEqual(["cs_info", "de_ok", "de_ruim", "zm_escuro"]);
  });
  test("busca por título e por modo", () => {
    expect(run({ query: "escura" })).toEqual(["zm_escuro"]);
    expect(run({ query: "zombie" })).toEqual(["zm_escuro"]);
  });
  test("filtro por gravidade", () => {
    expect(run({ severity: "critical" })).toEqual(["de_ruim"]);
    expect(run({ severity: "warn" })).toEqual(["de_ruim", "zm_escuro"]);
    expect(run({ severity: "clean" })).toEqual(["cs_info", "de_ok"]);
  });
  test("filtro por regra", () => {
    expect(run({ rule: "fullbright" })).toEqual(["zm_escuro"]);
  });
  test("problemas primeiro pesa crítico acima de aviso", () => {
    expect(run({ sort: "problems" })[0]).toBe("de_ruim");
  });
  test("ordena por tamanho", () => {
    expect(run({ sort: "size" })[0]).toBe("de_ruim");
  });
  test("favoritos, tags e busca em nota", () => {
    const ann = {
      "de_ok.bsp": { favorite: true, tags: ["competitivo"], note: "bom para scrim" },
      "zm_escuro.bsp": { favorite: false, tags: ["zombie", "competitivo"], note: "" },
    };
    expect(run({ favoritesOnly: true }, ann)).toEqual(["de_ok"]);
    expect(run({ tag: "competitivo" }, ann)).toEqual(["de_ok", "zm_escuro"]);
    expect(run({ query: "scrim" }, ann)).toEqual(["de_ok"]);
    expect(allTags(ann)).toEqual(["competitivo", "zombie"]);
  });
  test("lista de regras presentes", () => {
    expect(allRules(maps, 2)).toEqual(["cs-sem-resgate", "de-sem-bomb-target", "fullbright", "sem-buyzone"]);
  });
});

describe("anotações", () => {
  test("chave é o nome do arquivo, com barra do Windows também", () => {
    expect(annotationKey({ path: "C:\\cs\\maps\\de_x.bsp" })).toBe("de_x.bsp");
    expect(annotationKey({ path: "/cs/maps/de_x.bsp" })).toBe("de_x.bsp");
  });
  test("normalizeTag", () => {
    expect(normalizeTag("  Meu Mapa, Bom ")).toBe("meu-mapa-bom");
    expect(normalizeTag("x".repeat(80)).length).toBe(32);
  });
  test("anotação vazia é descartável", () => {
    expect(isEmptyAnnotation({ favorite: false, tags: [], note: "  " })).toBe(true);
    expect(isEmptyAnnotation({ favorite: true, tags: [], note: "" })).toBe(false);
  });
});
