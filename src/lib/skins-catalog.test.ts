import { describe, expect, test } from "bun:test";
import {
  filterModels,
  groupByDir,
  isPlayerModel,
  modelInScope,
  modelPrefix,
  modelStem,
  scopeCounts,
  teamOf,
} from "./skins-catalog.ts";
import type { ModFileEntry } from "../types/modfiles.ts";

function entry(path: string, over: Partial<ModFileEntry> = {}): ModFileEntry {
  const name = path.split("/").pop()!;
  return {
    path,
    name,
    kind: "model",
    size: 1024,
    layer: "mod",
    origin: "mod",
    pak: null,
    ...over,
  };
}

describe("prefixo e pasta", () => {
  test("reconhece v_, w_ e p_ pelo nome do arquivo", () => {
    expect(modelPrefix("v_ak47.mdl")).toBe("v");
    expect(modelPrefix("w_ak47.mdl")).toBe("w");
    expect(modelPrefix("p_357.mdl")).toBe("p");
    expect(modelPrefix("barrel.mdl")).toBeNull();
  });

  test("stem tira a pasta e a extensão; sem ponto não quebra", () => {
    expect(modelStem("models/weapons/v_ak47.mdl")).toBe("v_ak47");
    expect(modelStem("V_AK47.MDL")).toBe("v_ak47");
    expect(modelStem("modelo")).toBe("modelo");
    expect(modelStem(".mdl")).toBe(".mdl");
  });

  test("modelo dentro de models/player/ conta como jogador mesmo sem prefixo", () => {
    expect(isPlayerModel("models/player/urban/urban.mdl", "urban.mdl")).toBe(true);
    expect(isPlayerModel("models\\player\\gign.mdl", "gign.mdl")).toBe(true);
    expect(isPlayerModel("models/weapons/v_ak47.mdl", "v_ak47.mdl")).toBe(false);
    expect(isPlayerModel("models/player_old/x.mdl", "x.mdl")).toBe(false);
  });
});

describe("time", () => {
  test("nomes clássicos do CS: TR e CT", () => {
    expect(teamOf("models/player/terror/terror.mdl", "terror.mdl")).toBe("tr");
    expect(teamOf("models/player/leet/leet.mdl", "leet.mdl")).toBe("tr");
    expect(teamOf("models/player/arctic/arctic.mdl", "arctic.mdl")).toBe("tr");
    expect(teamOf("models/player/guerilla/guerilla.mdl", "guerilla.mdl")).toBe("tr");
    expect(teamOf("models/player/urban/urban.mdl", "urban.mdl")).toBe("ct");
    expect(teamOf("models/player/gign/gign.mdl", "gign.mdl")).toBe("ct");
    expect(teamOf("models/player/gsg9/gsg9.mdl", "gsg9.mdl")).toBe("ct");
    expect(teamOf("models/player/sas/sas.mdl", "sas.mdl")).toBe("ct");
    expect(teamOf("models/player/vip/vip.mdl", "vip.mdl")).toBe("ct");
  });

  test("a pasta também identifica o time (modelo com nome próprio)", () => {
    expect(teamOf("models/player/urban/urban_2010.mdl", "urban_2010.mdl")).toBe("ct");
    expect(teamOf("models/player/terror/terror1.mdl", "terror1.mdl")).toBe("tr");
  });

  test("mod zumbi tem prioridade e reconhece os nomes livres", () => {
    expect(teamOf("models/player/zombie_source/zombie_source.mdl", "zombie_source.mdl")).toBe("zombie");
    expect(teamOf("models/player/zumbi/zumbi.mdl", "zumbi.mdl")).toBe("zombie");
    expect(teamOf("models/player/zp_class.mdl", "zp_class.mdl")).toBe("zombie");
    expect(teamOf("models/player/urban/zombie_urban.mdl", "zombie_urban.mdl")).toBe("zombie");
  });

  test("nome sem pista de time devolve null", () => {
    expect(teamOf("models/props/barrel.mdl", "barrel.mdl")).toBeNull();
    expect(teamOf("models/weapons/v_ak47.mdl", "v_ak47.mdl")).toBeNull();
  });
});

describe("escopos das subabas", () => {
  const models = [
    entry("models/weapons/v_ak47.mdl"),
    entry("models/weapons/w_ak47.mdl"),
    entry("models/weapons/p_357.mdl"),
    entry("models/player/urban/urban.mdl"),
    entry("models/player/terror/terror.mdl"),
    entry("models/player/zombie_source/zombie_source.mdl"),
    entry("models/props/barrel.mdl"),
  ];

  test("cada escopo traz só o que é dele", () => {
    expect(filterModels(models, "v").map((m) => m.name)).toEqual(["v_ak47.mdl"]);
    expect(filterModels(models, "w").map((m) => m.name)).toEqual(["w_ak47.mdl"]);
    expect(filterModels(models, "p").map((m) => m.name)).toEqual([
      "p_357.mdl",
      "urban.mdl",
      "terror.mdl",
      "zombie_source.mdl",
    ]);
    expect(filterModels(models, "ct").map((m) => m.name)).toEqual(["urban.mdl"]);
    expect(filterModels(models, "tr").map((m) => m.name)).toEqual(["terror.mdl"]);
    expect(filterModels(models, "zombie").map((m) => m.name)).toEqual(["zombie_source.mdl"]);
  });

  test("escopo null mostra todos e ignora o que não é .mdl solto", () => {
    const mixed = [
      ...models,
      entry("models/weapons/v_deagle.mdl", { layer: "pak", pak: "pak0.pak" }),
      entry("maps/de_dust2.bsp", { kind: "map" }),
      entry("sound/step.wav", { kind: "sound" }),
    ];
    expect(filterModels(mixed, null).map((m) => m.name)).not.toContain("v_deagle.mdl");
    expect(filterModels(mixed, null).map((m) => m.name)).not.toContain("de_dust2.bsp");
    expect(filterModels(mixed, null)).toHaveLength(models.length);
  });

  test("contador por subaba", () => {
    expect(scopeCounts(models)).toEqual({ p: 4, v: 1, w: 1, tr: 1, ct: 1, zombie: 1 });
  });

  test("modelInScope aceita o mínimo (path + name)", () => {
    expect(modelInScope("v", { path: "models/v_x.mdl", name: "v_x.mdl" })).toBe(true);
    expect(modelInScope("ct", { path: "models/v_x.mdl", name: "v_x.mdl" })).toBe(false);
  });

  test("agrupa por pasta em ordem, com os arquivos ordenados", () => {
    const groups = groupByDir([
      entry("models/player/urban/urban.mdl"),
      entry("models/player/gign/gign.mdl"),
      entry("models/weapons/v_ak47.mdl"),
      entry("models/weapons/v_deagle.mdl"),
    ]);
    expect([...groups.keys()]).toEqual(["models/player/gign", "models/player/urban", "models/weapons"]);
    expect(groups.get("models/weapons")!.map((m) => m.name)).toEqual(["v_ak47.mdl", "v_deagle.mdl"]);
  });
});
