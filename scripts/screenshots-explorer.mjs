// Capturas da seção Explorador sem precisar do app Tauri nem de fixtures do cargo.
//
//   1. bun run build
//   2. node scripts/screenshots-explorer.mjs [pasta-de-saida]      (padrão: prints)
//
// O frontend de verdade roda no Chromium; só o `invoke` do Tauri é trocado por um simulador
// que devolve respostas sintéticas dos comandos novos (list_mod_files, analyze_mod_file,
// read_mod_audio, list_wad, wad_texture, wad_usage, load_sprite, load_image, read_mod_file).
// Imagens e o WAV são gerados dentro da página (canvas), então nada do jogo vai pro repositório.
// Precisa de `playwright` (global ou local); PLAYWRIGHT_MODULE aponta para outro caminho.
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const out = resolve(process.argv[2] ?? "prints");
mkdirSync(out, { recursive: true });
const pw = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
const chromium = pw.chromium ?? pw.default.chromium; // módulo CJS: o export nomeado some

const port = 4700 + Math.floor(Math.random() * 300);
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

  // ---------- geradores (canvas) ----------
  const hash = (s) => { let h = 7; for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0; return h; };
  function png(w, h, draw) {
    const c = document.createElement("canvas"); c.width = w; c.height = h;
    draw(c.getContext("2d"), w, h); return c.toDataURL("image/png");
  }
  function texture(name, w, h, transparent) {
    const hu = hash(name) % 360;
    return png(w, h, (g) => {
      g.fillStyle = "hsl(" + hu + " 35% 32%)"; g.fillRect(0, 0, w, h);
      g.fillStyle = "hsl(" + hu + " 40% 46%)";
      const s = 16; for (let y = 0; y < h; y += s) for (let x = ((y / s) % 2) * (s / 2); x < w; x += s) g.fillRect(x + 1, y + 1, s - 2, s / 2 - 2);
      g.strokeStyle = "hsl(" + hu + " 50% 70%)"; g.lineWidth = 2; g.strokeRect(1, 1, w - 2, h - 2);
      if (transparent) { g.clearRect(0, 0, w, h); g.strokeStyle = "#e9c46a"; g.lineWidth = 3; for (let x = 4; x < w; x += 12) { g.beginPath(); g.moveTo(x, 0); g.lineTo(x, h); g.stroke(); } g.beginPath(); g.moveTo(0, h / 2); g.lineTo(w, h / 2); g.stroke(); }
    });
  }
  function glow(size, phase) {
    return png(size, size, (g) => {
      g.fillStyle = "#000"; g.fillRect(0, 0, size, size);
      const r = size * (0.18 + 0.3 * phase);
      const grad = g.createRadialGradient(size / 2, size / 2, 0, size / 2, size / 2, r);
      grad.addColorStop(0, "#fff7c2"); grad.addColorStop(0.5, "#ff9a2e"); grad.addColorStop(1, "#000");
      g.fillStyle = grad; g.fillRect(0, 0, size, size);
    });
  }
  function wavDataUrl(rate, secs) {
    const n = Math.floor(rate * secs), buf = new ArrayBuffer(44 + n * 2), v = new DataView(buf);
    const str = (o, s) => { for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i)); };
    str(0, "RIFF"); v.setUint32(4, 36 + n * 2, true); str(8, "WAVE"); str(12, "fmt "); v.setUint32(16, 16, true);
    v.setUint16(20, 1, true); v.setUint16(22, 1, true); v.setUint32(24, rate, true); v.setUint32(28, rate * 2, true);
    v.setUint16(32, 2, true); v.setUint16(34, 16, true); str(36, "data"); v.setUint32(40, n * 2, true);
    for (let i = 0; i < n; i++) { const t = i / rate; const env = Math.min(1, t * 8) * Math.exp(-t * 0.9) * (0.6 + 0.4 * Math.sin(t * 9)); v.setInt16(44 + i * 2, Math.sin(2 * Math.PI * (180 + 120 * t) * t) * env * 12000, true); }
    let bin = ""; const u = new Uint8Array(buf); for (let i = 0; i < u.length; i++) bin += String.fromCharCode(u[i]);
    return "data:audio/wav;base64," + btoa(bin);
  }
  function envelope(secs) {
    const out = [], N = 512;
    for (let i = 0; i < N; i++) { const t = (i / N) * secs; const e = Math.min(1, t * 8) * Math.exp(-t * 0.9) * (0.6 + 0.4 * Math.sin(t * 9)) * 0.37; out.push([-e, e]); }
    return out;
  }

  // ---------- dados ----------
  const E = (path, kind, size, layer = "mod", pak = null) => ({ path, name: path.split("/").pop(), kind, size, layer, origin: layer === "valve" ? "valve" : "mod", pak });
  const entries = [
    E("liblist.gam", "cfg", 1180), E("mapcycle.txt", "text", 410), E("motd.txt", "text", 880), E("config.cfg", "cfg", 5200),
    E("maps/de_dust2.bsp", "map", 6_400_000), E("maps/de_museu.bsp", "map", 1_900_000), E("maps/cs_assault.bsp", "map", 3_800_000),
    E("maps/de_dust2.nav", "nav", 220_000), E("maps/de_museu.nav", "nav", 48_000), E("maps/de_museu.txt", "text", 340),
    E("cstrike.wad", "wad", 12_400_000), E("decals.wad", "wad", 380_000), E("custom.wad", "wad", 150_000),
    E("sprites/flame.spr", "sprite", 9200), E("sprites/muzzleflash1.spr", "sprite", 42_000), E("sprites/hud.txt", "text", 2200),
    E("sprites/arrow_facing.spr", "sprite", 6200), E("sprites/arrow_oriented.spr", "sprite", 6200), E("sprites/smoke.spr", "sprite", 18_000), E("sprites/laserdot.spr", "sprite", 3100),
    E("models/player/leet/leet.mdl", "model", 410_000), E("models/w_ak47.mdl", "model", 88_000),
    E("sound/ambience/wind2.wav", "sound", 88_300), E("sound/ambience/wind1.wav", "sound", 64_000), E("sound/weapons/ak47-1.wav", "sound", 21_800),
    E("sound/weapons/deagle-1.wav", "sound", 19_000), E("sound/radio/go.wav", "sound", 12_000), E("sound/music/theme.mp3", "sound", 2_400_000),
    E("gfx/vgui/logos/dust.bmp", "image", 4400), E("gfx/env/desertbk.tga", "image", 196_000), E("overviews/de_dust2.bmp", "image", 66_000),
    E("demos/scrim.dem", "demo", 2_200_000), E("dlls/mp.dll", "dll", 1_400_000), E("cl_dlls/client.dll", "dll", 880_000),
    E("pak0.pak", "pak", 25_000_000), E("sound/ambience/wind2.wav", "sound", 88_300, "valve"), E("sprites/flame.spr", "sprite", 9200, "valve"),
    E("halflife.wad", "wad", 7_900_000, "valve"), E("decals.wad", "wad", 360_000, "valve"), E("liblist.gam", "cfg", 1100, "valve"),
    E("sound/player/pl_step1.wav", "sound", 14_000, "pak", "pak0.pak"), E("sound/player/pl_step2.wav", "sound", 14_100, "pak", "pak0.pak"),
    E("sprites/steam1.spr", "sprite", 12_000, "pak", "pak0.pak"),
  ];
  for (let i = 1; i <= 140; i++) entries.push(E("sound/ambience/amb" + String(i).padStart(3, "0") + ".wav", "sound", 20_000 + i * 311));
  const listing = { mod_root: "/jogos/cstrike", valve_root: "/jogos/valve", entries, totals: [], truncated: false, warnings: ["link simbólico para fora da raiz ignorado: maps/atalho"] };

  const wadNames = ["WALL_BRICK1", "WALL_BRICK2", "WALL_STONE", "FLOOR_TILE", "FLOOR_WOOD", "CEILING1", "DOOR_METAL", "DOOR_WOOD", "CRATE01", "CRATE02", "BARREL", "LAVA1", "!WATER1", "{FENCE01", "{GRATE", "{LEAVES", "SKY", "TRIM_RED", "TRIM_BLUE", "WINDOW1", "METAL_PLATE", "CONCRETE1", "CONCRETE2", "GRASS", "SAND1", "DIRT", "PIPE1", "VENT1", "LAMP01", "SIGN_EXIT", "+0BUTTON", "+0LIGHT", "{BLUE", "DUST_WALL", "DUST_FLOOR", "ARAB_ARCH", "COBBLE", "PLASTER", "STEEL", "TILE_WHITE"];
  const wadEntries = wadNames.map((n, i) => ({ name: n, kind: "miptex", type_code: 0x43, width: i % 5 === 0 ? 128 : 64, height: i % 7 === 0 ? 128 : 64, transparent: n[0] === "{", size: 6000 + i * 70, compressed: false, decodable: true }));
  wadEntries.push({ name: "PALETTE", kind: "palette", type_code: 0x40, width: 0, height: 0, transparent: false, size: 768, compressed: false, decodable: false });
  const wadListing = (path) => ({ path, name: path.split("/").pop(), file_size: 380000, magic: "WAD3", lump_count: wadEntries.length, texture_count: wadNames.length, entries: wadEntries });

  const nav = { type: "nav", version: 5, bsp_size: 1_900_000, bsp_size_matches: true, parsed: true, area_count: 412, connection_count: 1620, hiding_spot_count: 233, approach_count: 87, encounter_count: 12, places: ["BombsiteA", "BombsiteB", "CTSpawn", "TSpawn", "Long", "Mid", "Catwalk"], bounds: [-2048, -1536, -64, 2304, 1792, 512],
    areas: Array.from({ length: 30 }, (_, i) => ({ id: i + 1, attributes: i % 5 === 0 ? 1 : i % 7 === 0 ? 2 : 0, min: [i * 40, 0, 0], max: [i * 40 + 96, 80 + (i % 4) * 24, 0], ne_z: 0, sw_z: 0, connections: [i, i + 2, 0, 1], hiding_spots: i % 3, place: i % 2 ? "Mid" : "BombsiteA" })), areas_truncated: false, warnings: [] };
  const liblist = { type: "liblist", entries: [["game", "Counter-Strike"], ["startmap", "de_dust2"], ["trainmap", "de_dust2"], ["gamedll", "dlls\\\\mp.dll"], ["gamedll_linux", "dlls/cs.so"], ["type", "multiplayer_only"], ["version", "1.6"], ["url_info", "http://www.counter-strike.net"]].map(([key, value]) => ({ key, value })), game: "Counter-Strike", gamedll: "dlls\\\\mp.dll", gamedll_linux: "dlls/cs.so", startmap: "de_dust2", trainmap: "de_dust2", game_type: "multiplayer_only", version: "1.6", url_info: "http://www.counter-strike.net", url_dl: null, warnings: [] };
  const hud = { type: "hud_sprites", declared_count: 4, entries: [{ name: "flame", resolution: 640, file: "flame", sprite_path: "sprites/flame.spr", x: 0, y: 0, width: 32, height: 32 }, { name: "smoke", resolution: 640, file: "smoke", sprite_path: "sprites/smoke.spr", x: 32, y: 0, width: 32, height: 32 }, { name: "ghost", resolution: 640, file: "ghost", sprite_path: "sprites/ghost.spr", x: 0, y: 32, width: 16, height: 16 }], skipped: 1, warnings: [] };
  const audioInfo = (name, secs) => ({ type: "audio", container: name.endsWith(".mp3") ? "mp3" : "wav", codec: name.endsWith(".mp3") ? "mp3" : "pcm", format_tag: 1, channels: 1, sample_rate: 22050, bits_per_sample: 16, block_align: 2, bitrate_kbps: null, sample_frames: Math.floor(22050 * secs), duration_secs: secs, duration_approx: false, size_bytes: 88300, playable: true,
    cue_points: [{ id: 1, sample_offset: Math.floor(22050 * secs * 0.18) }], loops: [{ start_sample: Math.floor(22050 * secs * 0.18), end_sample: null, source: "cue" }], tags: [{ key: "INAM", value: "Vento do deserto" }, { key: "IART", value: "bsp-museum (sintético)" }], peak: 0.37, envelope: envelope(secs), warnings: [] });

  const analysis = (rel) => {
    const name = rel.split("/").pop().toLowerCase();
    const info = { path: rel, name, kind: "other", size: 1000, layer: "mod", origin: "mod", pak: null };
    if (name.endsWith(".wav") || name.endsWith(".mp3")) return { ...info, kind: "sound", analysis: audioInfo(name, 4) };
    if (name.endsWith(".nav")) return { ...info, kind: "nav", analysis: nav };
    if (name === "liblist.gam") return { ...info, kind: "cfg", analysis: liblist };
    if (name === "hud.txt") return { ...info, kind: "text", analysis: hud };
    if (name === "mapcycle.txt") return { ...info, kind: "text", analysis: { type: "map_cycle", entries: [{ map: "de_dust2", min_players: null, max_players: null }, { map: "de_museu", min_players: 4, max_players: 24 }, { map: "cs_assault", min_players: null, max_players: 16 }], warnings: [] } };
    if (name === "pak0.pak") return { ...info, kind: "pak", analysis: { type: "pak", file_size: 25_000_000, skipped: 0, warnings: [], entries: [
      ...Array.from({ length: 60 }, (_, i) => ({ name: "sound/player/pl_step" + (i + 1) + ".wav", offset: 12 + i * 100, size: 14_000 + i })),
      { name: "sprites/steam1.spr", offset: 9000, size: 12000 }, { name: "gfx/shell.lmp", offset: 9100, size: 4100 }, { name: "default.cfg", offset: 9200, size: 320 }] } };
    return { ...info, kind: "text", analysis: { type: "text", text: "// " + name + "\\nlinha de exemplo\\nsegunda linha", line_count: 3, bytes: 40, truncated: false } };
  };

  // seta com "UP" no topo e uma barra na base: dá para ver para onde o quad aponta no 3D
  function arrow(w, h) {
    return png(w, h, (g) => {
      g.fillStyle = "rgba(0,0,0,0)"; g.clearRect(0, 0, w, h);
      g.fillStyle = "#e9c46a"; g.beginPath(); g.moveTo(w / 2, 2); g.lineTo(w - 4, h * 0.45); g.lineTo(w * 0.68, h * 0.45); g.lineTo(w * 0.68, h - 14); g.lineTo(w * 0.32, h - 14); g.lineTo(w * 0.32, h * 0.45); g.lineTo(4, h * 0.45); g.closePath(); g.fill();
      g.strokeStyle = "#7a4b0a"; g.lineWidth = 3; g.stroke();
      g.fillStyle = "#2a9d8f"; g.fillRect(4, h - 10, w - 8, 8);
      g.fillStyle = "#3a2a05"; g.font = "bold 14px sans-serif"; g.textAlign = "center"; g.fillText("UP", w / 2, h * 0.62);
    });
  }
  const sprite = (rel) => {
    if (rel.includes("arrow_")) {
      const oriented = rel.includes("oriented");
      return { version: 2, sprite_type: oriented ? "oriented" : "facing_upright", tex_format: oriented ? "index_alpha" : "alpha_test", bounding_radius: 56, max_width: 64, max_height: 96, frame_count: 1, beam_length: 0, sync_type: "sync", additive: false, palette_colors: 256, total_duration: 0,
        frames: [{ kind: "single", durations: [], images: [{ origin_x: -32, origin_y: 96, width: 64, height: 96, png: arrow(64, 96) }] }] };
    }
    const additive = !rel.includes("laserdot");
    const frames = Array.from({ length: 8 }, (_, i) => ({ kind: "single", durations: [], images: [{ origin_x: -32, origin_y: 32, width: 64, height: 64, png: glow(64, Math.abs(Math.sin(i / 8 * Math.PI))) }] }));
    return { version: 2, sprite_type: "parallel_upright", tex_format: additive ? "additive" : "alpha_test", bounding_radius: 24, max_width: 64, max_height: 64, frame_count: 8, beam_length: 0, sync_type: "sync", additive, palette_colors: 256, total_duration: 0, frames };
  };

  const calls = { usage: 0 };
  window.__calls = calls;
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 0,
    invoke: async (cmd, a = {}) => {
      switch (cmd) {
        case "load_settings": return settings;
        case "save_settings": return null;
        case "scan_maps": return [];
        case "list_mod_files": return listing;
        case "analyze_mod_file": return analysis(a.rel);
        case "read_mod_audio": return { mime: "audio/wav", size_bytes: 88300, data_url: wavDataUrl(22050, 4) };
        case "read_mod_file": return { path: a.rel, name: a.rel.split("/").pop(), kind: "other", size: 1000, layer: "mod", origin: "mod", pak: null, bytes_read: 128, truncated: true, is_text: false, text: null, base64: btoa(Array.from({ length: 128 }, (_, i) => String.fromCharCode((i * 7 + 33) & 255)).join("")) };
        case "load_image": return { format: "bmp", width: 128, height: 128, bits_per_pixel: 8, has_alpha: false, rle: false, palette: "embedded", png: texture("overview", 128, 128, false) };
        case "load_sprite": return sprite(a.path);
        case "list_wad": return wadListing(a.path);
        case "wad_texture": {
          const e = wadEntries.find((x) => x.name === a.name);
          const mips = a.withMips ? [1, 2, 3].map((l) => ({ level: l, width: e.width >> l, height: e.height >> l, png: texture(a.name, e.width >> l, e.height >> l, e.transparent) })) : [];
          return { name: e.name, kind: e.kind, width: e.width, height: e.height, transparent: e.transparent, png: texture(e.name, e.width, e.height, e.transparent), mips };
        }
        case "wad_usage": {
          calls.usage++;
          await new Promise((r) => setTimeout(r, 700));
          return { wad: a.wadPath, maps_dir: a.mapsDir, maps_scanned: 38, maps_unreadable: 1, truncated: false, unused_count: 9,
            textures: wadNames.map((n, i) => ({ name: n, map_count: i % 4 === 3 ? 0 : 1 + (i * 3) % 17, maps: i % 4 === 3 ? [] : ["de_dust2", "de_museu", "cs_assault", "de_inferno"].slice(0, 1 + (i % 4)) })) };
        }
        case "plugin:dialog|open": return "/jogos/cstrike";
        default: return null;
      }
    },
  };
})();
`;

const browser = await chromium.launch({ args: ["--no-sandbox", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist", "--autoplay-policy=no-user-gesture-required"] });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
let errors = 0;
page.on("pageerror", (e) => {
  errors++;
  console.error("pageerror:", e.message);
});
page.on("console", (m) => {
  if (m.type() === "error") {
    errors++;
    console.error("console.error:", m.text());
  }
});
await page.addInitScript(mock);
await page.goto(base);
const pause = (ms) => page.waitForTimeout(ms);
const shot = async (name, target = page) => {
  await target.screenshot({ path: join(out, name) });
  console.log("ok", name);
};

await page.click("#nav-explorer");
await page.waitForSelector(".exp-list .vrow");
await pause(300);

// 1. Arquivos do mod: lista, abre o liblist.gam
await page.fill(".f-query", "liblist");
await pause(300);
await page.click(".exp-list .vrow >> nth=0");
await page.waitForSelector(".exp-facts");
await page.fill(".f-query", "");
await pause(300);
await shot("v7-explorador-arquivos.png");

// 2. árvore filtrando por nav, abre o .nav
await page.click('.f-mode [data-mode="tree"]');
await page.fill(".f-query", "nav");
await pause(300);
await page.locator(".exp-list .vrow", { hasText: "de_museu.nav" }).click();
await page.waitForSelector(".exp-table");
await pause(300);
await shot("v7-explorador-arvore-nav.png");
await page.fill(".f-query", "");
await page.click('.f-mode [data-mode="list"]');

// 3. pak: abre o pak0.pak e seleciona um membro de áudio
await page.fill(".f-query", "pak0.pak");
await pause(250);
await page.locator(".exp-list .vrow", { hasText: "pak0.pak" }).first().click();
await page.waitForSelector(".pak-list .vrow");
await pause(300);
await shot("v7-explorador-pak.png");
await page.fill(".f-query", "");

// 4. WAD: grade
await page.click('[data-tab="wad"]');
await page.waitForSelector(".wad-tile.loaded", { timeout: 5000 });
await pause(800);
await shot("v7-explorador-wad-grade.png");

// 5. WAD: ampliado + mapas que usam
await page.locator(".wad-tile", { hasText: "WALL_BRICK2" }).click();
await page.waitForSelector(".big-img");
await page.click(".act-usage");
await page.waitForSelector(".wad-usedby .exp-chips", { timeout: 5000 });
await pause(300);
await shot("v7-explorador-wad-ampliado.png");
await page.keyboard.press("Escape");
await pause(200);
await page.check(".wad-transp");
await pause(300);
await shot("v7-explorador-wad-transparentes.png");
await page.uncheck(".wad-transp");

// 6. Sprites
await page.click('[data-tab="sprites"]');
await page.waitForSelector(".exp-list .vrow");
await page.locator(".exp-list .vrow", { hasText: "flame.spr" }).first().click();
await page.waitForSelector(".spr-canvas");
await pause(900);
await shot("v7-explorador-sprites.png");

await page.setViewportSize({ width: 1440, height: 1060 }); // o painel de controles do 3D é alto
// 6b. Sprites em 3D: tipo facing_upright (a câmera do mundo aparece como frustum laranja)
await page.locator(".exp-list .vrow", { hasText: "arrow_facing.spr" }).first().click();
await page.waitForSelector(".spr-mode-btn");
await page.click('.spr-mode-btn[data-mode="3d"]');
await page.waitForSelector(".s3d-canvas");
await pause(600);
await shot("v8-sprite-3d.png");

// 6c. aditivo (flame) com a câmera girando sozinha, fundo escuro
await page.locator(".exp-list .vrow", { hasText: "flame.spr" }).first().click();
await page.waitForSelector(".spr-mode-btn");
await page.click('.spr-mode-btn[data-mode="3d"]');
await page.waitForSelector(".s3d-canvas");
await page.check(".s3d-auto");
await pause(1500);
await shot("v8-sprite-3d-aditivo.png");

// 6d. oriented com angles editados
await page.locator(".exp-list .vrow", { hasText: "arrow_oriented.spr" }).first().click();
await page.waitForSelector(".spr-mode-btn");
await page.click('.spr-mode-btn[data-mode="3d"]');
await page.waitForSelector(".s3d-angle0");
await page.fill(".s3d-angle0", "-25");
await page.fill(".s3d-angle1", "50");
await page.fill(".s3d-angle2", "15");
await pause(500);
await shot("v8-sprite-3d-orientado.png");

// 6e. visão da câmera do jogo, volta ao 2D e troca de sprite (exercita o dispose do WebGL)
await page.selectOption(".s3d-view", "game");
await pause(300);
await shot("v8-sprite-3d-camera-jogo.png");
await page.click('.spr-mode-btn[data-mode="2d"]');
await page.waitForSelector(".spr-canvas", { state: "visible" });
await page.locator(".exp-list .vrow", { hasText: "smoke.spr" }).first().click();
await pause(400);
await page.setViewportSize({ width: 1440, height: 900 });

// 7. Áudio
await page.click('[data-tab="audio"]');
await page.waitForSelector(".exp-list .vrow");
await page.fill(".m-query", "wind2");
await pause(300);
await page.locator(".exp-list .vrow", { hasText: "wind2.wav" }).first().click();
await page.waitForSelector(".aud-wave");
await pause(600);
await page.evaluate(() => {
  const a = document.querySelector(".aud-el");
  if (a) a.currentTime = 1.4;
});
await pause(500);
await shot("v7-explorador-audio.png");

// 8. inglês + tema claro
await page.click("#lang-toggle");
await page.click("#theme-toggle");
await pause(500);
await shot("v7-explorador-audio-en-claro.png");

await browser.close();
server.kill();
if (errors) {
  console.error(errors + " erro(s) no console");
  process.exit(1);
}
