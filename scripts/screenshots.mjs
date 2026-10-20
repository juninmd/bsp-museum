// Gera as capturas de tela do README/PR sem precisar do app Tauri.
//
//   1. cargo test gerar_dados_das_capturas -- --ignored   (com FIXTURE_DIR=<pasta>)
//      e, para as capturas v7 da vista 3D (hulls, volumes, luz animada, cena):
//      cargo test gerar_dados_extras_das_capturas -- --ignored   (mesmo FIXTURE_DIR)
//   2. bun run build
//   3. node scripts/screenshots.mjs <FIXTURE_DIR> [pasta-de-saida]
//
// O frontend de verdade roda no Chromium; só o `invoke` do Tauri é trocado por um
// simulador que devolve os JSONs que o backend em Rust gerou no passo 1.
// Precisa de `playwright` (global ou local); PLAYWRIGHT_MODULE aponta para outro caminho.
import { spawn } from "node:child_process";
import { readFileSync, mkdirSync, existsSync } from "node:fs";
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
// .mdl rico (opcional): `cargo test gerar_dados_mdl_rico -- --ignored` com o mesmo FIXTURE_DIR
const hasMdl = existsSync(join(fixtures, "mdl-model-body0.json"));
if (hasMdl) {
  data.mdl = {};
  for (const f of ["model-body0", "model-body1", "model-team", "walk-0", "walk-255", "idle"]) data.mdl[f] = JSON.parse(read(`mdl-${f}.json`));
}
// Mapa sintético dos recursos extras (v0.7): hulls, volumes, lightstyles, cena. Opcional —
// só existe se `cargo test gerar_dados_extras_das_capturas -- --ignored` rodou no mesmo FIXTURE_DIR.
const hasExtras = existsSync(join(fixtures, "mesh-de_extras.json"));
const extras = hasExtras
  ? {
      summary: JSON.parse(read("summary-de_extras.json")),
      detail: JSON.parse(read("detail-de_extras.json")),
      thumb: read("thumb-de_extras.svg"),
      mesh: JSON.parse(read("mesh-de_extras.json")),
      volumes: JSON.parse(read("volumes-de_extras.json")),
      hulls: { 1: JSON.parse(read("hulls-de_extras-1.json")), 2: JSON.parse(read("hulls-de_extras-2.json")) },
      entities: JSON.parse(read("entities-de_extras.json")),
    }
  : null;
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
  const X = ${JSON.stringify(extras)};
  const settings = ${JSON.stringify(settings)};
  const nameOf = (p) => String(p).split(/[\\\\/]/).pop().replace(/\\.bsp$/, "");
  const isExtra = (p) => X && nameOf(p) === "de_extras";
  const meshFor = (p) => (isExtra(p) ? X.mesh : D.mesh[nameOf(p)] ?? D.mesh.de_museu);
  const entFor = (p) => (isExtra(p) ? X.entities : D.entities[nameOf(p)] ?? D.entities.de_museu);
  // sprite "glow" desenhado no próprio navegador: o .spr de verdade não está nas fixtures
  const glowSprite = () => {
    const c = document.createElement("canvas");
    c.width = c.height = 64;
    const g = c.getContext("2d");
    const grad = g.createRadialGradient(32, 32, 0, 32, 32, 32);
    grad.addColorStop(0, "rgba(255,255,255,1)");
    grad.addColorStop(0.35, "rgba(255,160,40,0.85)");
    grad.addColorStop(1, "rgba(0,0,0,1)");
    g.fillStyle = grad;
    g.fillRect(0, 0, 64, 64);
    return {
      version: 2, sprite_type: "parallel", tex_format: "additive", bounding_radius: 32, max_width: 64, max_height: 64,
      frame_count: 1, beam_length: 0, sync_type: "sync", additive: true, palette_colors: 256, total_duration: 0,
      frames: [{ kind: "single", durations: [], images: [{ origin_x: -32, origin_y: 32, width: 64, height: 64, png: c.toDataURL("image/png") }] }],
    };
  };
  const radarFor = (p) => D.radar[nameOf(p)] ?? D.radar.de_museu;
  window.__calls = [];
  const mdlModel = (a) => (a.top != null || a.bottom != null ? D.mdl["model-team"] : (a.body ?? 0) >= 1 ? D.mdl["model-body1"] : D.mdl["model-body0"]);
  const mdlSeq = (a) => {
    const f = a.index === 1 ? D.mdl.idle : (a.blend?.[0] ?? 0) >= 128 ? D.mdl["walk-255"] : D.mdl["walk-0"];
    return a.hitboxes ? f : { ...f, hitboxes: undefined };
  };
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 0,
    invoke: async (cmd, a = {}) => {
      switch (cmd) {
        case "load_settings": return settings;
        case "save_settings": Object.assign(settings, a.settings); return null;
        case "scan_maps": return window.__withExtras && X ? [...D.scan, X.summary] : D.scan;
        case "map_thumbnail": return isExtra(a.path) ? X.thumb : D.thumb[nameOf(a.path)];
        case "map_detail": return isExtra(a.path) ? X.detail : D.detail[nameOf(a.path)];
        case "map_hulls": return X.hulls[a.hull] ?? X.hulls[1];
        case "map_volumes": return X.volumes;
        case "load_sprite": return glowSprite();
        case "map_mesh": return meshFor(a.path);
        case "map_entities": return entFor(a.path);
        case "map_radar": return radarFor(a.path);
        case "audit_folder": return D.audit;
        case "compare_maps": return D.compare;
        case "list_model_dirs": return [{ name: "models", path: "/fx/models", count: 1 }];
        case "list_models": return ["/fx/models/rico.mdl"];
        case "load_model": window.__calls.push(["load_model", a]); return mdlModel(a);
        case "load_sequence": window.__calls.push(["load_sequence", a]); return mdlSeq(a);
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
// erros de shader/WebGL do Three.js saem só no console, não como exceção
page.on("console", (m) => {
  if (m.type() === "error") console.error("console:", m.text().slice(0, 300));
});
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

// 10. visualizador de .mdl (Recursos): controles, hitboxes/attachments e eventos
if (hasMdl) {
  await page.keyboard.press("Escape");
  await pause(300);
  await page.setViewportSize({ width: 1440, height: 1150 });
  await page.click("#nav-resources");
  await page.click("#res-pick");
  await page.locator(".res-item", { hasText: "models" }).click();
  await page.locator(".res-item", { hasText: "rico.mdl" }).click();
  await page.waitForSelector("#res-panel .mdl-sec");
  await pause(800);
  // controladores: boca e blend; bodygroup: o submodel "b"; cor de time (recarrega a textura)
  await page.locator("#res-panel .mdl-sec", { hasText: /controladores|controllers/i }).locator("input[type=range]").nth(2).fill("48");
  await page.locator("#res-panel .mdl-blend input[type=range]").fill("255");
  await page.locator("#res-panel select").first().selectOption("1");
  await page.locator("#res-panel .mdl-sec", { hasText: /cores de time|team colors/i }).locator("input[type=range]").first().fill("30");
  await page.check("#res-panel .mdl-check:nth-of-type(1) input");
  await page.locator("#res-panel .mdl-check input").nth(1).check();
  await pause(1200);
  const calls = await page.evaluate(() => window.__calls.map(([c, a]) => c + " " + JSON.stringify(a)));
  console.log(calls.join("\n"));
  await shot("v7-mdl-controles.png", page.locator("#resources"));
  // inglês, tema claro
  await page.click("#lang-toggle");
  await page.click("#theme-toggle");
  await pause(600);
  await shot("v7-mdl-controles-en-claro.png", page.locator("#resources"));
}

// 10. v0.7 — vista 3D com o painel "Camadas" no mapa sintético dos recursos extras
//     (hulls, volumes, lightstyles, luzes, sprite, som ambiente, porta aditiva, esteira, trem)
if (extras) {
  await page.keyboard.press("Escape");
  await pause(300);
  await page.click("#nav-maps");
  await page.click("#lang-toggle");
  await page.click("#theme-toggle");
  await page.evaluate(() => { window.__withExtras = true; });
  await page.click("#pick");
  await page.waitForSelector('.card[data-path$="de_extras.bsp"]');
  await page.click('.card[data-path$="de_extras.bsp"] .card-open');
  await page.waitForSelector("#plan-holder svg");
  await page.click('[data-view="tex"]');
  await page.waitForSelector("#viewer3d canvas");
  await pause(800);
  await page.locator("#ent-panel summary").click();
  await page.waitForSelector(".ent-row");
  const goTo = async (query) => {
    await page.fill("#ent-search", query);
    await pause(300);
    await page.locator(".ent-go").first().click();
    await pause(1500);
  };
  const layer = (label) => page.locator(".layers-panel label.chk", { hasText: label }).locator("input");
  // (o painel "Camadas" abre e fecha com o botão no canto do viewer)

  const openSection = async (title) => {
    const sec = page.locator(".layers-sec", { hasText: title }).first();
    if (!(await sec.evaluate((e) => e.open))) await sec.locator("summary").click();
  };

  // sala com as duas camadas de luz, hull de pé, zona de compra, luzes, sprite e som
  await goTo("info_player_start");
  await page.click(".layers-btn");
  await openSection("Colisão");
  await page.locator(".hull-btn").first().click();
  await page.waitForFunction(() => /triângulos/.test(document.querySelector(".hull-status")?.textContent ?? ""));
  await openSection("Volumes");
  await layer("Mostrar volumes").check();
  await page.waitForSelector(".vol-chip");
  await openSection("Cena");
  await layer("Luzes").check();
  await layer("Sprites e glows").check();
  await layer("Sons ambientes").check();
  await layer("Decalques").check();
  await pause(1500);
  await shot("v7-3d-camadas.png", viewer);

  // a mesma sala com o painel fechado: luz animada (camada de estilo 1 somada) e estática
  await page.click(".layers-btn");
  await pause(600);
  await shot("v7-3d-luz-animada.png", viewer);
  await page.click(".layers-btn");
  await layer("Luz animada").uncheck();
  await page.click(".layers-btn");
  await pause(600);
  await shot("v7-3d-luz-estatica.png", viewer);

  // SMOKE=1: liga as camadas de movimento/névoa/sol e troca de modo só para ver se o console fica limpo
  if (process.env.SMOKE) {
    await page.click(".layers-btn");
    for (const label of ["Animar portas", "Rotas de trem", "Névoa", "Sol", "Luz animada"]) await layer(label).check();
    await page.locator(".hull-btn").nth(1).click();
    await pause(1500);
    await shot("smoke-tudo.png", viewer);
    await page.click('[data-view="3d"]');
    await page.waitForSelector("#viewer3d canvas");
    await pause(800);
    await shot("smoke-sem-textura.png", viewer);
  }
}

await browser.close();
server.kill();
