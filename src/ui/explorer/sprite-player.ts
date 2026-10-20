// Player de sprites `.spr`: canvas com fundo escolhido, play/pause, fps dos frames simples,
// tempos dos grupos animados e aditivo simulado (`lighter`) conforme a `tex_format`.

import { t } from "../../i18n.ts";
import { buildTimeline, segmentAt, timelineDuration, type SpriteSegment } from "../../lib/explorer.ts";
import type { SpriteInfo, SpriteTexFormat } from "../../types/formats.ts";
import { esc, factsHtml } from "./common.ts";

type Background = "checker" | "dark" | "light" | "gray" | "green";
type Blend = "auto" | "normal" | "additive";

/** Fundo inicial: o aditivo some no xadrez claro, então começa no escuro. */
export function defaultBackground(tex: SpriteTexFormat): Background {
  return tex === "additive" ? "dark" : "checker";
}

const BG_COLOR: Record<Exclude<Background, "checker">, string> = {
  dark: "#05080c",
  light: "#f1f3f5",
  gray: "#7a8793",
  green: "#2b6b3a",
};

function drawBackground(ctx: CanvasRenderingContext2D, w: number, h: number, bg: Background) {
  if (bg !== "checker") {
    ctx.fillStyle = BG_COLOR[bg];
    ctx.fillRect(0, 0, w, h);
    return;
  }
  const s = 8;
  for (let y = 0; y < h; y += s) {
    for (let x = 0; x < w; x += s) {
      ctx.fillStyle = (x / s + y / s) % 2 === 0 ? "#2a323c" : "#39434f";
      ctx.fillRect(x, y, s, s);
    }
  }
}

function typeLabel(type: SpriteInfo["sprite_type"]): string {
  return t(`exp.spr.type.${type}`);
}

function texLabel(tex: SpriteTexFormat): string {
  return t(`exp.spr.tex.${tex}`);
}

/**
 * Monta o player em `host`. Devolve a função que desmonta (para o `requestAnimationFrame`).
 */
export function mountSpritePlayer(host: HTMLElement, info: SpriteInfo, title: string): () => void {
  const groups = info.frames.filter((f) => f.kind === "group").length;
  host.innerHTML = `
    <div class="spr-player">
      <h3 class="exp-title">${esc(title)}</h3>
      <div class="spr-controls">
        <button type="button" class="spr-play" aria-label="${esc(t("exp.spr.pause"))}">${esc(t("exp.spr.pause"))}</button>
        <label class="spr-ctl"><span>${esc(t("exp.spr.frame"))}</span>
          <input type="range" class="spr-scrub" min="0" max="0" step="1" value="0" aria-label="${esc(t("exp.spr.frame"))}" />
          <output class="spr-frame-label">–</output></label>
        <label class="spr-ctl"><span>${esc(t("exp.spr.fps"))}</span>
          <input type="range" class="spr-fps" min="1" max="60" step="1" value="10" aria-label="${esc(t("exp.spr.fps"))}" />
          <output class="spr-fps-label">10</output></label>
        <label class="spr-ctl"><span>${esc(t("exp.spr.zoom"))}</span>
          <select class="spr-zoom">
            <option value="1">1×</option><option value="2" selected>2×</option><option value="4">4×</option><option value="8">8×</option>
          </select></label>
        <label class="spr-ctl"><span>${esc(t("exp.spr.bg"))}</span>
          <select class="spr-bg">
            <option value="checker">${esc(t("exp.spr.bg.checker"))}</option>
            <option value="dark">${esc(t("exp.spr.bg.dark"))}</option>
            <option value="light">${esc(t("exp.spr.bg.light"))}</option>
            <option value="gray">${esc(t("exp.spr.bg.gray"))}</option>
            <option value="green">${esc(t("exp.spr.bg.green"))}</option>
          </select></label>
        <label class="spr-ctl"><span>${esc(t("exp.spr.blend"))}</span>
          <select class="spr-blend">
            <option value="auto">${esc(t("exp.spr.blend.auto", texLabel(info.tex_format)))}</option>
            <option value="normal">${esc(t("exp.spr.blend.normal"))}</option>
            <option value="additive">${esc(t("exp.spr.blend.additive"))}</option>
          </select></label>
        <label class="spr-ctl chk"><input type="checkbox" class="spr-origin" /> <span>${esc(t("exp.spr.origin"))}</span></label>
      </div>
      <div class="spr-stage"><canvas class="spr-canvas" aria-label="${esc(title)}"></canvas></div>
      <div class="spr-strip" role="group" aria-label="${esc(t("exp.spr.strip"))}"></div>
      ${factsHtml([
        [t("exp.spr.version"), String(info.version)],
        [t("exp.spr.type"), esc(typeLabel(info.sprite_type))],
        [t("exp.spr.texFormat"), `${esc(texLabel(info.tex_format))} <code>${esc(info.tex_format)}</code>`],
        [t("exp.spr.size"), `${info.max_width} × ${info.max_height}`],
        [t("exp.spr.frames"), `${info.frame_count}${groups ? ` (${esc(t("exp.spr.groups", groups))})` : ""}`],
        [t("exp.spr.duration"), info.total_duration > 0 ? `${info.total_duration.toFixed(2)} s` : ""],
        [t("exp.spr.radius"), String(info.bounding_radius)],
        [t("exp.spr.sync"), esc(t(`exp.spr.sync.${info.sync_type}`))],
        [t("exp.spr.palette"), String(info.palette_colors)],
        [t("exp.spr.beam"), info.beam_length ? String(info.beam_length) : ""],
      ])}
    </div>`;

  const q = <E extends HTMLElement>(sel: string) => host.querySelector<E>(sel)!;
  const canvas = q<HTMLCanvasElement>(".spr-canvas");
  const ctx = canvas.getContext("2d")!;
  const playBtn = q<HTMLButtonElement>(".spr-play");
  const scrub = q<HTMLInputElement>(".spr-scrub");
  const scrubLabel = q<HTMLOutputElement>(".spr-frame-label");
  const fpsInput = q<HTMLInputElement>(".spr-fps");
  const fpsLabel = q<HTMLOutputElement>(".spr-fps-label");
  const zoomSel = q<HTMLSelectElement>(".spr-zoom");
  const bgSel = q<HTMLSelectElement>(".spr-bg");
  const blendSel = q<HTMLSelectElement>(".spr-blend");
  const originChk = q<HTMLInputElement>(".spr-origin");
  const strip = q<HTMLElement>(".spr-strip");

  // imagens decodificadas, por [frame][imagem]
  const imgs: HTMLImageElement[][] = info.frames.map((f) =>
    f.images.map((im) => {
      const el = new Image();
      el.src = im.png;
      el.addEventListener("load", () => draw());
      return el;
    }),
  );

  let fps = 10;
  let timeline: SpriteSegment[] = [];
  let total = 0;
  let time = 0;
  let playing = info.frame_count > 1 || timelineHasMany();
  let raf = 0;
  let last = 0;
  let disposed = false;
  bgSel.value = defaultBackground(info.tex_format);

  function timelineHasMany(): boolean {
    return info.frames.some((f) => f.images.length > 1);
  }

  function rebuild() {
    timeline = buildTimeline(info.frames, fps);
    total = timelineDuration(timeline);
    scrub.max = String(Math.max(0, timeline.length - 1));
    if (!timeline.length) playing = false;
  }

  function currentIndex(): number {
    return Math.max(0, segmentAt(timeline, time, true));
  }

  function additive(): boolean {
    const b = blendSel.value as Blend;
    return b === "additive" || (b === "auto" && info.additive);
  }

  function draw() {
    if (disposed) return;
    const zoom = Number(zoomSel.value) || 1;
    const w = Math.max(64, info.max_width + 16);
    const h = Math.max(64, info.max_height + 16);
    if (canvas.width !== w * zoom || canvas.height !== h * zoom) {
      canvas.width = w * zoom;
      canvas.height = h * zoom;
    }
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.imageSmoothingEnabled = false;
    drawBackground(ctx, canvas.width, canvas.height, bgSel.value as Background);
    const seg = timeline[currentIndex()];
    if (!seg) return;
    const meta = info.frames[seg.frame]?.images[seg.image];
    const img = imgs[seg.frame]?.[seg.image];
    const cx = Math.floor(w / 2) * zoom;
    const cy = Math.floor(h / 2) * zoom;
    if (meta && img?.complete && img.naturalWidth) {
      ctx.globalCompositeOperation = additive() ? "lighter" : "source-over";
      // origin_x/y: deslocamento do canto superior esquerdo em relação à origem (y para cima)
      ctx.drawImage(img, cx + meta.origin_x * zoom, cy - meta.origin_y * zoom, meta.width * zoom, meta.height * zoom);
      ctx.globalCompositeOperation = "source-over";
    }
    if (originChk.checked) {
      ctx.strokeStyle = "rgba(255, 80, 80, 0.9)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(cx - 6 * zoom, cy + 0.5);
      ctx.lineTo(cx + 6 * zoom, cy + 0.5);
      ctx.moveTo(cx + 0.5, cy - 6 * zoom);
      ctx.lineTo(cx + 0.5, cy + 6 * zoom);
      ctx.stroke();
    }
    scrub.value = String(seg === undefined ? 0 : currentIndex());
    const frameNo = seg.frame + 1;
    const inGroup = info.frames[seg.frame]?.kind === "group";
    scrubLabel.textContent = inGroup
      ? `${frameNo}/${info.frames.length} · ${seg.image + 1}/${info.frames[seg.frame]!.images.length}`
      : `${frameNo}/${info.frames.length}`;
    for (const [i, b] of [...strip.children].entries()) b.classList.toggle("active", i === currentIndex());
  }

  function tick(now: number) {
    raf = 0;
    if (disposed) return;
    if (playing && total > 0) {
      time += last ? (now - last) / 1000 : 0;
      last = now;
      draw();
      raf = requestAnimationFrame(tick);
    }
  }

  function setPlaying(value: boolean) {
    playing = value && timeline.length > 0;
    playBtn.textContent = playing ? t("exp.spr.pause") : t("exp.spr.play");
    playBtn.setAttribute("aria-label", playBtn.textContent);
    last = 0;
    if (playing && !raf) raf = requestAnimationFrame(tick);
  }

  // faixa de miniaturas (limitada: sprites de 1000 frames existem)
  function buildStrip() {
    strip.replaceChildren();
    const shown = timeline.slice(0, 96);
    for (const [i, seg] of shown.entries()) {
      const meta = info.frames[seg.frame]?.images[seg.image];
      if (!meta) continue;
      const b = document.createElement("button");
      b.type = "button";
      b.className = "spr-thumb";
      b.title = `${seg.frame + 1}${info.frames[seg.frame]!.kind === "group" ? `.${seg.image + 1}` : ""} — ${meta.width}×${meta.height}`;
      b.innerHTML = `<img alt="" src="${meta.png}" />`;
      b.addEventListener("click", () => {
        setPlaying(false);
        time = timeline[i]!.start + 1e-6;
        draw();
      });
      strip.append(b);
    }
    if (timeline.length > shown.length) {
      const more = document.createElement("span");
      more.className = "dim";
      more.textContent = t("exp.spr.more", timeline.length - shown.length);
      strip.append(more);
    }
  }

  playBtn.addEventListener("click", () => setPlaying(!playing));
  scrub.addEventListener("input", () => {
    setPlaying(false);
    const seg = timeline[Number(scrub.value)];
    if (seg) time = seg.start + 1e-6;
    draw();
  });
  fpsInput.addEventListener("input", () => {
    fps = Number(fpsInput.value) || 10;
    fpsLabel.textContent = String(fps);
    const idx = currentIndex();
    const before = timeline[idx];
    rebuild();
    // mantém o mesmo frame na tela ao mudar a velocidade
    const same = timeline[idx];
    if (before && same) time = same.start + 1e-6;
    buildStrip();
    draw();
  });
  for (const c of [zoomSel, bgSel, blendSel, originChk]) c.addEventListener("change", draw);

  rebuild();
  buildStrip();
  setPlaying(playing);
  draw();

  return () => {
    disposed = true;
    if (raf) cancelAnimationFrame(raf);
  };
}
