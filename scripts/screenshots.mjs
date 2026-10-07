// Gera as capturas de tela do README/PR sem precisar do app Tauri.
//
//   1. cargo test gerar_dados_das_capturas -- --ignored   (com FIXTURE_DIR=<pasta>)
//   2. bun run build
//   3. node scripts/screenshots.mjs <FIXTURE_DIR> [pasta-de-saida]
//
// O frontend de verdade roda no Chromium; só o `invoke` do Tauri é trocado por um
// simulador que devolve os JSONs que o backend em Rust gerou no passo 1.
// Precisa de `playwright` (global ou local); PLAYWRIGHT_MODULE aponta para outro caminho.
import { spawn } from "node:child_process";
import { readFileSync, mkdirSync } from "node:fs";
import { join, resolve, basename } from "node:path";

const fixtures = resolve(process.argv[2] ?? "");
const out = resolve(process.argv[3] ?? "prints");
if (!process.argv[2]) {
  console.error("uso: node scripts/screenshots.mjs <FIXTURE_DIR> [saida]");
  process.exit(1);
}
mkdirSync(out, { recursive: true });

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
const read = (f) => readFileSync(join(fixtures, f), "utf8");
const mapsDir = read("maps_dir.txt");
const nameOf = (path) => basename(String(path)).replace(/\.bsp$/, "");

const data = {
  scan: JSON.parse(read("scan.json")),
  audit: JSON.parse(read("audit.json")),
  compare: JSON.parse(read("compare.json")),
  detail: {}, thumb: {}, mesh: {}, entities: {}, radar: {},
};
for (const m of data.scan) {
  const n = m.name;
  data.detail[n] = JSON.parse(read(`detail-${n}.json`));
  data.thumb[n] = read(`thumb-${n}.svg`);
}
for (const n of ["de_museu", "de_parede"]) {
  data.mesh[n] = JSON.parse(read(`mesh-${n}.json`));
  data.entities[n] = JSON.parse(read(`entities-${n}.json`));
  data.radar[n] = read(`radar-${n}.txt`);
}
const settings = {
  last_dir: mapsDir, slots: 32, lang: "pt", theme: "dark",
  annotations: {
    "de_museu.bsp": { favorite: true, tags: ["competitivo", "scrim"], note: "Mapa de referência para o scrim de sexta." },
    "de_leak.bsp": { favorite: false, tags: ["revisar"], note: "" },
    "zm_escuro.bsp": { favorite: true, tags: ["zombie"], note: "" },
  },
};

// ---- servidor estático do build
const port = 4173 + Math.floor(Math.random() * 500);
const server = spawn("bunx", ["vite", "preview", "--port", String(port), "--strictPort"], { stdio: "ignore" });
const base = `http://localhost:${port}/`;
for (let i = 0; i < 60; i++) {
  try { if ((await fetch(base)).ok) break; } catch {}
  await new Promise((r) => setTimeout(r, 250));
}

const mock = `
(() => {
  const D = ${JSON.stringify(data)};
  const settings = ${JSON.stringify(settings)};
  const nameOf = (p) => String(p).split(/[\\\\/]/).pop().replace(/\\.bsp$/, "");
  const meshFor = (p) => D.mesh[nameOf(p)] ?? D.mesh.de_museu;
  const entFor = (p) => D.entities[nameOf(p)] ?? D.entities.de_museu;
  const radarFor = (p) => D.radar[nameOf(p)] ?? D.radar.de_museu;
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 0,
    invoke: async (cmd, a = {}) => {
      switch (cmd) {
        case "load_settings": return settings;
        case "save_settings": Object.assign(settings, a.settings); return null;
        case "scan_maps": return D.scan;
        case "map_thumbnail": return D.thumb[nameOf(a.path)];
        case "map_detail": return D.detail[nameOf(a.path)];
        case "map_mesh": return meshFor(a.path);
        case "map_entities": return entFor(a.path);
        case "map_radar": return radarFor(a.path);
        case "audit_folder": return D.audit;
        case "compare_maps": return D.compare;
        case "export_text": return null;
        case "export_radar": return ["a.bmp", "a.png", "a.txt"];
        case "plugin:dialog|open": return ${JSON.stringify(mapsDir)};
        case "plugin:dialog|save": return "/tmp/saida";
        default: return null;
      }
    },
  };
})();
`;

const browser = await chromium.launch({
  args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist", "--no-sandbox"],
});
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
page.on("pageerror", (e) => console.error("pageerror:", e.message));
await page.addInitScript(mock);
await page.goto(base);
await page.waitForSelector(".card");
await page.waitForFunction(() => document.querySelectorAll(".thumb svg").length >= 4);

const shot = async (name, target = page, opts = {}) => {
  await target.screenshot({ path: join(out, name), ...opts });
  console.log("ok", name);
};
const pause = (ms) => page.waitForTimeout(ms);

// 1. galeria (pt, escuro)
await shot("v6-galeria.png");

// 2. galeria em inglês, tema claro, filtrando por regra
await page.click("#lang-toggle");
await page.click("#theme-toggle");
await page.selectOption("#sev-filter", "critical");
await pause(300);
await shot("v6-galeria-en-claro.png");
await page.selectOption("#sev-filter", "");
await page.click("#theme-toggle");
await page.click("#lang-toggle");
await pause(200);

// 3. auditoria
await page.click("#audit");
await page.waitForSelector(".audit-table");
await shot("v6-auditoria.png");
await page.keyboard.press("Escape");

// 4. comparar de_museu x de_parede
const museu = page.locator('.card[data-path$="de_museu.bsp"] .cmp-check');
const parede = page.locator('.card[data-path$="de_parede.bsp"] .cmp-check');
await museu.check();
await parede.check();
await page.click("#compare");
await page.waitForSelector(".facts-table");
await shot("v6-comparar.png");
await page.keyboard.press("Escape");
await museu.uncheck();
await parede.uncheck();

// 5. detalhe de de_museu: planta + diagnóstico
await page.click('.card[data-path$="de_museu.bsp"] .card-open');
await page.waitForSelector("#plan-holder svg");
await pause(300);
await shot("v6-detalhe-planta.png");

// 6. 3D: o painel de entidades leva a câmera para dentro da sala A (func_buyzone)
await page.click('[data-view="tex"]');
await page.waitForSelector("#viewer3d canvas");
await pause(1200);
await page.locator("#ent-panel summary").click();
await page.waitForSelector(".ent-row");
await page.fill("#ent-search", "func_buyzone");
await pause(300);
await page.locator(".ent-go").first().click();
await pause(2200);
const viewer = page.locator("#viewer3d");
await shot("v6-3d-lightmap.png", viewer);
await page.uncheck("#lm-toggle");
await pause(900);
await shot("v6-3d-sem-lightmap.png", viewer);
await page.check("#lm-toggle");

// 7. PVS: de dentro da sala A a sala C some do desenho (compare o contador do HUD)
await pause(600);
await shot("v6-3d-pvs.png", viewer);
await page.uncheck("#pvs-toggle");
await pause(900);
await shot("v6-3d-sem-pvs.png", viewer);
await page.check("#pvs-toggle");
await page.fill("#ent-search", "");
await pause(300);
await page.locator("#ent-panel").scrollIntoViewIfNeeded();
await shot("v6-entidades.png", page.locator("#ent-panel"));

// 8. recursos (FastDL) e radar
const resSection = page.locator("section", { has: page.locator("#export-fastdl") });
await resSection.scrollIntoViewIfNeeded();
await shot("v6-recursos-fastdl.png", resSection);
await page.click("#radar-preview");
await page.waitForSelector(".radar-holder img");
await pause(300);
await shot("v6-radar.png", page.locator("section", { has: page.locator("#radar-preview") }));

// 9. notas/tags e diagnóstico do mapa com spawn em parede
await page.keyboard.press("Escape");
await pause(300);
await page.click('.card[data-path$="de_parede.bsp"] .card-open');
await page.waitForSelector("#plan-holder svg");
await pause(300);
await shot("v6-diagnostico.png", page.locator("section", { has: page.locator(".findings") }));

await browser.close();
server.kill();
