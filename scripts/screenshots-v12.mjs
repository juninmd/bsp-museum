// Capturas da v12: Claude Link, cubo de céu (aba Imagens) e HUD do viewer 3D.
//   1. cargo test gerar_dados_das_capturas -- --ignored   (com FIXTURE_DIR=<pasta>)  [só para o HUD do 3D]
//   2. bun run build
//   3. node scripts/screenshots-v12.mjs <FIXTURE_DIR> [pasta-de-saida]
// O frontend de verdade roda no Chromium; o `invoke` do Tauri é trocado por um simulador. Os lados do céu
// são desenhados na própria página (canvas), então nada do jogo vai para o repositório.
// Precisa de `playwright`; PLAYWRIGHT_MODULE aponta para outro caminho.
import { spawn } from "node:child_process";
import { readFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const fixtures = resolve(process.argv[2] ?? "");
const out = resolve(process.argv[3] ?? "prints");
if (!process.argv[2]) {
  console.error("uso: node scripts/screenshots-v12.mjs <FIXTURE_DIR> [saida]");
  process.exit(1);
}
mkdirSync(out, { recursive: true });
const pw = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
const chromium = pw.chromium ?? pw.default.chromium;
const read = (f) => readFileSync(join(fixtures, f), "utf8");
const scan = JSON.parse(read("scan.json"));
const data = {
  scan,
  detail: Object.fromEntries(scan.map((m) => [m.name, JSON.parse(read(`detail-${m.name}.json`))])),
  thumb: Object.fromEntries(scan.map((m) => [m.name, read(`thumb-${m.name}.svg`)])),
  mesh: JSON.parse(read("mesh-de_museu.json")),
  entities: JSON.parse(read("entities-de_museu.json")),
};

const port = 4900 + Math.floor(Math.random() * 300);
const server = spawn("bunx", ["vite", "preview", "--port", String(port), "--strictPort"], { stdio: "ignore" });
const base = `http://localhost:${port}/`;
for (let i = 0; i < 80; i++) {
  try {
    if ((await fetch(base)).ok) break;
  } catch {}
  await new Promise((r) => setTimeout(r, 250));
}

const mock = `(() => {
  const D = ${JSON.stringify(data)};
  const settings = { last_dir: "/jogos/cstrike/maps", annotations: {}, theme: null, lang: null, slots: 32 };
  const nameOf = (p) => String(p).split(/[\\\\/]/).pop().replace(/\\.bsp$/, "");
  const SKY = { up: ["#2f6fcf", "#9cc4f2"], dn: ["#8a6a3a", "#c9a56a"], lf: ["#3d7fd6", "#f2b36d"], rt: ["#3d7fd6", "#f2b36d"], ft: ["#4a86d8", "#f6c27a"], bk: ["#3d7fd6", "#f2b36d"] };
  const face = (name) => {
    const f = name.slice(-7, -5);
    const [a, b] = SKY[f] ?? ["#444", "#888"];
    const c = document.createElement("canvas"); c.width = c.height = 128;
    const g = c.getContext("2d");
    const grad = g.createLinearGradient(0, 0, 0, 128);
    grad.addColorStop(0, a); grad.addColorStop(1, b);
    g.fillStyle = grad; g.fillRect(0, 0, 128, 128);
    g.fillStyle = "rgba(255,255,255,.55)";
    for (let i = 0; i < 5; i++) { g.beginPath(); g.ellipse(20 + i * 24, 40 + ((i * 17) % 40), 22, 8, 0, 0, 7); g.fill(); }
    return c.toDataURL("image/png");
  };
  const E = (path, kind, size, layer = "mod") => ({ path, name: path.split("/").pop(), kind, size, layer, origin: layer === "valve" ? "valve" : "mod", pak: null });
  const entries = [
    ...["up", "dn", "lf", "rt", "ft", "bk"].filter((f) => f !== "dn").map((f) => E("gfx/env/desert" + f + ".tga", "image", 196000)),
    E("gfx/env/snowup.tga", "image", 64000), E("gfx/vgui/logos/dust.bmp", "image", 4400), E("overviews/de_museu.bmp", "image", 66000),
    E("maps/de_museu.bsp", "map", 1900000), E("liblist.gam", "cfg", 1180),
  ];
  const listing = { mod_root: "/jogos/cstrike", valve_root: null, entries, totals: [], truncated: false, warnings: [] };
  window.__TAURI_INTERNALS__ = { transformCallback: () => 0, invoke: async (cmd, a = {}) => {
    switch (cmd) {
      case "load_settings": return settings;
      case "save_settings": return null;
      case "scan_maps": return D.scan;
      case "map_thumbnail": return D.thumb[nameOf(a.path)];
      case "map_detail": return D.detail[nameOf(a.path)] ?? D.detail.de_museu;
      case "map_mesh": return D.mesh;
      case "map_entities": return D.entities;
      case "list_mod_files": return listing;
      case "load_image": {
        const p = String(a.path);
        if (/dn\\.(tga|bmp)$/i.test(p)) throw "arquivo não encontrado";
        return { format: "tga", width: 128, height: 128, bits_per_pixel: 24, has_alpha: false, rle: false, palette: "none", png: face(p) };
      }
      default: return null;
    }
  } };
})();`;

const browser = await chromium.launch({ args: ["--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
let errors = 0;
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, locale: "pt-BR" });
  page.on("pageerror", (e) => {
    errors++;
    console.error("pageerror:", e.message);
  });
  await page.addInitScript(mock);
  await page.goto(base);
  await page.waitForSelector(".card");
  const pause = (ms) => page.waitForTimeout(ms);
  const shot = async (name) => {
    await page.screenshot({ path: join(out, name) });
    console.log("ok", name);
  };

  // 1. Claude Link
  await page.click("#nav-claude");
  await page.waitForSelector(".claude-tool");
  await pause(600);
  await shot("v12-claude-link.png");
  await page.fill(".claude-search", "ceu");
  await pause(300);
  await shot("v12-claude-link-busca.png");

  // 2. cubo de céu: abre desertbk.tga na aba Imagens (falta o lado dn)
  await page.click("#nav-skins");
  await page.waitForSelector('[data-tab="images"]');
  await page.click('[data-tab="images"]');
  await page.waitForSelector(".exp-list .vrow");
  await page.locator(".exp-list .vrow", { hasText: "desertbk.tga" }).first().click();
  await page.waitForSelector(".sky-cface");
  await pause(1200);
  await shot("v12-ceu-cubo.png");

  // 3. HUD do viewer 3D (câmera x y z + yaw)
  await page.click("#nav-maps");
  await page.waitForSelector(".card");
  await page.locator(".card-open", { hasText: "de_museu" }).first().click();
  await page.waitForSelector('[data-view="tex"]');
  await page.click('[data-view="tex"]');
  await page.waitForSelector(".hud");
  await pause(2500);
  await shot("v12-hud-3d.png");
} finally {
  await browser.close();
  server.kill();
}
process.exit(errors ? 1 : 0);
