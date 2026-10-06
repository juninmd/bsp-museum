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
  /** regras decididas só com entidades + cabeçalho (sem `poucos-spawns`) */
  problems: Problem[];
  /** 30 = GoldSrc, 29 = Quake */
  bsp_version: number;
}

export type Severity = "critical" | "warn" | "info";

export interface Problem {
  id: string;
  severity: Severity;
}

export interface Finding {
  id: string;
  severity: Severity;
  /** texto em português vindo do backend; `args` permite remontar em outro idioma */
  title: string;
  detail: string;
  hint: string;
  args: string[];
}

export type ResourceKind =
  | "mapa"
  | "res"
  | "txt"
  | "overview"
  | "wad"
  | "sky"
  | "model"
  | "sprite"
  | "sound"
  | "extra";

export interface ResourceItem {
  kind: ResourceKind;
  path: string;
  size: number;
  found: boolean;
  /** já existe no jogo base (valve/ ou WAD padrão): fora do FastDL */
  shared: boolean;
}

export interface ResourceReport {
  items: ResourceItem[];
  download_size: number;
  download_count: number;
  missing: number;
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
  resources: ResourceReport;
}

export interface Annotation {
  favorite: boolean;
  tags: string[];
  note: string;
}

export type Lang = "pt" | "en";
export type Theme = "dark" | "light";

export interface Settings {
  last_dir: string | null;
  slots: number;
  lang: Lang | null;
  theme: Theme | null;
  /** chave = nome do arquivo do mapa (com extensão, sem pasta) */
  annotations: Record<string, Annotation>;
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
  /** nomes das sequências — metadado; trocar não muda a pose desenhada nesta versão */
  sequences: string[];
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
  /** atlas de lightmaps (`data:image/png`); null em mapa fullbright */
  lightmap: string | null;
  /** uv no atlas por vértice (6 floats por triângulo); vazio sem atlas */
  lm_uvs: number[];
  /** face do BSP de cada triângulo; -1 = prop `.mdl` (sempre visível) */
  tri_face: number[];
  pvs: PvsData | null;
  bsp_version: number;
}

export interface PvsData {
  /** 4 floats por plano: nx, ny, nz, dist */
  planes: number[];
  /** 3 ints por nó: plano, filho 0 (frente), filho 1 (trás); negativo = folha `-1 - n` */
  nodes: number[];
  /** 4 ints por folha: contents, visofs, primeira marksurface, quantidade */
  leaves: number[];
  marksurfaces: number[];
  /** lump de visibilidade em base64 */
  visibility: string;
  headnode: number;
  visleafs: number;
  world_first_face: number;
  world_face_count: number;
}

export interface EntityRow {
  index: number;
  classname: string;
  targetname: string | null;
  origin: [number, number, number] | null;
  model: string | null;
  keys: [string, string][];
}

export interface AuditRow {
  name: string;
  path: string;
  file_size: number;
  mode: GameMode;
  ct_spawns: number;
  t_spawns: number;
  bsp_version: number;
  error: string | null;
  findings: Finding[];
}

export interface AuditReport {
  dir: string;
  slots: number;
  rows: AuditRow[];
  duplicates: string[][];
}

export interface CompareSide {
  name: string;
  title: string | null;
  file_size: number;
  mode: GameMode;
  bsp_version: number;
  ct_spawns: number;
  t_spawns: number;
  entities: number;
  faces: number;
  vertices: number;
  textures: number;
  bounds: Bounds | null;
  fullbright: boolean;
  findings: [string, Severity][];
}

export interface CountDiff {
  name: string;
  a: number;
  b: number;
}

export interface Comparison {
  a: CompareSide;
  b: CompareSide;
  lumps: CountDiff[];
  entities: CountDiff[];
  textures_only_a: string[];
  textures_only_b: string[];
  wads_only_a: string[];
  wads_only_b: string[];
}
