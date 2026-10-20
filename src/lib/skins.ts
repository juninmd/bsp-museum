// Partes puras da visualização de skins do `.mdl` (sem DOM nem Three.js): família de skin ->
// textura de cada triângulo, uso das texturas, projeção UV <-> pixel (com zoom e pan), teste de
// acerto de triângulo no mapa UV, rótulos das flags de material e as cores de time de prévia.
// Testadas em `skins.test.ts`.
import type { TextureInfo } from "../types/mdl-extra.ts";

// ------------------------------------------------------------------ família de skin -> textura

/** Linha identidade (sem famílias de skin): cada textura é ela mesma. */
export function identityMap(textureCount: number): number[] {
  return Array.from({ length: Math.max(0, textureCount) }, (_, i) => i);
}

/**
 * Tabela de uma família de skin. Índice fora da faixa vale 0, como no SDK
 * (`skin` fora de `1..numskinfamilies-1` é 0); sem tabela nenhuma vale a identidade.
 */
export function familyMap(families: readonly (readonly number[])[], index: number, textureCount: number): number[] {
  const row = families[index >= 0 && index < families.length ? index : 0];
  return row ? [...row] : identityMap(textureCount);
}

/**
 * Textura de cada triângulo na família `map`: o `texindex` do backend (já na família 0) passa
 * pela tabela. É o mesmo resultado de `load_model(.., skin = i)`, sem pedir o modelo inteiro de novo.
 */
export function skinTexindex(texindex: ArrayLike<number>, map: readonly number[]): Uint32Array {
  const out = new Uint32Array(texindex.length);
  for (let i = 0; i < texindex.length; i++) out[i] = map[texindex[i]!] ?? texindex[i]!;
  return out;
}

/** Textura efetiva de um triângulo na família `map`. */
export function triTexture(texindex: ArrayLike<number>, map: readonly number[], tri: number): number {
  const base = texindex[tri];
  return base === undefined ? -1 : (map[base] ?? base);
}

/** Triângulos de cada textura (ids em ordem crescente) na família `map`. */
export function trianglesByTexture(texindex: ArrayLike<number>, map: readonly number[]): Map<number, number[]> {
  const groups = new Map<number, number[]>();
  for (let tri = 0; tri < texindex.length; tri++) {
    const id = triTexture(texindex, map, tri);
    let list = groups.get(id);
    if (!list) groups.set(id, (list = []));
    list.push(tri);
  }
  return new Map([...groups].sort((a, b) => a[0] - b[0]));
}

/** Famílias com a mesma tabela de outra anterior não precisam de miniatura própria. */
export function distinctFamilies(families: readonly (readonly number[])[]): number[] {
  const seen = new Set<string>();
  const keep: number[] = [];
  families.forEach((row, i) => {
    const key = row.join(",");
    if (seen.has(key)) return;
    seen.add(key);
    keep.push(i);
  });
  return keep;
}

/** Índices das texturas que a família `index` troca em relação à família 0. */
export function changedTextures(families: readonly (readonly number[])[], index: number): number[] {
  const base = families[0] ?? [];
  const row = families[index] ?? [];
  const out: number[] = [];
  for (let i = 0; i < Math.max(base.length, row.length); i++) if ((row[i] ?? i) !== (base[i] ?? i)) out.push(i);
  return out;
}

// ------------------------------------------------------------------ flags de material

/** Chaves (i18n `skins.flag.<chave>`) das flags ligadas de uma textura, em ordem fixa. */
export function flagKeys(info: TextureInfo | undefined): string[] {
  if (!info) return [];
  const keys: string[] = [];
  if (info.chrome) keys.push("chrome");
  if (info.additive) keys.push("additive");
  if (info.masked) keys.push("masked");
  if (info.fullbright) keys.push("fullbright");
  if (info.flatshade) keys.push("flatshade");
  if (info.nomips) keys.push("nomips");
  if (info.alpha) keys.push("alpha");
  if (info.remap) keys.push("remap");
  return keys;
}

// ------------------------------------------------------------------ cores de time (prévia)

export interface TeamPreset {
  id: string;
  /** chave i18n do nome */
  label: string;
  top: number;
  bottom: number;
}

/**
 * Cores de prévia das duas equipes: matiz 0..255 do remap (0 = vermelho, ~170 = azul). São valores de
 * conveniência (azul 140 / vermelho 0, o par clássico do deathmatch do Half-Life), não uma regra do
 * Counter-Strike: os modelos de jogador do CS trocam de modelo por time em vez de remapear a cor.
 */
export const TEAM_PRESETS: readonly TeamPreset[] = [
  { id: "blue", label: "skins.team.blue", top: 140, bottom: 140 },
  { id: "red", label: "skins.team.red", top: 0, bottom: 0 },
];

/** Chave de cache de um `load_model` (corpo + cores de time); `null` = cor do modelo. */
export function modelKey(body: number, top: number | null, bottom: number | null): string {
  return `${body}|${top ?? "-"}|${bottom ?? "-"}`;
}

// ------------------------------------------------------------------ mapa UV: projeção, zoom e pan

/**
 * Janela sobre a textura: `scale` = pixels de tela por texel; (`ox`, `oy`) = posição na tela
 * do canto superior esquerdo da textura. UV do GoldSrc tem v pra baixo (linha 0 da imagem em v = 0).
 */
export interface UvView {
  ox: number;
  oy: number;
  scale: number;
}

export const UV_MIN_SCALE = 0.05;
export const UV_MAX_SCALE = 256;

/** UV (0..1 = a textura inteira) -> pixel da tela. */
export function uvToPixel(u: number, v: number, texW: number, texH: number, view: UvView): [number, number] {
  return [view.ox + u * texW * view.scale, view.oy + v * texH * view.scale];
}

/** Pixel da tela -> UV (inverso de `uvToPixel`). */
export function pixelToUv(px: number, py: number, texW: number, texH: number, view: UvView): [number, number] {
  const w = Math.max(1, texW) * view.scale;
  const h = Math.max(1, texH) * view.scale;
  return [(px - view.ox) / w, (py - view.oy) / h];
}

/** Enquadra a textura inteira, centrada, com uma margem em pixels. */
export function fitView(texW: number, texH: number, viewW: number, viewH: number, margin = 12): UvView {
  const w = Math.max(1, texW);
  const h = Math.max(1, texH);
  const scale = clampScale(Math.min(Math.max(1, viewW - margin * 2) / w, Math.max(1, viewH - margin * 2) / h));
  return { scale, ox: (viewW - w * scale) / 2, oy: (viewH - h * scale) / 2 };
}

export function clampScale(scale: number): number {
  return Math.min(UV_MAX_SCALE, Math.max(UV_MIN_SCALE, scale));
}

/** Zoom por `factor` mantendo o ponto (`px`, `py`) da tela parado. */
export function zoomAt(view: UvView, factor: number, px: number, py: number): UvView {
  const scale = clampScale(view.scale * factor);
  const k = scale / view.scale;
  return { scale, ox: px - (px - view.ox) * k, oy: py - (py - view.oy) * k };
}

export function panBy(view: UvView, dx: number, dy: number): UvView {
  return { ...view, ox: view.ox + dx, oy: view.oy + dy };
}

// ------------------------------------------------------------------ mapa UV: teste de acerto

/** Coordenadas baricêntricas de (`u`, `v`) no triângulo (a, b, c); `null` se for degenerado. */
export function barycentric(
  u: number,
  v: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
  cx: number,
  cy: number,
): [number, number, number] | null {
  const d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
  if (Math.abs(d) < 1e-12) return null;
  const l1 = ((by - cy) * (u - cx) + (cx - bx) * (v - cy)) / d;
  const l2 = ((cy - ay) * (u - cx) + (ax - cx) * (v - cy)) / d;
  return [l1, l2, 1 - l1 - l2];
}

/** O ponto UV está dentro do triângulo `tri` de `uvs` (6 floats por triângulo)? `eps` dá folga nas bordas. */
export function uvInTriangle(uvs: ArrayLike<number>, tri: number, u: number, v: number, eps = 0): boolean {
  const o = tri * 6;
  if (o + 5 >= uvs.length) return false;
  const l = barycentric(u, v, uvs[o]!, uvs[o + 1]!, uvs[o + 2]!, uvs[o + 3]!, uvs[o + 4]!, uvs[o + 5]!);
  return !!l && l[0] >= -eps && l[1] >= -eps && l[2] >= -eps;
}

/**
 * Triângulo (dentre `candidates`) sob o ponto UV, ou -1. Com sobreposição vence o último da lista
 * (o desenhado por cima no mapa). `eps` em unidades de UV.
 */
export function hitTestUv(uvs: ArrayLike<number>, candidates: readonly number[], u: number, v: number, eps = 0): number {
  for (let i = candidates.length - 1; i >= 0; i--) {
    const tri = candidates[i]!;
    if (uvInTriangle(uvs, tri, u, v, eps)) return tri;
  }
  return -1;
}

/** Tolerância de acerto em UV equivalente a `px` pixels de tela. */
export function uvTolerance(px: number, texW: number, texH: number, view: UvView): number {
  return px / (Math.max(1, Math.min(texW, texH)) * view.scale);
}

/** Todos os triângulos (dentre `candidates`) sob o ponto UV: vértices compartilhados acertam mais de um. */
export function hitTestUvAll(uvs: ArrayLike<number>, candidates: readonly number[], u: number, v: number, eps = 0): number[] {
  return candidates.filter((tri) => uvInTriangle(uvs, tri, u, v, eps));
}

// ------------------------------------------------------------------ canal alpha

/** Troca o RGB pelo canal alpha em tons de cinza (`ImageData.data`, RGBA); o alpha fica opaco. */
export function alphaToGray(data: Uint8ClampedArray): void {
  for (let i = 0; i < data.length; i += 4) {
    const a = data[i + 3]!;
    data[i] = a;
    data[i + 1] = a;
    data[i + 2] = a;
    data[i + 3] = 255;
  }
}

/** A textura tem algum pixel com alpha < 255? (decide se vale mostrar o botão de alpha) */
export function hasTransparency(data: Uint8ClampedArray): boolean {
  for (let i = 3; i < data.length; i += 4) if (data[i]! < 255) return true;
  return false;
}

// ------------------------------------------------------------------ exportação

/** Nome de arquivo seguro para a textura exportada (`<nome>.png`; o `.bmp` do nome sai). */
export function textureFileName(name: string, fallback = "textura"): string {
  const base = name
    .replace(/\.(bmp|png|tga)$/i, "")
    .replace(/[^\p{L}\p{N}._-]+/gu, "_")
    .replace(/^[._]+|[._]+$/g, "");
  return `${base || fallback}.png`;
}
