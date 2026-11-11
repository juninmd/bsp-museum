// Capturas da paleta de comandos e dos atalhos (UI/UX), sem Tauri: o `invoke` é simulado com os
// JSONs que o backend em Rust gera (`cargo test gerar_dados_das_capturas -- --ignored`).
//   node scripts/screenshots-ux.mjs <FIXTURE_DIR> [saida]
import { spawn } from "node:child_process";
import { readFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const fixtures = resolve(process.argv[2] ?? "");
const out = resolve(process.argv[3] ?? "prints");
if (!process.argv[2]) {
  console.error("uso: node scripts/screenshots-ux.mjs <FIXTURE_DIR> [saida]");
  process.exit(1);
}
mkdirSync(out, { recursive: true });
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
const read = (f) => readFileSync(join(fixtures, f), "utf8");
const scan = JSON.parse(read("scan.json"));
const detail = Object.fromEntries(scan.map((m) => [m.name, JSON.parse(read(`detail-${m.name}.json`))]));
const thumb = Object.fromEntries(scan.map((m) => [m.name, read(`thumb-${m.name}.svg`)]));

const port = 4173 + Math.floor(Math.random() * 500);
const server = spawn("bunx", ["vite", "preview", "--port", String(port), "--strictPort"], { stdio: "ignore" });
const base = `http://localhost:${port}/`;
for (let i = 0; i < 60; i++) {
  try { if ((await fetch(base)).ok) break; } catch {}
  await new Promise((r) => setTimeout(r, 250));
}
const mock = `(() => {
  const D = ${JSON.stringify({ scan, detail, thumb })};
  const settings = { last_dir: ${JSON.stringify(read("maps_dir.txt"))}, annotations: {}, theme: null, lang: null, slots: 32 };
  const nameOf = (p) => String(p).split(/[\\\\/]/).pop().replace(/\\.bsp$/, "");
  window.__TAURI_INTERNALS__ = { transformCallback: () => 0, invoke: async (cmd, a = {}) => {
    switch (cmd) {
      case "load_settings": return settings;
      case "save_settings": Object.assign(settings, a.settings); return null;
      case "scan_maps": return D.scan;
      case "map_thumbnail": return D.thumb[nameOf(a.path)];
      case "map_detail": return D.detail[nameOf(a.path)];
      default: return null;
    }
  } };
})();`;

const browser = await chromium.launch();
try {
  const shot = async (name, theme = "dark", lang = "pt", fn) => {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 760 }, deviceScaleFactor: 2, colorScheme: theme, locale: lang === "en" ? "en-US" : "pt-BR" });
    const page = await ctx.newPage();
    await page.addInitScript(mock);
    await page.goto(base);
    await page.waitForSelector(".card");
    await fn(page);
    await page.screenshot({ path: join(out, name + ".png") });
    await ctx.close();
  };
  await shot("v11-galeria-observatorio", "dark", "pt", async (p) => { await p.waitForTimeout(1600); });
  await shot("v11-galeria-claro-en", "light", "en", async (p) => { await p.waitForTimeout(1600); });
  await shot("v9-paleta-comandos", "dark", "pt", async (p) => {
    await p.keyboard.press("Control+k");
    await p.waitForSelector(".palette-item");
  });
  await shot("v9-paleta-busca-mapa", "dark", "pt", async (p) => {
    await p.keyboard.press("Control+k");
    await p.keyboard.type("museu");
    await p.keyboard.press("ArrowDown");
  });
  await shot("v9-paleta-en-claro", "light", "en", async (p) => {
    await p.keyboard.press("Control+k");
    await p.keyboard.type("dark");
  });
  await shot("v9-atalhos", "dark", "pt", async (p) => {
    await p.keyboard.press("Shift+?");
    await p.waitForSelector(".shortcuts");
  });
  // fluxo: abre o mapa pela paleta
  await shot("v9-paleta-abre-mapa", "dark", "pt", async (p) => {
    await p.keyboard.press("/");
    await p.keyboard.type("de_leak");
    await p.keyboard.press("Enter");
    await p.waitForSelector("#drawer:not([hidden])");
    await p.waitForTimeout(400);
  });
} finally {
  await browser.close();
  server.kill();
}
