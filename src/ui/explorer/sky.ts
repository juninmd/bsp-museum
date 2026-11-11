// Bloco "Céu" do visualizador de imagens: ao abrir um lado de `gfx/env` (…up/dn/lf/rt/ft/bk), carrega os
// seis irmãos, mostra o cubo CSS 3D girando e a planificação, e diz quantos lados existem.

import { invoke } from "@tauri-apps/api/core";
import { t } from "../../i18n.ts";
import { foundCount, skyExtCandidates, skyFacePath, SKY_FACES, type SkyFace, type SkySet } from "../../lib/sky.ts";
import type { ImageInfo } from "../../types/formats.ts";
import { esc } from "./common.ts";

type Faces = Record<SkyFace, string | null>;

/** Lê um lado: tenta a extensão do arquivo e a outra; `null` se nenhuma existe/decodifica. */
async function loadFace(set: SkySet, face: SkyFace): Promise<string | null> {
  for (const ext of skyExtCandidates(set.ext)) {
    try {
      const info = await invoke<ImageInfo>("load_image", { path: skyFacePath(set, face, ext) });
      return info.png;
    } catch {
      // próximo candidato
    }
  }
  return null;
}

function faceHtml(face: SkyFace, png: string | null, cls: string): string {
  const label = esc(t(`exp.sky.face.${face}`));
  return png
    ? `<div class="${cls} sky-face sky-${face}" title="${label}"><img src="${png}" alt="${label}" draggable="false" /><i>${esc(face)}</i></div>`
    : `<div class="${cls} sky-face sky-${face} is-missing" title="${label}"><span>${esc(t("exp.sky.missing"))}</span><i>${esc(face)}</i></div>`;
}

/** Monta o bloco dentro de `host`; devolve o desmonte (cancela o que ainda estiver carregando). */
export function mountSkyBlock(host: HTMLElement, set: SkySet): () => void {
  let live = true;
  host.innerHTML = `<section class="sky-block" aria-label="${esc(t("exp.sky.title"))}">
    <header class="sky-head"><h4>${esc(t("exp.sky.title"))} <code>${esc(set.prefix)}</code></h4><span class="sky-count dim">${esc(t("exp.sky.loading"))}</span></header>
  </section>`;
  const block = host.querySelector<HTMLElement>(".sky-block")!;

  void Promise.all(SKY_FACES.map((f) => loadFace(set, f))).then((list) => {
    if (!live) return;
    const faces = Object.fromEntries(SKY_FACES.map((f, i) => [f, list[i] ?? null])) as Faces;
    const found = foundCount(Object.fromEntries(SKY_FACES.map((f) => [f, faces[f] !== null])) as Record<SkyFace, boolean>);
    const ok = found === 6;
    block.innerHTML = `
      <header class="sky-head">
        <h4>${esc(t("exp.sky.title"))} <code>${esc(set.prefix)}</code></h4>
        <span class="sky-count ${ok ? "ok" : "warn"}">${esc(t("exp.sky.found", found))}</span>
      </header>
      <div class="sky-body">
        <div class="sky-scene" role="img" aria-label="${esc(t("exp.sky.cube"))}">
          <div class="sky-cube">${SKY_FACES.map((f) => faceHtml(f, faces[f], "sky-cface")).join("")}</div>
        </div>
        <div class="sky-net" role="group" aria-label="${esc(t("exp.sky.net"))}">
          <span></span>${faceHtml("up", faces.up, "sky-nface")}<span></span><span></span>
          ${faceHtml("lf", faces.lf, "sky-nface")}${faceHtml("ft", faces.ft, "sky-nface")}${faceHtml("rt", faces.rt, "sky-nface")}${faceHtml("bk", faces.bk, "sky-nface")}
          <span></span>${faceHtml("dn", faces.dn, "sky-nface")}<span></span><span></span>
        </div>
      </div>
      <p class="sky-note ${ok ? "ok" : "warn"}">${esc(ok ? t("exp.sky.complete", set.prefix) : t("exp.sky.incomplete"))}</p>`;
  });
  return () => {
    live = false;
  };
}
