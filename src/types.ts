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
