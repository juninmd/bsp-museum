/**
 * Relógios da vista 3D: o que cada animação do GoldSrc vale num instante `t` (segundos).
 *
 * Código puro (sem DOM, sem Three.js) para ser testado: estilos de luz, quadros de textura
 * animada, rolagem de UV, efeitos de render (`renderfx`) e o movimento de portas, plataformas,
 * ventiladores e trens. O viewer só chama estas funções a cada quadro e aplica o resultado.
 */

import type { LightStyleDef, MoverKind, Vec3 } from "../types/bsp-extra.ts";

// ---------------------------------------------------------------- estilos de luz

/** Letras do padrão por segundo (o motor avança uma por 0,1 s). */
export const STYLE_FPS = 10;
/** Tamanho da tabela de estilos que o shader recebe: ids 0..63 (255 = slot vazio). */
export const STYLE_SLOTS = 64;
/** Valor da letra `m`, o "brilho normal". */
const NORMAL_LETTER = 12 * 22;

/** Multiplicador de cada letra do padrão (`m` = 1,0; `a` = 0; `z` ≈ 2,08). Fora de a..z vira `m`. */
export function patternValues(pattern: string): number[] {
  const out: number[] = [];
  for (let i = 0; i < Math.min(pattern.length, 256); i++) {
    const c = pattern.charCodeAt(i);
    out.push(c >= 97 && c <= 122 ? ((c - 97) * 22) / NORMAL_LETTER : 1);
  }
  return out;
}

/** O que `styleValueAt` precisa de um estilo (o `LightStyleDef` do backend serve). */
export type StyleLike = Pick<LightStyleDef, "id"> & Partial<Pick<LightStyleDef, "values" | "pattern" | "fps" | "switchable">>;

/** Multiplicador do estilo no instante `time`: `values[floor(t * fps) % values.length]`. Sem padrão vale 1. */
export function styleValueAt(def: StyleLike, time: number): number {
  const values = def.values && def.values.length ? def.values : patternValues(def.pattern ?? "");
  if (!values.length || !Number.isFinite(time)) return 1;
  const fps = def.fps && def.fps > 0 ? def.fps : STYLE_FPS;
  const frame = Math.floor(Math.max(0, time) * fps);
  return values[frame % values.length]!;
}

/**
 * Preenche a tabela `id -> multiplicador` que o shader lê.
 * - `animated` falso: tudo 1,0 (a luz estática de sempre);
 * - luz ligável em `off` vale 0 (apagada), com ou sem animação.
 */
export function fillStyleTable(out: Float32Array, defs: readonly StyleLike[], time: number, animated: boolean, off?: ReadonlySet<number>): void {
  out.fill(1);
  for (const def of defs) {
    if (def.id < 0 || def.id >= out.length) continue;
    if (off?.has(def.id)) out[def.id] = 0;
    else if (animated) out[def.id] = styleValueAt(def, time);
  }
}

// ---------------------------------------------------------------- texturas animadas

/** Segundos por quadro de `+0nome`…`+9nome` (10 quadros/s). */
export const TEXTURE_FRAME_INTERVAL = 0.1;

/** Quadro (0..count-1) no instante `time`; `count <= 1` fica no zero. */
export function frameIndex(count: number, time: number, interval = TEXTURE_FRAME_INTERVAL): number {
  if (count <= 1 || !Number.isFinite(time) || !(interval > 0)) return 0;
  return Math.floor(Math.max(0, time) / interval) % count;
}

// ---------------------------------------------------------------- rolagem de textura

/**
 * Deslocamento de UV (parte fracionária, `[0, 1)`) de uma textura `scroll*` que anda `speed`
 * texels/s para `+s`: a imagem desliza para +s quando a coordenada de amostragem diminui,
 * então `u = -speed * t / largura`. Fica na parte fracionária para não perder precisão em
 * sessões longas.
 */
export function scrollOffset(speed: number, time: number, width: number): number {
  if (!(width > 0) || !Number.isFinite(speed) || !Number.isFinite(time)) return 0;
  const raw = (-speed * time) / width;
  const frac = raw - Math.floor(raw);
  return frac >= 1 ? 0 : frac;
}

// ---------------------------------------------------------------- efeitos de render (renderfx)

/**
 * Fator (0..1) que o `renderfx` aplica à opacidade no instante `time`.
 * 1–4 pulso (lento/rápido, largo = mais fundo), 9–11 strobe, 12–13 tremer; o resto não anima.
 */
export function renderFxFactor(fx: number, time: number): number {
  const sine = (hz: number) => Math.sin(time * hz);
  switch (fx) {
    case 1: return 0.7 + 0.3 * sine(2);
    case 2: return 0.7 + 0.3 * sine(8);
    case 3: return 0.5 + 0.5 * sine(2);
    case 4: return 0.5 + 0.5 * sine(8);
    case 9: return sine(4) > 0 ? 1 : 0.08;
    case 10: return sine(16) > 0 ? 1 : 0.08;
    case 11: return sine(36) > 0 ? 1 : 0.08;
    case 12: return 0.8 + 0.2 * sine(13) * sine(31);
    case 13: return 0.75 + 0.25 * sine(37) * sine(53);
    default: return 1;
  }
}

// ---------------------------------------------------------------- movers

/** Pausa mínima/máxima (s) entre idas e vindas, para a animação ser legível. */
const WAIT_MIN = 0.6;
const WAIT_MAX = 3;
/** Duração mínima/máxima (s) de um trecho do movimento. */
const TRAVEL_MIN = 0.4;
const TRAVEL_MAX = 8;

const clamp = (v: number, lo: number, hi: number) => (v < lo ? lo : v > hi ? hi : v);
const len3 = (v: readonly number[]) => Math.hypot(v[0] ?? 0, v[1] ?? 0, v[2] ?? 0);

/**
 * Fase 0..1 de um vai-e-vem: 0 fechado -> (viagem) -> 1 aberto -> (espera) -> volta -> (espera).
 * `travel` e `wait` em segundos.
 */
export function cyclePhase(travel: number, wait: number, time: number): number {
  const cycle = 2 * (travel + wait);
  if (!(cycle > 0) || !Number.isFinite(time)) return 0;
  const u = ((time % cycle) + cycle) % cycle;
  if (u < travel) return u / travel;
  if (u < travel + wait) return 1;
  if (u < 2 * travel + wait) return 1 - (u - travel - wait) / travel;
  return 0;
}

/** O que o movimento precisa de um `Mover` do backend. */
export interface MoverSpec {
  kind: MoverKind;
  speed: number;
  wait: number;
  open_offset: Vec3 | null;
  rot_delta: Vec3 | null;
  start_open: boolean;
}

export interface MoverPose {
  /** deslocamento em relação à posição do mapa (espaço do mapa, Z-up) */
  offset: Vec3;
  /** rotação em graus (pitch, yaw, roll) em relação à posição do mapa */
  rot: Vec3;
}

const ZERO: Vec3 = [0, 0, 0];

/**
 * Pose de porta, botão, plataforma e ventilador no instante `time`. Trens usam `routePosition`.
 * Aberto/fechado é uma demonstração (vai e volta); no jogo quem manda é o gatilho.
 */
export function moverPose(m: MoverSpec, time: number): MoverPose {
  if (m.kind === "rotating") {
    const r = m.rot_delta ?? ZERO;
    return { offset: ZERO, rot: [r[0] * time, r[1] * time, r[2] * time] };
  }
  const speed = m.speed > 0 ? m.speed : 100;
  const wait = clamp(m.wait, WAIT_MIN, WAIT_MAX);
  if (m.open_offset) {
    const travel = clamp(len3(m.open_offset) / speed, TRAVEL_MIN, TRAVEL_MAX);
    const k = cyclePhase(travel, wait, time) * (m.start_open ? -1 : 1);
    return { offset: [m.open_offset[0] * k, m.open_offset[1] * k, m.open_offset[2] * k], rot: ZERO };
  }
  if (m.rot_delta) {
    const travel = clamp(len3(m.rot_delta) / speed, TRAVEL_MIN, TRAVEL_MAX);
    const k = cyclePhase(travel, wait, time) * (m.start_open ? -1 : 1);
    return { offset: ZERO, rot: [m.rot_delta[0] * k, m.rot_delta[1] * k, m.rot_delta[2] * k] };
  }
  return { offset: ZERO, rot: ZERO };
}

export interface RouteSpec {
  points: readonly Vec3[];
  /** velocidade nova ao chegar em cada ponto (0 = mantém) */
  speeds: readonly number[];
  /** espera em cada ponto */
  waits: readonly number[];
  looped: boolean;
}

/**
 * Posição do **centro** do trem no instante `time`, seguindo a rota ponto a ponto.
 * Rota em circuito volta ao primeiro ponto; rota aberta vai e volta (o trem do jogo pararia
 * no último ponto; ir e voltar mostra o trajeto inteiro sem parar a demonstração).
 */
export function routePosition(route: RouteSpec, baseSpeed: number, time: number): Vec3 {
  const pts = route.points;
  if (!pts.length) return ZERO;
  if (pts.length === 1) return pts[0]!;
  // sequência de pontos visitados, em ordem de viagem
  const seq: number[] = pts.map((_, i) => i);
  if (route.looped) seq.push(0);
  else for (let i = pts.length - 2; i >= 0; i--) seq.push(i);

  let speed = baseSpeed > 0 ? baseSpeed : 100;
  const legs: { from: number; to: number; travel: number; wait: number }[] = [];
  let total = 0;
  for (let i = 0; i + 1 < seq.length; i++) {
    const a = seq[i]!;
    const b = seq[i + 1]!;
    const s = route.speeds[b];
    if (s && s > 0) speed = s;
    const travel = clamp(dist3(pts[a]!, pts[b]!) / speed, 0.05, 60);
    const wait = clamp(route.waits[b] ?? 0, 0, WAIT_MAX);
    legs.push({ from: a, to: b, travel, wait });
    total += travel + wait;
  }
  if (!(total > 0) || !Number.isFinite(time)) return pts[0]!;
  let u = ((time % total) + total) % total;
  for (const leg of legs) {
    if (u < leg.travel) {
      const k = u / leg.travel;
      const p = pts[leg.from]!;
      const q = pts[leg.to]!;
      return [p[0] + (q[0] - p[0]) * k, p[1] + (q[1] - p[1]) * k, p[2] + (q[2] - p[2]) * k];
    }
    u -= leg.travel;
    if (u < leg.wait) return pts[leg.to]!;
    u -= leg.wait;
  }
  return pts[0]!;
}

function dist3(a: readonly number[], b: readonly number[]): number {
  return Math.hypot((b[0] ?? 0) - (a[0] ?? 0), (b[1] ?? 0) - (a[1] ?? 0), (b[2] ?? 0) - (a[2] ?? 0));
}
