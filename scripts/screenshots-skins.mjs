// Capturas da visualização de skins do .mdl (aba Recursos) sem precisar do app Tauri nem de fixtures do cargo.
//
//   1. bun run build
//   2. node scripts/screenshots-skins.mjs [pasta-de-saida]      (padrão: prints)
//
// O frontend de verdade roda no Chromium (WebGL por software); só o `invoke` do Tauri é trocado por um simulador
// que devolve um modelo sintético rico: boneco de caixas com 7 texturas desenhadas na própria página (canvas),
// 3 famílias de skin, texturas de remap (cores de time top/bottom), chrome e máscara. Nada do jogo entra no repositório.
// Além das imagens, o script confere: contextos WebGL vivos (galeria compartilhada = 1 contexto extra), troca de skin,
// divisor arrastável, realce do triângulo no mapa UV e liberação dos contextos ao fechar o modelo.
// Precisa de `playwright` (global ou local); PLAYWRIGHT_MODULE aponta para outro caminho.
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const out = resolve(process.argv[2] ?? "prints");
mkdirSync(out, { recursive: true });
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");

const port = 4300 + Math.floor(Math.random() * 300);
const server = spawn("bunx", ["vite", "preview", "--port", String(port), "--strictPort"], { stdio: "ignore" });
const base = `http://localhost:${port}/`;
for (let i = 0; i < 80; i++) {
  try {
    if ((await fetch(base)).ok) break;
  } catch {}
  await new Promise((r) => setTimeout(r, 250));
}

const mock = `
(() => {
  const settings = { last_dir: "/jogos/cstrike/maps", slots: 32, lang: "pt", theme: "dark", annotations: {} };

  // contextos WebGL criados (para conferir que nenhum fica vivo depois de fechar o modelo)
  window.__gl = [];
  const origGet = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function (type, ...rest) {
    const ctx = origGet.call(this, type, ...rest);
    if (ctx && /webgl/.test(String(type))) window.__gl.push(ctx);
    return ctx;
  };
  window.__glAlive = () => window.__gl.filter((g) => !g.isContextLost()).length;

  // ---------- texturas (canvas) ----------
  const hue = (b) => Math.round((b / 255) * 360);
  const cache = new Map();
  function png(key, w, h, draw) {
    if (cache.has(key)) return cache.get(key);
    const c = document.createElement("canvas"); c.width = w; c.height = h;
    draw(c.getContext("2d"), w, h);
    const url = c.toDataURL("image/png");
    cache.set(key, url);
    return url;
  }
  /** grade de células; cada célula tem 3x2 faces (mesmo layout dos UVs gerados abaixo) */
  function faces(g, w, n, paint) {
    const cols = Math.ceil(Math.sqrt(n)), rows = Math.ceil(n / cols);
    const cw = w / cols, ch = w / rows;
    for (let idx = 0; idx < n; idx++) {
      const cx = idx % cols, cy = Math.floor(idx / cols);
      for (let f = 0; f < 6; f++) {
        const x = cx * cw + (f % 3) * (cw / 3), y = cy * ch + Math.floor(f / 3) * (ch / 2);
        paint(x, y, cw / 3, ch / 2, f, idx);
      }
    }
  }
  function remapTex(name, top, bottom, base) {
    return png("remap|" + name + "|" + top + "|" + bottom, 128, 128, (g, w) => {
      g.fillStyle = base; g.fillRect(0, 0, w, w);
      faces(g, w, 3, (x, y, fw, fh, f, idx) => {
        g.fillStyle = "hsl(" + hue(top) + " 80% " + (42 + idx * 4) + "%)"; g.fillRect(x + 1, y + 1, fw - 2, fh * 0.42);
        g.fillStyle = "hsl(" + hue(bottom) + " 75% " + (36 + idx * 4) + "%)"; g.fillRect(x + 1, y + fh * 0.58, fw - 2, fh * 0.4 - 1);
        g.strokeStyle = "rgba(255,255,255,.55)"; g.strokeRect(x + 0.5, y + 0.5, fw - 1, fh - 1);
        g.fillStyle = "#fff"; g.font = "bold 9px sans-serif"; g.fillText(idx + "." + f, x + 4, y + fh / 2 + 3);
      });
    });
  }
  function plainTex(key, w, color, accent) {
    return png("plain|" + key, w, w, (g) => {
      g.fillStyle = color; g.fillRect(0, 0, w, w);
      faces(g, w, key === "head" || key === "head2" ? 1 : 1, (x, y, fw, fh, f) => {
        g.fillStyle = accent; g.fillRect(x + fw * 0.2, y + fh * 0.25, fw * 0.6, fh * 0.2);
        g.strokeStyle = "rgba(0,0,0,.45)"; g.strokeRect(x + 0.5, y + 0.5, fw - 1, fh - 1);
      });
    });
  }
  const chromeTex = () => png("chrome", 64, 64, (g, w) => {
    const grad = g.createRadialGradient(w * 0.35, w * 0.3, 2, w / 2, w / 2, w * 0.7);
    grad.addColorStop(0, "#ffffff"); grad.addColorStop(0.35, "#9fc2e6"); grad.addColorStop(0.7, "#2b4a6e"); grad.addColorStop(1, "#0b1622");
    g.fillStyle = grad; g.fillRect(0, 0, w, w);
  });
  const gridTex = () => png("grid", 64, 64, (g, w) => {
    g.fillStyle = "#b9a46a"; g.fillRect(0, 0, w, w);
    g.clearRect(0, 0, w, w);
    g.fillStyle = "#d9c27a";
    for (let x = 0; x < w; x += 8) g.fillRect(x, 0, 3, w);
    for (let y = 0; y < w; y += 8) g.fillRect(0, y, w, 3);
  });

  const RANGE = { top_start: 160, top_end: 191, bottom_end: 223 };
  const info = (name, over = {}) => Object.assign({
    name, flags: 0, flatshade: false, chrome: false, fullbright: false, nomips: false, alpha: false,
    additive: false, masked: false, blend: "opaque", remap: null,
  }, over);
  const TEX_INFO = [
    info("Remap1_160_191_223", { remap: RANGE }),
    info("cabeca"),
    info("visor", { chrome: true, flags: 2 }),
    info("grade", { masked: true, alpha: true, blend: "masked", flags: 0x40 }),
    info("Remap2_160_191_223", { remap: RANGE }),
    info("urbano", { flatshade: true, flags: 1 }),
    info("cabeca2"),
  ];
  const textures = (top, bottom) => [
    { name: "Remap1_160_191_223", png: remapTex("a", top ?? 90, bottom ?? 30, "#46503c") },
    { name: "cabeca", png: plainTex("head", 64, "#c89a7a", "#3a2a22") },
    { name: "visor", png: chromeTex() },
    { name: "grade", png: gridTex() },
    { name: "Remap2_160_191_223", png: remapTex("b", top ?? 25, bottom ?? 8, "#8a7a52") },
    { name: "urbano", png: plainTex("urban", 64, "#4c5560", "#9aa5b1") },
    { name: "cabeca2", png: plainTex("head2", 64, "#7a5a44", "#e8d9c8") },
  ];
  // família 0 = identidade; 1 troca corpo e cabeça (deserto); 2 usa o corpo urbano
  const FAMILIES = [[0, 1, 2, 3, 4, 5, 6], [4, 6, 2, 3, 4, 5, 6], [5, 1, 2, 3, 4, 5, 6]];

  // ---------- malha: boneco de caixas (Z para cima, unidades do jogo) ----------
  const BOXES = [
    { min: [-6, -3, 0], max: [-1, 3, 30], tex: 0 }, { min: [1, -3, 0], max: [6, 3, 30], tex: 0 },
    { min: [-7, -4, 30], max: [7, 4, 56], tex: 0 },
    { min: [-12, -3, 32], max: [-7, 3, 56], tex: 3 }, { min: [7, -3, 32], max: [12, 3, 56], tex: 3 },
    { min: [-4, -4, 56], max: [4, 4, 68], tex: 1 }, { min: [-3.5, 4, 60], max: [3.5, 4.6, 65], tex: 2 },
  ];
  const QUADS = [[4, 6, 2, 0], [5, 7, 3, 1], [4, 5, 1, 0], [6, 7, 3, 2], [0, 1, 3, 2], [4, 5, 7, 6]]; // v cresce pra baixo nas laterais (z do topo ao pé)
  function mesh() {
    const positions = [], uvs = [], texindex = [];
    const perTex = {};
    for (const b of BOXES) perTex[b.tex] = (perTex[b.tex] ?? 0) + 1;
    const used = {};
    for (const b of BOXES) {
      const idx = (used[b.tex] = (used[b.tex] ?? -1) + 1), n = perTex[b.tex];
      const cols = Math.ceil(Math.sqrt(n)), rows = Math.ceil(n / cols);
      const cx = idx % cols, cy = Math.floor(idx / cols);
      const corner = (i) => [i & 1 ? b.max[0] : b.min[0], i & 2 ? b.max[1] : b.min[1], i & 4 ? b.max[2] : b.min[2]];
      QUADS.forEach((q, f) => {
        const u0 = (cx + (f % 3) / 3) / cols, u1 = (cx + ((f % 3) + 1) / 3) / cols;
        const v0 = (cy + Math.floor(f / 3) / 2) / rows, v1 = (cy + (Math.floor(f / 3) + 1) / 2) / rows;
        const cuv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        for (const t of [[0, 1, 2], [0, 2, 3]]) {
          for (const k of t) { positions.push(...corner(q[k])); uvs.push(...cuv[k]); }
          texindex.push(b.tex);
        }
      });
    }
    return { positions, uvs, texindex };
  }
  const M = mesh();
  const model = (a) => ({
    positions: M.positions, uvs: M.uvs, texindex: M.texindex,
    textures: textures(a.top ?? null, a.bottom ?? null),
    sequences: ["parado"],
    sequence_info: [{ name: "parado", fps: 10, frames: 2, looping: true, blends: 1, group: 0 }],
    local_positions: M.positions,
    vert_bones: new Array(M.texindex.length * 3).fill(0),
    num_bones: 1,
    skin_families: FAMILIES,
    texture_info: TEX_INFO,
    bone_info: [{ name: "root", parent: -1 }],
    selection: { body: a.body ?? 0, skin: 0, top: a.top ?? null, bottom: a.bottom ?? null },
  });
  window.__calls = [];

  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 0,
    invoke: async (cmd, a = {}) => {
      switch (cmd) {
        case "load_settings": return settings;
        case "save_settings": return null;
        case "scan_maps": return [];
        case "list_model_dirs": return [{ name: "models", path: "/fx/models", count: 2 }];
        case "list_models": return ["/fx/models/soldado.mdl", "/fx/models/outro.mdl"];
        case "load_model": window.__calls.push(["load_model", a]); return model(a);
        case "load_sequence": window.__calls.push(["load_sequence", a]);
          return { frames: 2, bones: 1, fps: 10, looping: true, data: [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0.04, 0.9992] };
        case "plugin:dialog|open": return "/jogos/cstrike";
        default: return null;
      }
    },
  };
})();
`;

const browser = await chromium.launch({
  args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist", "--no-sandbox"],
});
const page = await browser.newPage({ viewport: { width: 1440, height: 1280 }, deviceScaleFactor: 1 });
page.on("pageerror", (e) => console.error("pageerror:", e.message));
page.on("console", (m) => {
  if (m.type() === "error") console.error("console:", m.text().slice(0, 300));
});
await page.addInitScript(mock);
await page.goto(base);
const pause = (ms) => page.waitForTimeout(ms);
const shot = async (name, target) => {
  await target.screenshot({ path: join(out, name) });
  console.log("ok", name);
};
const check = (label, ok, extra = "") => {
  console.log(ok ? "OK  " : "FALHA", label, extra);
  if (!ok) process.exitCode = 1;
};

try {
  await page.click("#nav-resources");
  await page.click("#res-pick");
  await page.locator(".res-item", { hasText: "models" }).click();
  await page.locator(".res-item", { hasText: "soldado.mdl" }).click();
  await page.waitForSelector(".skins-dock .skin-tile");
  await pause(2500);
  const res = page.locator("#resources");

  // 1. galeria de miniaturas 3D
  const tiles = await page.locator(".skin-tile").count();
  check("galeria com uma miniatura por família", tiles === 3, String(tiles));
  const alive = await page.evaluate(() => window.__glAlive());
  check("2 contextos WebGL (visualizador + estúdio das miniaturas)", alive === 2, String(alive));
  const painted = await page.evaluate(() =>
    [...document.querySelectorAll(".skin-tile-canvas")].map((c) => {
      const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
      let n = 0;
      for (let i = 3; i < d.length; i += 4) if (d[i] > 0) n++;
      return n;
    }),
  );
  check("miniaturas desenhadas", painted.every((n) => n > 500), painted.join(","));
  const callsBefore = await page.evaluate(() => window.__calls.filter((c) => c[0] === "load_model").length);

  // 2. clicar numa miniatura aplica a skin no visualizador
  await page.locator(".skin-tile").nth(1).click();
  await pause(600);
  check("skin da miniatura vira a do seletor", (await page.inputValue("#res-skin")) === "1");
  check("miniatura ativa marcada", (await page.locator(".skin-tile.on").count()) === 1);
  const callsAfter = await page.evaluate(() => window.__calls.filter((c) => c[0] === "load_model").length);
  check("trocar de skin não pede o modelo de novo", callsAfter === callsBefore, callsBefore + "→" + callsAfter);
  await page.locator(".skin-tile").nth(0).click();
  await pause(300);
  await page.locator(".skins-toggle", { hasText: "mapa UV" }).click();
  await pause(300);
  await page.locator(".skins-toggle", { hasText: "mapa UV" }).click(); // fecha: a captura 1 só mostra a galeria
  await pause(800);
  await shot("v8-skins-3d.png", res);

  // 3. comparar (antes/depois com divisor arrastável)
  await page.locator(".skins-toggle", { hasText: "comparar" }).click();
  await page.locator(".skins-side").nth(1).locator("select").first().selectOption("1");
  await page.locator(".skins-compare select").last().selectOption("split");
  await pause(1500);
  const handle = page.locator(".skin-cmp-handle");
  const box = await handle.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 - 160, box.y + box.height / 2, { steps: 6 });
  await page.mouse.up();
  await pause(500);
  const moved = await handle.boundingBox();
  check("divisor arrastável", moved.x < box.x - 100, `${Math.round(box.x)} → ${Math.round(moved.x)}`);
  await shot("v8-skins-comparar.png", res);

  // 3b. lado a lado
  await page.locator(".skins-compare select").last().selectOption("side");
  await pause(900);
  await shot("v8-skins-lado-a-lado.png", page.locator(".res-viewer3d"));
  await page.locator(".skins-toggle", { hasText: "comparar" }).click();
  await pause(300);

  // 4. equipes (remap): azul 140 × vermelho 0
  await page.locator(".skins-toggle", { hasText: "equipes" }).click();
  await pause(2200);
  const teamCalls = await page.evaluate(() => window.__calls.filter((c) => c[0] === "load_model" && c[1].top != null).map((c) => c[1].top));
  check("equipes pedem o modelo com top 140 e 0", teamCalls.includes(140) && teamCalls.includes(0), teamCalls.join(","));
  await shot("v8-skins-equipes.png", res);
  await page.locator(".skins-toggle", { hasText: "comparar" }).click();
  await pause(400);

  // 5. mapa UV: lista de texturas, malha, realce ao passar o mouse e seleção pelo modelo
  await page.locator(".skins-toggle", { hasText: "mapa UV" }).click();
  await page.waitForSelector(".uv-item");
  await pause(700);
  const items = await page.locator(".uv-item").count();
  check("lista com as 7 texturas", items === 7, String(items));
  const canvas = page.locator(".uv-canvas");
  await canvas.scrollIntoViewIfNeeded();
  const cb = await canvas.boundingBox();
  // varre o canvas até o mouse cair num triângulo
  let hovered = false;
  for (let y = cb.y + 40; y < cb.y + cb.height - 20 && !hovered; y += 14) {
    for (let x = cb.x + 40; x < cb.x + cb.width - 20 && !hovered; x += 14) {
      await page.mouse.move(x, y);
      hovered = await page.evaluate(() => /* o realce é desenhado no canvas; confere pelo amarelo */ (() => {
        const c = document.querySelector(".uv-canvas");
        const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
        for (let i = 0; i < d.length; i += 4) if (d[i] > 240 && d[i + 1] > 190 && d[i + 1] < 230 && d[i + 2] < 100) return true;
        return false;
      })());
    }
  }
  check("passar o mouse realça um triângulo no mapa UV", hovered);
  await pause(300);
  await shot("v8-skins-uv.png", res);
  // clicar na cabeça do modelo seleciona a textura dela na lista
  const view3d = page.locator(".res-viewer3d");
  await view3d.scrollIntoViewIfNeeded();
  const vb = await view3d.boundingBox();
  await page.mouse.click(vb.x + vb.width / 2, vb.y + vb.height * 0.31);
  await pause(500);
  const picked = await page.locator(".uv-item.on .uv-name").innerText();
  check("clique no modelo seleciona a textura no mapa UV", picked === "cabeca", picked);
  await page.locator(".uv-item").nth(3).click();
  await pause(500);

  // 6. idioma/tema: inglês, claro
  await page.click("#lang-toggle");
  await page.click("#theme-toggle");
  await pause(700);
  await shot("v8-skins-en-claro.png", page.locator(".skins-dock"));
  await page.click("#theme-toggle");
  await page.click("#lang-toggle");

  // 7. abrir outro modelo libera os contextos do anterior
  await page.locator(".res-item", { hasText: "outro.mdl" }).click();
  await page.waitForSelector(".skins-dock .skin-tile");
  await pause(1500);
  const alive2 = await page.evaluate(() => window.__glAlive());
  const total = await page.evaluate(() => window.__gl.length);
  check("contextos do modelo anterior liberados", alive2 === 2, `${alive2} vivos de ${total} criados`);
} finally {
  await browser.close();
  server.kill();
}
