export type GameMode =
  | "bomb"
  | "hostage"
  | "assassination"
  | "escape"
  | "zombie"
  | "deathmatch"
  | "unknown";

export interface Bounds {
  mins: [number, number, number];
  maxs: [number, number, number];
  size: [number, number, number];
}

export interface MapSummary {
  path: string;
  name: string;
  file_size: number;
  title: string | null;
  mode: GameMode;
  mode_label: string;
  mode_by_entities: GameMode;
  ct_spawns: number;
  t_spawns: number;
  entity_count: number;
  bounds: Bounds | null;
  fullbright: boolean;
  error: string | null;
}

export type Severity = "critical" | "warn" | "info";

export interface Finding {
  id: string;
  severity: Severity;
  title: string;
  detail: string;
  hint: string;
}

export interface LumpInfo {
  name: string;
  length: number;
  percent: number;
}

export interface MapDetail {
  summary: MapSummary;
  sky: string | null;
  wads: string[];
  textures: string[];
  texture_count: number;
  embedded_textures: number;
  face_count: number;
  vertex_count: number;
  model_count: number;
  histogram: [string, number][];
  lumps: LumpInfo[];
  findings: Finding[];
  svg: string;
  polygons: number;
}

export interface Settings {
  last_dir: string | null;
  slots: number;
}

export interface SpawnPoint {
  team: string;
  position: [number, number, number];
}

export interface MeshTexture {
  name: string;
  /** `data:image/png;base64,...`; null = textura de WAD sem pixels */
  png: string | null;
}

export interface SkyBox {
  up: string;
  down: string;
  left: string;
  right: string;
  front: string;
  back: string;
}

export interface ModelDir {
  name: string;
  path: string;
  count: number;
}

/** `.mdl` isolado (visualizador avulso) — mesma malha não-indexada do `MeshDetail`. */
export interface MdlSummary {
  positions: number[];
  uvs: number[];
  texindex: number[];
  textures: MeshTexture[];
  /** nomes das sequências (mesma ordem de `sequence_info`) */
  sequences: string[];
  sequence_info: MdlSeqInfo[];
  /** mesmos vértices de `positions`, no espaço local do bone dono — entrada do skinning */
  local_positions: number[];
  /** bone dono de cada vértice (3 por triângulo) */
  vert_bones: number[];
  num_bones: number;
  /** por família de skin: textura que substitui cada textura base (índice = textura da família 0) */
  skin_families: number[][];
}

export interface MdlSeqInfo {
  name: string;
  fps: number;
  frames: number;
  looping: boolean;
  blends: number;
  /** 0 = dados no próprio arquivo; N > 0 = no arquivo externo `nome0N.mdl` */
  group: number;
}

/** Quadros de uma sequência: pose de mundo por bone, 7 floats (px,py,pz,qx,qy,qz,qw) por bone por quadro. */
export interface MdlSeqFrames {
  frames: number;
  bones: number;
  fps: number;
  looping: boolean;
  data: number[];
}

export interface MeshDetail {
  /** xyz por vértice, 9 floats por triângulo */
  positions: number[];
  /** uv por vértice, 6 floats por triângulo */
  uvs: number[];
  /** índice de textura por triângulo; u32::MAX = sem imagem */
  texindex: number[];
  textures: MeshTexture[];
  spawns: SpawnPoint[];
  bounds: Bounds | null;
  /** céu do mapa (gfx/env), quando existe */
  skybox: SkyBox | null;
  /** quantas texturas vieram de WAD externo */
  wad_textures: number;
  triangles: number;
  skipped: number;
}
