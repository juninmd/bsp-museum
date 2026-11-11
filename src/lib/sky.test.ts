import { describe, expect, test } from "bun:test";
import { foundCount, isSkyEnvPath, parseSkyPath, skyExtCandidates, skyFacePath, SKY_FACES } from "./sky.ts";

describe("céu (skybox)", () => {
  test("só vale em gfx/env", () => {
    expect(isSkyEnvPath("gfx/env/desertbk.tga")).toBe(true);
    expect(isSkyEnvPath("/jogos/cstrike/GFX/ENV/desertbk.tga")).toBe(true);
    expect(isSkyEnvPath("C:\\jogos\\cstrike\\gfx\\env\\desertbk.tga")).toBe(true);
    expect(isSkyEnvPath("gfx/vgui/logos/dust.bmp")).toBe(false);
    expect(parseSkyPath("gfx/vgui/desertbk.tga")).toBeNull();
  });

  test("decompõe o nome", () => {
    const s = parseSkyPath("/jogos/cstrike/gfx/env/desertbk.tga")!;
    expect(s).toEqual({ dir: "/jogos/cstrike/gfx/env/", prefix: "desert", ext: "tga", face: "bk", upper: false });
    expect(parseSkyPath("gfx/env/snow.tga")).toBeNull(); // sem sufixo de lado
    expect(parseSkyPath("gfx/env/up.tga")).toBeNull(); // sem nome de céu
    expect(parseSkyPath("gfx/env/desertup.png")).toBeNull();
  });

  test("monta os 6 irmãos preservando separador e caixa", () => {
    const s = parseSkyPath("C:\\m\\gfx\\env\\NIGHTUP.TGA")!;
    expect(s.upper).toBe(true);
    expect(s.face).toBe("up");
    expect(skyFacePath(s, "ft")).toBe("C:\\m\\gfx\\env\\NIGHTFT.TGA");
    const t = parseSkyPath("gfx/env/snowlf.bmp")!;
    expect(SKY_FACES.map((f) => skyFacePath(t, f))).toEqual([
      "gfx/env/snowup.bmp",
      "gfx/env/snowdn.bmp",
      "gfx/env/snowlf.bmp",
      "gfx/env/snowrt.bmp",
      "gfx/env/snowft.bmp",
      "gfx/env/snowbk.bmp",
    ]);
  });

  test("tenta a outra extensão e conta os lados", () => {
    expect(skyExtCandidates("tga")).toEqual(["tga", "bmp"]);
    expect(skyExtCandidates("BMP")).toEqual(["BMP", "TGA"]);
    const found = { up: true, dn: false, lf: true, rt: true, ft: true, bk: false };
    expect(foundCount(found)).toBe(4);
  });
});
