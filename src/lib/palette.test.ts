import { describe, expect, test } from "bun:test";
import { fuzzyScore, rankItems, type PaletteItem } from "./palette.ts";

const map = (label: string, hint = ""): PaletteItem => ({ id: `map:${label}`, label, hint, group: "map" });
const act = (id: string, label: string, keywords = ""): PaletteItem => ({ id, label, group: "action", keywords });

describe("fuzzyScore", () => {
  test("letras fora de ordem não casam", () => {
    expect(fuzzyScore("dm", "mad")).toBe(-1);
    expect(fuzzyScore("xyz", "de_dust2")).toBe(-1);
  });
  test("ignora acento e caixa", () => {
    expect(fuzzyScore("tema", "Alternar TEMA")).toBeGreaterThan(0);
    expect(fuzzyScore("acao", "ação")).toBeGreaterThan(0);
  });
  test("prefixo e trecho contíguo ganham de letras espalhadas", () => {
    expect(fuzzyScore("dust", "de_dust2")).toBeGreaterThan(fuzzyScore("dust", "de_desert_stone"));
    expect(fuzzyScore("de_d", "de_dust2")).toBeGreaterThan(fuzzyScore("de_d", "xx_de_something_d"));
  });
  test("consulta vazia pontua zero", () => expect(fuzzyScore("", "qualquer")).toBe(0));
});

describe("rankItems", () => {
  const items = [act("theme", "Alternar tema", "claro escuro dark light"), map("de_dust2", "Dust II"), map("de_inferno"), map("cs_office")];
  test("sem consulta mantém a ordem", () => {
    expect(rankItems(items, "").map((i) => i.id)).toEqual(items.map((i) => i.id));
  });
  test("acha mapa por nome e por dica", () => {
    expect(rankItems(items, "dust")[0]?.id).toBe("map:de_dust2");
    expect(rankItems(items, "inf")[0]?.id).toBe("map:de_inferno");
    expect(rankItems(items, "II")[0]?.id).toBe("map:de_dust2");
  });
  test("acha ação por sinônimo", () => {
    expect(rankItems(items, "dark")[0]?.id).toBe("theme");
  });
  test("sem resultado devolve vazio e respeita o limite", () => {
    expect(rankItems(items, "zzzz")).toEqual([]);
    expect(rankItems(items, "e", 2).length).toBe(2);
  });
});
