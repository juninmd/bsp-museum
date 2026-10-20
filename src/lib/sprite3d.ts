// Matemática pura da visualização 3D de sprites `.spr` (sem Three.js, testável).
//
// Convenção do motor (GoldSrc/Xash3D): Z para cima, X à frente, Y à ESQUERDA (sistema destro).
// Ângulos de entidade são `[pitch, yaw, roll]` em graus, como em `AngleVectors`.
//
// Regras de cada tipo de sprite (R_GetSpriteAxes / R_DrawSpriteModel do Xash3D; o desenho do
// quad em si é `origem + right * x + up * y`, com x de `origin_x` a `origin_x + width` e y de
// `origin_y - height` a `origin_y`, e depois escala). ESCRITO DE MEMÓRIA do código do motor —
// não conferido contra o fonte nesta sessão, então os detalhes finos (limiar de 1°, o
// deslocamento anti z-fighting) podem diferir do jogo real:
//
//   parallel_upright (0)  up = (0,0,1); right = normalizado(vpn.y, -vpn.x, 0). Gira só em Z
//                         acompanhando a DIREÇÃO do olhar. Se o olhar é quase vertical
//                         (|vpn.z| > cos 1° = 0,999848) os eixos não existem: o motor não
//                         desenha o sprite.
//   facing_upright (1)    up = (0,0,1); right = normalizado(org.y - eye.y, -(org.x - eye.x), 0).
//                         Gira só em Z, mas de frente para a POSIÇÃO da câmera (não para a
//                         direção); sem distância horizontal não há eixo.
//   parallel (2)          right = vright, up = vup da câmera (inclui a rolagem da câmera).
//   oriented (3)          AngleVectors(angles): o plano do sprite é (right, up) e fica fixo
//                         no mundo; a origem recua 0,01 * forward para evitar z-fighting.
//   parallel_oriented (4) como `parallel`, com `angles[ROLL]` girando right/up em torno do
//                         olhar: right = vright*cos + vup*sin; up = -vright*sin + vup*cos.

import type { SpriteSegment } from "./explorer.ts";
import type { SpriteTexFormat, SpriteType } from "../types/formats.ts";

export type Vec3 = [number, number, number];

const DEG = Math.PI / 180;

export const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const scale = (a: Vec3, k: number): Vec3 => [a[0] * k, a[1] * k, a[2] * k];
export const dot = (a: Vec3, b: Vec3): number => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
export const length = (a: Vec3): number => Math.hypot(a[0], a[1], a[2]);

/** Normaliza; vetor nulo vira `fallback` (nunca NaN). */
export function normalize(a: Vec3, fallback: Vec3 = [1, 0, 0]): Vec3 {
  const l = length(a);
  return l > 1e-9 ? [a[0] / l, a[1] / l, a[2] / l] : fallback;
}

// ------------------------------------------------------------------ câmera

export interface CameraBasis {
  /** `vpn`: para onde a câmera olha */
  forward: Vec3;
  /** `vright` */
  right: Vec3;
  /** `vup` */
  up: Vec3;
}

/** Gira `right`/`up` em torno do olhar: o mesmo cálculo do `parallel_oriented` (roll em graus). */
export function rollAxes(right: Vec3, up: Vec3, rollDeg: number): { right: Vec3; up: Vec3 } {
  const s = Math.sin(rollDeg * DEG);
  const c = Math.cos(rollDeg * DEG);
  return {
    right: add(scale(right, c), scale(up, s)),
    up: add(scale(right, -s), scale(up, c)),
  };
}

/** Base da câmera olhando de `eye` para `target`, com Z do mundo para cima e rolagem opcional. */
export function cameraBasis(eye: Vec3, target: Vec3, rollDeg = 0): CameraBasis {
  const forward = normalize(sub(target, eye));
  // forward × Z: no motor `right = forward × up` (X à frente, Y à esquerda => right = -Y)
  const right0 = normalize(cross(forward, [0, 0, 1]), [0, -1, 0]);
  const up0 = cross(right0, forward);
  if (!rollDeg) return { forward, right: right0, up: up0 };
  const r = rollAxes(right0, up0, rollDeg);
  return { forward, right: r.right, up: r.up };
}

/** Posição da câmera em órbita (yaw/pitch em graus) ao redor de `target`, a `dist` de distância. */
export function orbitEye(target: Vec3, yawDeg: number, pitchDeg: number, dist: number): Vec3 {
  const cp = Math.cos(pitchDeg * DEG);
  return [
    target[0] + dist * cp * Math.cos(yawDeg * DEG),
    target[1] + dist * cp * Math.sin(yawDeg * DEG),
    target[2] + dist * Math.sin(pitchDeg * DEG),
  ];
}

/** Pitch de órbita limitado (evita virar de cabeça para baixo). */
export function clampPitch(pitchDeg: number, limit = 89.5): number {
  return Math.max(-limit, Math.min(limit, pitchDeg));
}

// ------------------------------------------------------------------ eixos do sprite

/** `AngleVectors` do Quake/GoldSrc: `[pitch, yaw, roll]` em graus. */
export function angleVectors(angles: Vec3): { forward: Vec3; right: Vec3; up: Vec3 } {
  const sp = Math.sin(angles[0] * DEG);
  const cp = Math.cos(angles[0] * DEG);
  const sy = Math.sin(angles[1] * DEG);
  const cy = Math.cos(angles[1] * DEG);
  const sr = Math.sin(angles[2] * DEG);
  const cr = Math.cos(angles[2] * DEG);
  return {
    forward: [cp * cy, cp * sy, -sp],
    right: [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp],
    up: [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
  };
}

/** Limiar do `parallel_upright`: cos(1°). */
export const UPRIGHT_LIMIT = 0.999848;

export interface SpriteAxes {
  right: Vec3;
  up: Vec3;
  /** deslocamento a somar à origem (só o `oriented` usa: -0,01 * forward) */
  offset: Vec3;
  /** false = o motor não desenha (eixos degenerados) */
  valid: boolean;
}

export interface ViewState {
  eye: Vec3;
  basis: CameraBasis;
}

/** Eixos `right`/`up` do quad de um sprite do tipo `type`, para a câmera `view` e a origem dada. */
export function spriteAxes(type: SpriteType, view: ViewState, origin: Vec3, angles: Vec3 = [0, 0, 0]): SpriteAxes {
  const none: Vec3 = [0, 0, 0];
  switch (type) {
    case "parallel_upright": {
      const f = view.basis.forward;
      if (f[2] > UPRIGHT_LIMIT || f[2] < -UPRIGHT_LIMIT) return { right: [0, -1, 0], up: [0, 0, 1], offset: none, valid: false };
      return { right: normalize([f[1], -f[0], 0], [0, -1, 0]), up: [0, 0, 1], offset: none, valid: true };
    }
    case "facing_upright": {
      const dx = origin[0] - view.eye[0];
      const dy = origin[1] - view.eye[1];
      if (Math.hypot(dx, dy) < 1e-6) return { right: [0, -1, 0], up: [0, 0, 1], offset: none, valid: false };
      return { right: normalize([dy, -dx, 0], [0, -1, 0]), up: [0, 0, 1], offset: none, valid: true };
    }
    case "oriented": {
      const v = angleVectors(angles);
      return { right: v.right, up: v.up, offset: scale(v.forward, -0.01), valid: true };
    }
    case "parallel_oriented": {
      const r = rollAxes(view.basis.right, view.basis.up, angles[2]);
      return { right: r.right, up: r.up, offset: none, valid: true };
    }
    case "parallel":
    default:
      return { right: view.basis.right, up: view.basis.up, offset: none, valid: true };
  }
}

/** Retângulo de uma imagem do sprite (campos de `SpriteImage` que o quad usa). */
export interface QuadRect {
  origin_x: number;
  origin_y: number;
  width: number;
  height: number;
}

/**
 * Cantos do quad no mundo, na ordem inferior-esquerdo, inferior-direito, superior-direito,
 * superior-esquerdo (uv 0,0 / 1,0 / 1,1 / 0,1). `origin_x/y` posiciona o canto superior
 * esquerdo em relação à origem (y para cima), como o 2D.
 */
export function quadCorners(axes: SpriteAxes, origin: Vec3, img: QuadRect, k = 1): [Vec3, Vec3, Vec3, Vec3] {
  const o = add(origin, axes.offset);
  const x0 = img.origin_x * k;
  const x1 = (img.origin_x + img.width) * k;
  const y0 = (img.origin_y - img.height) * k;
  const y1 = img.origin_y * k;
  const at = (x: number, y: number): Vec3 => add(o, add(scale(axes.right, x), scale(axes.up, y)));
  return [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)];
}

// ------------------------------------------------------------------ mistura

export type BlendOverride = "auto" | "normal" | "additive";

export interface BlendSpec {
  mode: "opaque" | "alpha" | "alphaTest" | "additive";
  /** blending aditivo (preto soma zero) */
  additive: boolean;
  transparent: boolean;
  depthWrite: boolean;
  depthTest: boolean;
  /** corte de alpha (0 = sem) */
  alphaTest: number;
}

/**
 * Mapa `tex_format` -> estado de mistura. O backend já aplicou a transparência no PNG:
 *   normal      opaco; additive  soma na tela, sem escrever nem testar profundidade (como o
 *   motor com kRenderTransAdd); index_alpha  alpha = índice, sem escrita de profundidade;
 *   alpha_test  índice 255 vaza (corte em 0,5).
 * `override`: "normal" desenha em source-over (no aditivo isso mostra o preto como opaco);
 * "additive" força a soma. `opacity` (0..1, o `renderamt`) abaixo de 1 liga a transparência.
 */
export function blendFor(tex: SpriteTexFormat, override: BlendOverride = "auto", opacity = 1): BlendSpec {
  const eff: SpriteTexFormat | "additive" = override === "additive" ? "additive" : override === "normal" && tex === "additive" ? "normal" : tex;
  const translucent = opacity < 1;
  switch (eff) {
    case "additive":
      return { mode: "additive", additive: true, transparent: true, depthWrite: false, depthTest: false, alphaTest: 0 };
    case "index_alpha":
      return { mode: "alpha", additive: false, transparent: true, depthWrite: false, depthTest: true, alphaTest: 0 };
    case "alpha_test":
      return { mode: "alphaTest", additive: false, transparent: translucent, depthWrite: !translucent, depthTest: true, alphaTest: 0.5 };
    case "normal":
    default:
      return { mode: "opaque", additive: false, transparent: translucent, depthWrite: !translucent, depthTest: true, alphaTest: 0 };
  }
}

/** `renderamt` 0..255 -> opacidade 0..1. */
export const opacityOf = (renderamt: number): number => Math.max(0, Math.min(255, renderamt)) / 255;

// ------------------------------------------------------------------ frames

/** Chave estável de um segmento da animação: a textura só é recriada quando ela muda. */
export function segmentKey(seg: Pick<SpriteSegment, "frame" | "image"> | undefined): string {
  return seg ? `${seg.frame}:${seg.image}` : "";
}

// ------------------------------------------------------------------ cena

/** Grade do chão: lado múltiplo de 64 que cobre ~4x o maior frame, com no máximo 64 células. */
export function gridSpec(maxDim: number): { size: number; divisions: number } {
  const size = Math.max(128, Math.ceil((Math.max(1, maxDim) * 4) / 64) * 64);
  const divisions = Math.min(64, Math.max(4, Math.round(size / 16)));
  return { size, divisions };
}
