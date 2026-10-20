// Partes puras do visualizador de `.mdl` (sem DOM nem Three.js): conversões dos
// sliders em bytes do backend, chave de cache das sequências, plano de material,
// cores e interpolação das caixas de acerto. Testadas em `mdl-controls.test.ts`.
import type { ControllerInfo, SeqDetail, TextureInfo } from "../types/mdl-extra.ts";

// ------------------------------------------------------------------ bone controllers

/** Slots de entrada de `load_sequence(.., controllers)`: 0..3 = olhar/mira, 4 = boca. */
export const CONTROLLER_SLOTS = 5;

/** `ControllerInfo::value_for` do backend (`StudioCalcBoneAdj` do SDK): graus/unidades do byte. */
export function controllerValue(c: ControllerInfo, byte: number): number {
  const b = clampByte(byte);
  if (isMouth(c)) {
    const t = Math.min(b / 64, 1);
    return (1 - t) * c.start + t * c.end;
  }
  if (c.wrap) return b * (360 / 256) + c.start;
  const t = Math.min(Math.max(b / 255, 0), 1);
  return (1 - t) * c.start + t * c.end;
}

/** `ControllerInfo::byte_for`: byte que produz `value` graus/unidades (inverso de `controllerValue`). */
export function controllerByte(c: ControllerInfo, value: number): number {
  if (c.wrap && !isMouth(c)) {
    const b = Math.round((value - c.start) * (256 / 360));
    return ((b % 256) + 256) % 256;
  }
  const span = isMouth(c) ? 64 : 255;
  const d = c.end - c.start;
  if (Math.abs(d) < 1e-7) return 0;
  const raw = (span * (value - c.start)) / d;
  return Math.trunc(Math.min(Math.max(raw, 0), c.input_max));
}

/** Qualquer `index` > 3 é a boca, como no SDK. */
export function isMouth(c: ControllerInfo): boolean {
  return c.index > 3;
}

/** O slider do controlador anda em bytes (0..`input_max`); o rótulo mostra o valor real. */
export function controllerSliderMax(c: ControllerInfo): number {
  return c.wrap && !isMouth(c) ? 255 : Math.max(1, c.input_max);
}

/** Valor do slider (byte) -> byte enviado: limita à faixa que ainda muda algo. */
export function sliderToControllerByte(c: ControllerInfo, slider: number): number {
  return Math.min(Math.max(0, Math.round(slider)), controllerSliderMax(c));
}

/** "12,5°" / "4,2" — número com 1 casa e unidade. */
export function formatControllerValue(c: ControllerInfo, byte: number): string {
  const v = controllerValue(c, byte);
  const n = Math.abs(v) < 0.05 ? "0" : v.toFixed(1).replace(/\.0$/, "");
  return c.unit === "deg" ? `${n}°` : n;
}

/** Bytes de repouso por slot (slot sem controlador = 0). */
export function restControllerBytes(infos: readonly ControllerInfo[]): number[] {
  const bytes = new Array<number>(CONTROLLER_SLOTS).fill(0);
  for (const c of infos) if (c.index >= 0 && c.index < CONTROLLER_SLOTS) bytes[c.index] = clampByte(c.rest);
  return bytes;
}

/**
 * Argumento `controllers` do `load_sequence`: `undefined` enquanto todos estão no repouso
 * (mantém o comportamento antigo e a mesma chave de cache), senão os 5 bytes.
 */
export function controllersArg(
  infos: readonly ControllerInfo[],
  bytes: readonly number[],
): [number, number, number, number, number] | undefined {
  const rest = restControllerBytes(infos);
  let changed = false;
  const out = rest.map((r, i) => {
    const hasController = infos.some((c) => c.index === i);
    const b = hasController ? clampByte(bytes[i] ?? r) : 0;
    if (b !== r) changed = true;
    return b;
  });
  return changed ? (out as [number, number, number, number, number]) : undefined;
}

// ------------------------------------------------------------------ blends

export interface BlendAxis {
  /** 0 ou 1: qual blender (`blend[which]` do `load_sequence`) */
  which: 0 | 1;
  /** "x" | "y" | "z" | "xr" | "yr" | "zr" | "" quando o arquivo não diz o eixo */
  axis: string;
  /** faixa em graus/unidades que o byte 0..255 cobre */
  start: number;
  end: number;
}

/**
 * Eixos de blend de uma sequência: 1 ou 2 (o 9-blend do jogador usa 2: pitch e yaw).
 * Com mais de um blend mas sem eixo declarado, devolve um eixo genérico (byte cru).
 */
export function blendAxes(blends: number, d: SeqDetail | undefined): BlendAxis[] {
  if (blends <= 1) return [];
  const axes: BlendAxis[] = [];
  if (d) {
    for (const w of [0, 1] as const) {
      if (d.blend_type[w] !== 0) axes.push({ which: w, axis: d.blend_axis[w] ?? "", start: d.blend_start[w], end: d.blend_end[w] });
    }
  }
  if (axes.length === 0) axes.push({ which: 0, axis: "", start: 0, end: 255 });
  return axes;
}

/** Valor em graus/unidades que o byte do blender representa (aproximação linear; a UI só rotula). */
export function blendValue(a: BlendAxis, byte: number): number {
  return a.start + (clampByte(byte) / 255) * (a.end - a.start);
}

/** Byte inicial de um blender: o ponto "zero" se a faixa o cruza (mira reta), senão o início. */
export function defaultBlendByte(d: SeqDetail | undefined, a: BlendAxis): number {
  if (!d || d.blend_type[a.which] === 0) return 0;
  const lo = Math.min(a.start, a.end);
  const hi = Math.max(a.start, a.end);
  if (lo < 0 && hi > 0) return Math.trunc((255 * (0 - a.start)) / (a.end - a.start));
  return 0;
}

// ------------------------------------------------------------------ cache

/** Chave do cache de quadros: sequência + blenders + controladores + se traz hitboxes. */
export function seqCacheKey(
  index: number,
  blend: readonly [number, number] | undefined,
  controllers: readonly number[] | undefined,
  hitboxes: boolean,
): string {
  const b = blend ?? [0, 0];
  return `${index}|${b[0]},${b[1]}|${controllers ? controllers.join(",") : "-"}|${hitboxes ? "h" : "-"}`;
}

/** Mapa com teto: ao passar de `max` entradas descarta a mais antiga (inserção). */
export class BoundedCache<V> {
  private map = new Map<string, V>();
  constructor(private max: number) {}
  get(key: string): V | undefined {
    return this.map.get(key);
  }
  set(key: string, value: V): void {
    this.map.delete(key);
    this.map.set(key, value);
    while (this.map.size > this.max) {
      const oldest = this.map.keys().next().value;
      if (oldest === undefined) break;
      this.map.delete(oldest);
    }
  }
  get size(): number {
    return this.map.size;
  }
}

// ------------------------------------------------------------------ body / skin

/** Escolha de cada bodypart a partir de `body` (inverso de `bodyValue`). */
export function bodyChoices(parts: readonly { base: number; models: readonly unknown[] }[], body: number): number[] {
  return parts.map((p) => {
    const n = p.models.length;
    if (n === 0) return 0;
    return Math.floor(body / Math.max(1, p.base | 0)) % n;
  });
}

// ------------------------------------------------------------------ material

export type ShadeKind = "matcap" | "basic" | "lit";

export interface MaterialPlan {
  /** chrome = matcap (reflexão); fullbright = sem iluminação; senão iluminado */
  shade: ShadeKind;
  /** normais da face em vez das suavizadas */
  flat: boolean;
  blend: "opaque" | "masked" | "additive";
}

/** Plano de material de uma textura. Sem `TextureInfo` (backend antigo): iluminado e com máscara. */
export function materialPlan(info: TextureInfo | undefined): MaterialPlan {
  if (!info) return { shade: "lit", flat: false, blend: "masked" };
  const shade: ShadeKind = info.chrome ? "matcap" : info.fullbright ? "basic" : "lit";
  const blend = info.additive ? "additive" : info.masked ? "masked" : "opaque";
  return { shade, flat: info.flatshade, blend };
}

// ------------------------------------------------------------------ cores

/** Cor de uma amostra do matiz HL 0..255 (cor de time top/bottom). */
export function hueColor(h: number): string {
  const hue = Math.round((clampByte(h) / 255) * 360);
  return `hsl(${hue} 85% 50%)`;
}

/** Cor (0xRRGGBB) do grupo de uma hitbox; a paleta é fixa para o grupo ser reconhecível. */
const GROUP_COLORS = [0x9aa5b1, 0xff5c5c, 0xffb347, 0xf4e04d, 0x6ee26e, 0x4fd1c5, 0x4c9aff, 0xb084ff];

export function hitGroupColor(group: number): number {
  const g = Math.max(0, Math.trunc(group));
  if (g < GROUP_COLORS.length) return GROUP_COLORS[g]!;
  // além de 7: matiz determinístico pelo número
  const hue = (g * 47) % 360;
  return hslToHex(hue / 360, 0.7, 0.62);
}

function hslToHex(h: number, s: number, l: number): number {
  const f = (n: number) => {
    const k = (n + h * 12) % 12;
    const a = s * Math.min(l, 1 - l);
    return l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
  };
  const c = (x: number) => Math.round(x * 255);
  return (c(f(0)) << 16) | (c(f(8)) << 8) | c(f(4));
}

// ------------------------------------------------------------------ caixas por quadro

/** Floats por hitbox em `MdlSeqFramesEx.hitboxes` (centro 3, meias-arestas 3, quaternion xyzw 4). */
export const BOX_STRIDE = 10;

/**
 * Caixas no quadro fracionário `f` (interpola entre `floor(f)` e o seguinte; quaternion por
 * nlerp pelo lado mais curto). `src` tem `frames * count * stride` floats; `out` recebe `count * stride`.
 */
export function lerpBoxes(
  src: ArrayLike<number>,
  frames: number,
  count: number,
  f: number,
  out: Float32Array,
  stride = BOX_STRIDE,
): void {
  if (frames <= 0 || count <= 0) return;
  const f0 = Math.max(0, Math.min(Math.floor(f), frames - 1));
  const f1 = Math.min(f0 + 1, frames - 1);
  const t = Math.max(0, Math.min(f - f0, 1));
  for (let i = 0; i < count; i++) {
    const a = (f0 * count + i) * stride;
    const c = (f1 * count + i) * stride;
    const o = i * stride;
    for (let k = 0; k < 6 && k < stride; k++) out[o + k] = src[a + k]! + (src[c + k]! - src[a + k]!) * t;
    if (stride >= 10) {
      const dot = src[a + 6]! * src[c + 6]! + src[a + 7]! * src[c + 7]! + src[a + 8]! * src[c + 8]! + src[a + 9]! * src[c + 9]!;
      const sign = dot < 0 ? -1 : 1;
      let x = src[a + 6]! + (sign * src[c + 6]! - src[a + 6]!) * t;
      let y = src[a + 7]! + (sign * src[c + 7]! - src[a + 7]!) * t;
      let z = src[a + 8]! + (sign * src[c + 8]! - src[a + 8]!) * t;
      let w = src[a + 9]! + (sign * src[c + 9]! - src[a + 9]!) * t;
      const len = Math.hypot(x, y, z, w) || 1;
      x /= len;
      y /= len;
      z /= len;
      w /= len;
      out[o + 6] = x;
      out[o + 7] = y;
      out[o + 8] = z;
      out[o + 9] = w;
    }
  }
}

/** Posição (em % da barra, 0..100) de um quadro na barra do tempo; 0 quando a sequência tem 1 quadro. */
export function framePercent(frame: number, frames: number): number {
  if (frames < 2) return 0;
  return Math.max(0, Math.min(100, (frame / (frames - 1)) * 100));
}

// ------------------------------------------------------------------ utilidades

export function clampByte(n: number): number {
  return Math.min(255, Math.max(0, Math.round(Number.isFinite(n) ? n : 0)));
}

/** Debounce simples; `flush`/`cancel` ajudam no descarte e nos testes. */
export function debounce<A extends unknown[]>(fn: (...args: A) => void, ms: number): ((...args: A) => void) & { cancel(): void } {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const wrapped = (...args: A) => {
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      fn(...args);
    }, ms);
  };
  wrapped.cancel = () => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  };
  return wrapped;
}
