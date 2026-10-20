// Player de áudio do Explorador: `<audio>` + forma de onda em canvas (envelope do backend),
// cursor de reprodução, marcadores de loop/cue e metadados.

import { invoke } from "@tauri-apps/api/core";
import { t } from "../../i18n.ts";
import {
  autoGain,
  formatDuration,
  loopSpans,
  peakDb,
  secondsToX,
  waveformPolyline,
  xToSeconds,
} from "../../lib/explorer.ts";
import type { AudioData, AudioInfo, ModFileAnalysis, ModFileEntry } from "../../types/modfiles.ts";
import { emptyHtml, errorHtml, esc, factsHtml, loadingHtml, sizeText, warningsHtml } from "./common.ts";
import { errMsg, fileArgs } from "./state.ts";

const HEIGHT = 140;

function cssVar(name: string, fallback: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

function metaHtml(a: AudioInfo): string {
  const db = peakDb(a.peak);
  const dur = `${formatDuration(a.duration_secs)}${a.duration_approx ? " ≈" : ""}`;
  const rows: [string, string][] = [
    [t("exp.aud.container"), `${esc(a.container.toUpperCase())} · ${esc(a.codec)}`],
    [t("exp.aud.channels"), a.channels === 1 ? t("exp.aud.mono") : a.channels === 2 ? t("exp.aud.stereo") : String(a.channels)],
    [t("exp.aud.rate"), `${a.sample_rate.toLocaleString(t("locale"))} Hz`],
    [t("exp.aud.bits"), a.bits_per_sample ? `${a.bits_per_sample} bit` : ""],
    [t("exp.aud.bitrate"), a.bitrate_kbps ? `${a.bitrate_kbps} kbps` : ""],
    [t("exp.aud.duration"), dur],
    [t("exp.aud.frames"), a.sample_frames != null ? a.sample_frames.toLocaleString(t("locale")) : ""],
    [t("exp.aud.peak"), a.peak != null ? `${(a.peak * 100).toFixed(1)}%${db != null ? ` (${db.toFixed(1)} dBFS)` : ""}` : ""],
    [t("exp.aud.size"), sizeText(a.size_bytes)],
  ];
  const tags = a.tags.length
    ? `<h4>${esc(t("exp.aud.tags"))}</h4>${factsHtml(a.tags.map((x) => [x.key, esc(x.value)] as [string, string]))}`
    : "";
  const spans = loopSpans(a.loops, a.sample_rate, a.duration_secs);
  const loops = spans.length
    ? `<h4>${esc(t("exp.aud.loops"))}</h4><ul class="exp-list-plain">${spans
        .map((s) => `<li><span class="dot loop"></span>${esc(formatDuration(s.start))} → ${esc(formatDuration(s.end))} <small class="dim">(${esc(s.source)})</small></li>`)
        .join("")}</ul>`
    : "";
  const cues = a.cue_points.length
    ? `<h4>${esc(t("exp.aud.cues"))}</h4><ul class="exp-list-plain">${a.cue_points
        .map((c) => `<li><span class="dot cue"></span>#${c.id} · ${esc(formatDuration(c.sample_offset / (a.sample_rate || 1)))}</li>`)
        .join("")}</ul>`
    : "";
  return `${factsHtml(rows)}${loops}${cues}${tags}${warningsHtml(a.warnings)}`;
}

/**
 * Carrega análise e áudio do arquivo e monta o player em `host`.
 * Devolve a função que desmonta (para o áudio e o laço de animação).
 */
export function mountAudioPlayer(host: HTMLElement, entry: ModFileEntry): () => void {
  let disposed = false;
  let raf = 0;
  let audio: HTMLAudioElement | null = null;
  const cleanups: (() => void)[] = [];

  host.innerHTML = loadingHtml(t("exp.aud.loading"));
  const args = fileArgs(entry);

  void Promise.all([
    invoke<ModFileAnalysis>("analyze_mod_file", args),
    // o áudio pode falhar (arquivo grande demais, codec) sem invalidar a análise
    invoke<AudioData>("read_mod_audio", args).then(
      (v) => ({ ok: true as const, v }),
      (e: unknown) => ({ ok: false as const, e: errMsg(e) }),
    ),
  ]).then(
    ([analysis, data]) => {
      if (disposed) return;
      if (analysis.analysis.type !== "audio") {
        host.innerHTML = emptyHtml(t("exp.aud.unsupported"));
        return;
      }
      build(analysis.analysis, data);
    },
    (e: unknown) => {
      if (!disposed) host.innerHTML = errorHtml(errMsg(e));
    },
  );

  function build(a: AudioInfo, data: { ok: true; v: AudioData } | { ok: false; e: string }) {
    const duration = a.duration_secs;
    const spans = loopSpans(a.loops, a.sample_rate, duration);
    host.innerHTML = `
      <div class="aud-player">
        <h3 class="exp-title">${esc(entry.name)}</h3>
        <div class="aud-wave-wrap"><canvas class="aud-wave" height="${HEIGHT}" aria-label="${esc(t("exp.aud.waveAria"))}"></canvas></div>
        <div class="aud-legend">
          <span><span class="dot loop"></span>${esc(t("exp.aud.legend.loop"))}</span>
          <span><span class="dot cue"></span>${esc(t("exp.aud.legend.cue"))}</span>
          <span><span class="dot head"></span>${esc(t("exp.aud.legend.head"))}</span>
          ${spans.length ? `<label class="chk"><input type="checkbox" class="aud-honor" /> ${esc(t("exp.aud.honorLoop"))}</label>` : ""}
          <label class="chk"><input type="checkbox" class="aud-boost" checked /> ${esc(t("exp.aud.boost"))}</label>
        </div>
        ${
          data.ok && a.playable
            ? `<audio class="aud-el" controls preload="auto" src="${data.v.data_url}"></audio>`
            : `<div class="exp-notice" data-tone="warn">${esc(data.ok ? t("exp.aud.notPlayable", a.codec) : `${t("exp.aud.readFail")}: ${data.e}`)}</div>`
        }
        <div class="aud-time"><output class="aud-pos">0:00.00</output> / <span>${esc(formatDuration(duration))}</span></div>
        <div class="aud-meta">${metaHtml(a)}</div>
      </div>`;

    const canvas = host.querySelector<HTMLCanvasElement>(".aud-wave")!;
    audio = host.querySelector<HTMLAudioElement>(".aud-el");
    const pos = host.querySelector<HTMLOutputElement>(".aud-pos")!;
    const honor = host.querySelector<HTMLInputElement>(".aud-honor");
    const boost = host.querySelector<HTMLInputElement>(".aud-boost")!;
    let cssWidth = 0;

    function draw() {
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(200, Math.floor(canvas.parentElement!.clientWidth));
      cssWidth = w;
      if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(HEIGHT * dpr)) {
        canvas.width = Math.round(w * dpr);
        canvas.height = Math.round(HEIGHT * dpr);
        canvas.style.width = `${w}px`;
        canvas.style.height = `${HEIGHT}px`;
      }
      const ctx = canvas.getContext("2d")!;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, HEIGHT);
      ctx.fillStyle = cssVar("--well", "#0a0e14");
      ctx.fillRect(0, 0, w, HEIGHT);

      // regiões de loop (atrás da onda)
      for (const s of spans) {
        const x0 = secondsToX(s.start, duration, w);
        const x1 = secondsToX(s.end, duration, w);
        ctx.fillStyle = "rgba(242, 193, 78, 0.14)";
        ctx.fillRect(x0, 0, Math.max(1, x1 - x0), HEIGHT);
      }

      ctx.strokeStyle = cssVar("--line", "#232c38");
      ctx.beginPath();
      ctx.moveTo(0, HEIGHT / 2 + 0.5);
      ctx.lineTo(w, HEIGHT / 2 + 0.5);
      ctx.stroke();

      if (a.envelope && a.envelope.length) {
        const poly = waveformPolyline(a.envelope, w, HEIGHT, boost.checked ? autoGain(a.peak) : 1);
        ctx.beginPath();
        poly.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
        ctx.closePath();
        ctx.fillStyle = cssVar("--accent", "#1f7a8c");
        ctx.fill();
      } else {
        ctx.fillStyle = cssVar("--dim", "#8b98a5");
        ctx.font = "12px ui-sans-serif, system-ui, sans-serif";
        ctx.textAlign = "center";
        ctx.fillText(t("exp.aud.noWave"), w / 2, HEIGHT / 2 - 8);
      }

      for (const s of spans) {
        ctx.strokeStyle = cssVar("--warm", "#f2c14e");
        ctx.lineWidth = 2;
        for (const sec of [s.start, ...(s.end < duration ? [s.end] : [])]) {
          const x = Math.round(secondsToX(sec, duration, w)) + 0.5;
          ctx.beginPath();
          ctx.moveTo(x, 0);
          ctx.lineTo(x, HEIGHT);
          ctx.stroke();
        }
      }
      ctx.lineWidth = 1;
      ctx.strokeStyle = cssVar("--t", "#f0883e");
      for (const c of a.cue_points) {
        const x = Math.round(secondsToX(c.sample_offset / (a.sample_rate || 1), duration, w)) + 0.5;
        ctx.beginPath();
        ctx.setLineDash([3, 3]);
        ctx.moveTo(x, 0);
        ctx.lineTo(x, HEIGHT);
        ctx.stroke();
        ctx.setLineDash([]);
      }

      // cursor de reprodução
      const now = audio?.currentTime ?? 0;
      const x = Math.round(secondsToX(now, duration, w)) + 0.5;
      ctx.strokeStyle = cssVar("--text", "#e6edf3");
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, HEIGHT);
      ctx.stroke();
      pos.textContent = formatDuration(now);
    }

    function loop() {
      raf = 0;
      if (disposed || !audio) return;
      // opção: respeitar o loop do arquivo (quando a reprodução passa do fim do trecho, volta ao início dele)
      if (honor?.checked && !audio.paused) {
        const s = spans[0];
        if (s && audio.currentTime >= s.end - 0.005) audio.currentTime = s.start;
      }
      draw();
      if (!audio.paused && !audio.ended) raf = requestAnimationFrame(loop);
    }
    const kick = () => {
      if (!raf && !disposed) raf = requestAnimationFrame(loop);
    };

    if (audio) {
      for (const ev of ["play", "playing", "seeking", "seeked", "timeupdate", "pause", "ended"] as const) {
        audio.addEventListener(ev, kick);
      }
    }
    boost.addEventListener("change", draw);

    // clicar/arrastar na onda posiciona o cursor
    let dragging = false;
    const seekTo = (e: PointerEvent) => {
      if (!audio) return;
      const rect = canvas.getBoundingClientRect();
      audio.currentTime = xToSeconds(e.clientX - rect.left, duration || audio.duration || 0, cssWidth || rect.width);
      draw();
    };
    canvas.addEventListener("pointerdown", (e) => {
      dragging = true;
      canvas.setPointerCapture(e.pointerId);
      seekTo(e);
    });
    canvas.addEventListener("pointermove", (e) => {
      if (dragging) seekTo(e);
    });
    canvas.addEventListener("pointerup", () => {
      dragging = false;
    });

    const ro = new ResizeObserver(draw);
    ro.observe(canvas.parentElement!);
    // trocar o tema muda as cores lidas das variáveis CSS: redesenha
    const themeWatch = new MutationObserver(draw);
    themeWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    cleanups.push(() => {
      ro.disconnect();
      themeWatch.disconnect();
    });
    draw();
  }

  return () => {
    disposed = true;
    if (raf) cancelAnimationFrame(raf);
    if (audio) {
      audio.pause();
      audio.removeAttribute("src");
      audio.load();
    }
    for (const c of cleanups) c();
  };
}
