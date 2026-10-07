import { describe, expect, test } from "bun:test";
import { Glob } from "bun";
import { DICT } from "./lib/dict.ts";
import { FINDINGS_EN } from "./lib/findings-en.ts";
import { detectLang, findingText, modeLabel, setLang, t } from "./i18n.ts";
import { fastdlText, auditCsv, auditMarkdown, auditTotals } from "./lib/report.ts";
import type { AuditReport, Finding } from "./types.ts";

const finding = (id: string, args: string[] = []): Finding => ({
  id,
  severity: "warn",
  title: "título pt",
  detail: "detalhe pt",
  hint: "dica pt",
  args,
});

describe("dicionário", () => {
  test("toda chave tem texto nos dois idiomas", () => {
    for (const [key, [pt, en]] of Object.entries(DICT)) {
      expect(pt.length, key).toBeGreaterThan(0);
      expect(en.length, key).toBeGreaterThan(0);
    }
  });

  test("placeholders {n} iguais em pt e en", () => {
    const slots = (s: string) => [...s.matchAll(/\{(\d+)\}/g)].map((m) => m[1]).sort().join();
    for (const [key, [pt, en]] of Object.entries(DICT)) expect(slots(pt), key).toBe(slots(en));
  });

  test("toda chave usada no código existe no dicionário", async () => {
    const used = new Set<string>();
    const files = [...new Glob("src/**/*.ts").scanSync(".")].filter((f) => !f.endsWith(".test.ts") && !f.endsWith("dict.ts"));
    for (const f of files) {
      const text = await Bun.file(f).text();
      for (const m of text.matchAll(/\bt\(\s*"([\w.]+)"/g)) used.add(m[1]!);
    }
    const html = await Bun.file("index.html").text();
    for (const m of html.matchAll(/data-i18n(?:-ph|-title|-aria)?="([^"]+)"/g)) used.add(m[1]!);
    const missing = [...used].filter((k) => !(k in DICT));
    expect(missing).toEqual([]);
  });

  test("há etiqueta curta para cada regra do backend", async () => {
    const rs = await Bun.file("src-tauri/src/diagnostics.rs").text();
    const ids = new Set([...rs.matchAll(/"([a-z]+(?:-[a-z]+)+)",\s*\n\s*Severity::/g)].map((m) => m[1]!));
    ids.add("poucos-spawns");
    ids.add("erro-leitura");
    for (const id of ids) expect(`rule.${id}` in DICT, id).toBe(true);
  });

  test("há texto em inglês para cada regra do backend", async () => {
    const rs = await Bun.file("src-tauri/src/diagnostics.rs").text();
    const ids = new Set([...rs.matchAll(/"([a-z]+(?:-[a-z]+)+)",\s*\n\s*Severity::/g)].map((m) => m[1]!));
    for (const id of ids) {
      const has = id in FINDINGS_EN || `${id}:warn` in FINDINGS_EN;
      expect(has, id).toBe(true);
    }
  });
});

describe("t / findingText", () => {
  test("interpola argumentos e cai na chave quando não existe", () => {
    setLang("pt");
    expect(t("audit.maps")).toBe("mapas");
    expect(t("card.open", "de_dust2")).toBe("abrir detalhes de de_dust2");
    expect(t("nao.existe")).toBe("nao.existe");
    setLang("en");
    expect(t("audit.maps")).toBe("maps");
  });

  test("detectLang: salvo > sistema", () => {
    expect(detectLang("en", "pt-BR")).toBe("en");
    expect(detectLang(null, "pt-BR")).toBe("pt");
    expect(detectLang(null, "de-DE")).toBe("en");
  });

  test("achado em português usa o texto do backend; em inglês remonta com os args", () => {
    setLang("pt");
    expect(findingText(finding("poucos-spawns", ["12", "32", "6", "6"])).title).toBe("título pt");
    setLang("en");
    expect(findingText(finding("poucos-spawns", ["12", "32", "6", "6"])).title).toBe("12 spawns for 32 slots");
    expect(findingText(finding("poucos-spawns", ["12", "32", "6", "6"])).detail).toBe("CT=6 · T=6");
  });

  test("prefixo-divergente traduz os modos dos args", () => {
    setLang("en");
    expect(findingText(finding("prefixo-divergente", ["bomb", "zombie"])).title).toBe("de_ · bomb entities in a zombie plague file");
  });

  test("cs-sem-resgate escolhe a variante pela severidade nos args", () => {
    setLang("en");
    expect(findingText(finding("cs-sem-resgate", ["2", "info"])).title).toContain("without an explicit");
    expect(findingText(finding("cs-sem-resgate", ["2", "warn"])).title).toContain("no CT spawn");
  });

  test("id desconhecido mantém o texto do backend", () => {
    setLang("en");
    expect(findingText(finding("regra-nova")).title).toBe("título pt");
  });

  test("modeLabel segue o idioma", () => {
    setLang("pt");
    expect(modeLabel("hostage")).toBe("cs_ · reféns");
    setLang("en");
    expect(modeLabel("hostage")).toBe("cs_ · hostages");
  });
});

describe("relatório", () => {
  const report: AuditReport = {
    dir: "/cs/maps",
    slots: 32,
    duplicates: [["/cs/maps/a.bsp", "/cs/maps/b.bsp"]],
    rows: [
      { name: "de_ok", path: "/cs/maps/de_ok.bsp", file_size: 2048, mode: "bomb", ct_spawns: 16, t_spawns: 16, bsp_version: 30, error: null, findings: [] },
      { name: "de_leak", path: "/cs/maps/de_leak.bsp", file_size: 4096, mode: "bomb", ct_spawns: 16, t_spawns: 16, bsp_version: 30, error: null, findings: [finding("sem-vis")] },
      { name: "lixo", path: "/cs/maps/lixo.bsp", file_size: 14, mode: "unknown", ct_spawns: 0, t_spawns: 0, bsp_version: 0, error: "versão 38", findings: [] },
    ],
  };

  test("totais separam crítico, aviso, limpo e ilegível", () => {
    setLang("pt");
    expect(auditTotals(report)).toEqual({ maps: 3, critical: 0, warn: 1, info: 0, clean: 1, errors: 1, duplicates: 1 });
  });

  test("CSV escapa vírgula e aspas, uma linha por achado", () => {
    setLang("pt");
    const csv = auditCsv({ ...report, rows: [{ ...report.rows[1]!, findings: [{ ...finding("sem-vis"), title: 'a, "b"' }] }] });
    expect(csv.split("\n")[1]).toContain('"a, ""b"""');
  });

  test("Markdown traz tabela, totais e duplicados", () => {
    setLang("en");
    const md = auditMarkdown(report);
    expect(md).toContain("# Map collection audit");
    expect(md).toContain("`de_leak`");
    expect(md).toContain("Duplicate files");
    expect(md).toContain("3 map(s)");
  });

  test("lista FastDL só tem o que se baixa", () => {
    const text = fastdlText({
      download_size: 0,
      download_count: 0,
      missing: 1,
      items: [
        { kind: "mapa", path: "maps/x.bsp", size: 1, found: true, shared: false },
        { kind: "wad", path: "halflife.wad", size: 1, found: true, shared: true },
        { kind: "sound", path: "sound/a.wav", size: 0, found: false, shared: false },
      ],
    });
    expect(text).toBe("maps/x.bsp\n");
  });
});

describe("CSV seguro", () => {
  test("célula que começa com = vira texto, e o arquivo leva BOM", () => {
    setLang("pt");
    const report: AuditReport = {
      dir: "/x",
      slots: 2,
      duplicates: [],
      rows: [{ name: "=HYPERLINK(1)", path: "/x/a.bsp", file_size: 10, mode: "unknown", ct_spawns: 0, t_spawns: 0, bsp_version: 30, error: null, findings: [] }],
    };
    const csv = auditCsv(report);
    expect(csv.startsWith("﻿")).toBe(true);
    expect(csv).toContain("'=HYPERLINK(1)");
    expect(csv).not.toContain("\n=HYPERLINK");
  });
});
