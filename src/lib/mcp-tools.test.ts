import { describe, expect, test } from "bun:test";
import { addCommand, allToolNames, filterGroups, mcpJson, MCP_GROUPS } from "./mcp-tools.ts";

describe("catálogo MCP", () => {
  test("nomes únicos, em snake_case e com descrição nos dois idiomas", () => {
    const names = allToolNames();
    expect(new Set(names).size).toBe(names.length);
    for (const g of MCP_GROUPS) {
      for (const t of g.tools) {
        expect(t.name, t.name).toMatch(/^[a-z][a-z0-9_]*$/);
        expect(t.desc[0].length, t.name).toBeGreaterThan(3);
        expect(t.desc[1].length, t.name).toBeGreaterThan(3);
      }
    }
  });

  test("inclui as ferramentas de imagem novas e as 29 atuais", () => {
    const names = new Set(allToolNames());
    for (const n of [
      "map_view", "model_view", "model_texture", "image_view", "wad_sheet", "map_texture_sheet", "map_skybox", "map_lightmap",
      "map_nearby", "texture_usage", "model_turnaround", "model_filmstrip", "sprite_sheet", "audio_waveform",
      "list_roots", "list_maps", "map_summary", "audit_folder", "demo_info",
    ]) {
      expect(names.has(n), n).toBe(true);
    }
    expect(names.size).toBe(38);
  });

  test("filtra sem acento e descarta grupos vazios", () => {
    expect(filterGroups("", 0)).toBe(MCP_GROUPS);
    const g = filterGroups("ceu", 0);
    expect(g.flatMap((x) => x.tools.map((t) => t.name))).toContain("map_skybox");
    expect(filterGroups("zzzz", 1)).toEqual([]);
    expect(filterGroups("waveform", 1).length).toBe(1);
  });

  test("comando e .mcp.json usam a pasta do mod", () => {
    expect(addCommand(null)).toContain("--root /caminho/cstrike");
    expect(addCommand("/jogos/cs trike")).toBe('claude mcp add bsp-museum -- /caminho/bsp-museum-mcp serve --root "/jogos/cs trike"');
    expect(JSON.parse(mcpJson("/x")).mcpServers["bsp-museum"].args).toEqual(["serve", "--root", "/x"]);
  });
});
