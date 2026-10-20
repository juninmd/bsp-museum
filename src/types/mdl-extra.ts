// Contrato do frontend para os extras do `.mdl` (bone controllers, blends, bodygroups,
// remap de cor de time, material, hitboxes, attachments, eventos).
//
// Os nomes são os do serde do backend (`src-tauri/src/mdl/extra.rs`, achatados em
// `MdlSummary` por `#[serde(flatten)]`). Tudo aqui é adição: os campos antigos de
// `MdlSummary`/`MdlSeqFrames` (src/types.ts) não mudam. Tabelas vazias não são enviadas
// pelo backend, então os campos de lista são opcionais.
//
// Comandos (ainda a registrar no main.rs; as funções já existem em `catalog.rs`):
//
//   load_model(path, body?, skin?, top?, bottom?)           -> MdlSummaryEx
//     body    valor de `body` (um submodel por bodypart; ver `bodyValue`), padrão 0
//     skin    família de skin; com skin > 0 o `texindex` já vem trocado
//     top     cor de cima 0..255 (matiz HL) das texturas de remap; bottom idem
//   load_sequence(path, index, blend?, controllers?, hitboxes?) -> MdlSeqFramesEx
//     blend        [b0, b1] bytes 0..255 dos blenders (mira/marcha)
//     controllers  [c0, c1, c2, c3, boca] bytes 0..255 (ver `ControllerInfo`)
//     hitboxes     true = devolve as caixas de acerto por quadro
//   As chamadas antigas (`load_model(path)`, `load_sequence(path, index)`) continuam válidas.

import type { MdlSeqFrames, MdlSummary } from "../types.ts";

export interface BoneInfo {
  name: string;
  /** -1 = raiz */
  parent: number;
}

/** `mstudiobonecontroller_t`: um slider por controlador. */
export interface ControllerInfo {
  /** slot de entrada em `controllers`: 0..3 = olhar/mira, 4 = boca */
  index: number;
  /** "controller0".."controller3" ou "mouth" */
  name: string;
  bone: number;
  /** eixo que o controlador move */
  kind: "x" | "y" | "z" | "xr" | "yr" | "zr" | "outro";
  /** dá a volta (byte 0..255 = 0..360°) em vez de ir de start a end */
  wrap: boolean;
  start: number;
  end: number;
  /** byte do repouso: valor inicial do slider */
  rest: number;
  /** maior byte que ainda muda algo: 255, ou 64 na boca */
  input_max: number;
  unit: "deg" | "units";
  raw_type: number;
}

export interface BodyPartInfo {
  name: string;
  /** multiplicador do `body` neste bodypart */
  base: number;
  /** nome de cada submodel; `models.length` é a quantidade de opções */
  models: string[];
}

/** Escolha do submodel de um bodypart para o valor `body`: `(body / base) % models.length`. */
export function bodyChoice(part: BodyPartInfo, body: number): number {
  const n = part.models.length;
  if (n === 0) return 0;
  const base = Math.max(1, part.base | 0);
  return Math.floor(body / base) % n;
}

/** Valor de `body` que escolhe `choices[i]` em cada bodypart (soma de escolha * base). */
export function bodyValue(parts: BodyPartInfo[], choices: number[]): number {
  let body = 0;
  parts.forEach((p, i) => {
    if (p.models.length === 0) return;
    const c = Math.min(Math.max(0, choices[i] ?? 0), p.models.length - 1);
    body += c * Math.max(1, p.base | 0);
  });
  return body;
}

export interface RemapRange {
  top_start: number;
  top_end: number;
  /** a faixa de baixo começa em top_end + 1 */
  bottom_end: number;
}

/** Material da textura (uma entrada por textura de `textures`, mesma ordem). */
export interface TextureInfo {
  name: string;
  flags: number;
  /** STUDIO_NF_FLATSHADE 0x1 */
  flatshade: boolean;
  /** STUDIO_NF_CHROME 0x2: UV = reflexão, s = (n·right + 1) * 32 / width, t = (n·up + 1) * 32 / height */
  chrome: boolean;
  /** STUDIO_NF_FULLBRIGHT 0x4: sem iluminação */
  fullbright: boolean;
  nomips: boolean;
  alpha: boolean;
  /** STUDIO_NF_ADDITIVE 0x20 */
  additive: boolean;
  /** STUDIO_NF_MASKED 0x40 (índice 255 da paleta é transparente) */
  masked: boolean;
  /** modo de mistura a usar; aditivo vence máscara */
  blend: "opaque" | "masked" | "additive";
  /** textura de cor de time (DM_Base e Remap): refeita pelo backend quando top/bottom são passados */
  remap: RemapRange | null;
}

/** Caixa orientada em espaço do modelo. */
export interface OrientedBox {
  center: [number, number, number];
  half: [number, number, number];
  /** quaternion xyzw do bone */
  rot: [number, number, number, number];
}

export interface HitboxInfo {
  bone: number;
  group: number;
  bbmin: [number, number, number];
  bbmax: [number, number, number];
  /** na pose de repouso */
  rest: OrientedBox;
}

export interface AttachmentInfo {
  name: string;
  type: number;
  bone: number;
  org: [number, number, number];
  vectors: [[number, number, number], [number, number, number], [number, number, number]];
  /** `org` em espaço do modelo na pose de repouso */
  rest: [number, number, number];
}

export interface SeqEvent {
  frame: number;
  event: number;
  type: number;
  options: string;
  /** "muzzleflash" (5001/5011/5021/5031), "spark" (5002), "sound" (5004), "SCRIPT_EVENT_*" (1000..1010); null = desconhecido */
  name: string | null;
  /** >= 5000 é tratado pelo cliente; abaixo, pelo servidor */
  side: "client" | "server";
  /** attachment (0..3) onde acontece, quando o evento define um */
  attachment: number | null;
}

/** Detalhe de cada sequência (mesma ordem de `sequence_info`). */
export interface SeqDetail {
  /** flags cruas (bit 0 = loop, já em `sequence_info[i].looping`) */
  flags: number;
  activity: number;
  act_weight: number;
  /** STUDIO_X/Y/Z (1/2/4): a translação do `motion_bone` é zerada na pose; LX/LY/LZ só informam */
  motion_type: number;
  motion_bone: number;
  linear_movement: [number, number, number];
  /** eixo de cada blender; vazio = sem blender */
  blend_axis: [string, string];
  blend_type: [number, number];
  /** faixa em graus/unidades que cada blender (byte 0..255) cobre */
  blend_start: [number, number];
  blend_end: [number, number];
  blend_parent: number;
  next_seq: number;
  events: SeqEvent[];
}

/** Byte de blender (0..255) para um valor em graus/unidades (`SetBlending` do SDK). */
export function blendByte(d: SeqDetail, which: 0 | 1, value: number): number {
  const type = d.blend_type[which];
  if (type === 0) return 0;
  const start = d.blend_start[which];
  const end = d.blend_end[which];
  let v = value;
  if (type & (0x08 | 0x10 | 0x20)) {
    if (end < start) v = -v;
    if (start + 359 >= end) {
      const mid = (start + end) / 2;
      if (v > mid + 180) v -= 360;
      if (v < mid - 180) v += 360;
    }
  }
  const span = end - start;
  if (Math.abs(span) < 1e-6) return 0;
  return Math.max(0, Math.min(255, Math.trunc((255 * (v - start)) / span)));
}

/** O que `load_model` aplicou. */
export interface Selection {
  body: number;
  skin: number;
  top: number | null;
  bottom: number | null;
}

/** Campos novos de `MdlSummary` (todos opcionais para o frontend antigo e para tabelas vazias). */
export interface MdlExtra {
  bone_info?: BoneInfo[];
  controllers?: ControllerInfo[];
  bodyparts?: BodyPartInfo[];
  texture_info?: TextureInfo[];
  hitboxes?: HitboxInfo[];
  attachments?: AttachmentInfo[];
  seq_details?: SeqDetail[];
  selection?: Selection;
  /** as texturas vieram de `nomeT.mdl` */
  external_textures?: boolean;
  /** tabelas opcionais corrompidas que foram ignoradas */
  warnings?: string[];
}

export type MdlSummaryEx = MdlSummary & MdlExtra;

/** Floats por hitbox por quadro em `MdlSeqFramesEx.hitboxes`. */
export const HITBOX_STRIDE = 10;

/** Campos novos de `MdlSeqFrames`. */
export interface MdlSeqExtra {
  /** origem de cada attachment em espaço do modelo, por quadro: frames * attachments * 3 (x,y,z) */
  attachments?: number[];
  /** só com `hitboxes: true`: frames * hitboxes * 10 (centro 3, meias-arestas 3, quaternion xyzw 4) */
  hitboxes?: number[];
}

export type MdlSeqFramesEx = MdlSeqFrames & MdlSeqExtra;

/** Parâmetros de `load_model` (nomes iguais aos do comando Tauri, em snake_case no invoke). */
export interface LoadModelArgs {
  path: string;
  body?: number;
  skin?: number;
  top?: number;
  bottom?: number;
}

/** Parâmetros de `load_sequence`. */
export interface LoadSequenceArgs {
  path: string;
  index: number;
  /** [blender 0, blender 1], 0..255 */
  blend?: [number, number];
  /** [controlador 0..3, boca], 0..255; ausente = sem controladores (comportamento antigo) */
  controllers?: [number, number, number, number, number];
  hitboxes?: boolean;
}
