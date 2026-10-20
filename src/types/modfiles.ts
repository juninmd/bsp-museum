// Contrato do explorador de arquivos do mod: espelha, campo a campo, as structs
// serializadas por `bsp_museum_core::{modfiles, audio, pak, nav, demo, textfiles}`.
// Nomes idênticos ao JSON; `null` = `Option::None`.

export type FileKind =
  | "map"
  | "wad"
  | "model"
  | "sprite"
  | "sound"
  | "image"
  | "nav"
  | "demo"
  | "text"
  | "pak"
  | "dll"
  | "cfg"
  | "other";

/** Camada onde o arquivo está: solto no mod, solto em `valve/` ou dentro de um `.pak`. */
export type Layer = "mod" | "valve" | "pak";
/** Pasta raiz de origem (um `.pak` mora numa delas). */
export type Root = "mod" | "valve";

export interface ModFileEntry {
  /** caminho virtual relativo à raiz, com `/`; em item de pak é o caminho dentro do pak */
  path: string;
  name: string;
  kind: FileKind;
  size: number;
  layer: Layer;
  origin: Root;
  /** só na camada `pak`: caminho do `.pak` relativo à raiz `origin` */
  pak: string | null;
}

/** Caminho que abre exatamente esta entrada em `read_mod_file`/`analyze_mod_file`. */
export function openPath(e: Pick<ModFileEntry, "path" | "pak">): string {
  return e.pak ? `${e.pak}/${e.path}` : e.path;
}

export interface KindTotal {
  kind: FileKind;
  count: number;
  bytes: number;
}

export interface ModListing {
  mod_root: string;
  valve_root: string | null;
  entries: ModFileEntry[];
  totals: KindTotal[];
  /** a listagem parou no teto de entradas */
  truncated: boolean;
  warnings: string[];
}

export interface ModTreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  kind: FileKind | null;
  /** arquivo: tamanho; pasta: soma dos filhos */
  size: number;
  file_count: number;
  layers: Layer[];
  children: ModTreeNode[];
}

/** Identificação do que foi aberto (campos achatados em `FilePreview` e `ModFileAnalysis`). */
export interface ModFileInfo {
  path: string;
  name: string;
  kind: FileKind;
  size: number;
  layer: Layer;
  origin: Root;
  pak: string | null;
}

export interface FilePreview extends ModFileInfo {
  bytes_read: number;
  truncated: boolean;
  is_text: boolean;
  /** conteúdo decodificado (quando `is_text`) */
  text: string | null;
  /** conteúdo em base64 (quando binário) */
  base64: string | null;
}

// ---- áudio ---------------------------------------------------------------------

export interface TextTag {
  key: string;
  value: string;
}

export interface CuePoint {
  id: number;
  /** posição em frames de amostra desde o início do `data` */
  sample_offset: number;
}

export interface LoopPoint {
  start_sample: number;
  /** `null` = até o fim do arquivo */
  end_sample: number | null;
  /** "cue" (regra do GoldSrc/ambient_generic) ou "smpl" */
  source: "cue" | "smpl";
}

export interface AudioInfo {
  container: "wav" | "mp3";
  /** "pcm", "float", "ms-adpcm", "ima-adpcm", "alaw", "mulaw", "mp3", "desconhecido(0x....)" */
  codec: string;
  format_tag: number | null;
  channels: number;
  sample_rate: number;
  bits_per_sample: number;
  block_align: number;
  bitrate_kbps: number | null;
  sample_frames: number | null;
  duration_secs: number;
  duration_approx: boolean;
  size_bytes: number;
  /** o `<audio>` do WebView toca (PCM/float e MP3) */
  playable: boolean;
  cue_points: CuePoint[];
  loops: LoopPoint[];
  /** WAV: LIST/INFO (INAM, IART…); MP3: title, artist, album, year, genre, track */
  tags: TextTag[];
  /** maior valor absoluto da onda, 0..1 */
  peak: number | null;
  /** pares [min, max] em -1..1 (escala cheia), um por trecho; null se não for PCM */
  envelope: [number, number][] | null;
  warnings: string[];
}

export interface AudioData {
  /** "audio/wav" | "audio/mpeg" */
  mime: string;
  size_bytes: number;
  /** `data:<mime>;base64,...` para o `<audio src>` */
  data_url: string;
}

// ---- pak -----------------------------------------------------------------------

export interface PakEntry {
  name: string;
  offset: number;
  size: number;
}

export interface PakListing {
  file_size: number;
  entries: PakEntry[];
  skipped: number;
  warnings: string[];
}

// ---- nav -----------------------------------------------------------------------

/** bits de `NavArea.attributes` */
export const NAV_CROUCH = 0x01;
export const NAV_JUMP = 0x02;
export const NAV_PRECISE = 0x04;
export const NAV_NO_JUMP = 0x08;

export interface NavArea {
  id: number;
  attributes: number;
  min: [number, number, number];
  max: [number, number, number];
  ne_z: number;
  sw_z: number;
  /** conexões de saída: [norte, leste, sul, oeste] */
  connections: [number, number, number, number];
  hiding_spots: number;
  place: string | null;
}

export interface NavInfo {
  version: number;
  bsp_size: number | null;
  /** o tamanho gravado bate com o .bsp irmão (null = não conferido) */
  bsp_size_matches: boolean | null;
  /** false = versão desconhecida, só o cabeçalho foi lido */
  parsed: boolean;
  area_count: number;
  connection_count: number;
  hiding_spot_count: number;
  approach_count: number;
  encounter_count: number;
  places: string[];
  /** [minx, miny, minz, maxx, maxy, maxz] */
  bounds: [number, number, number, number, number, number] | null;
  areas: NavArea[];
  areas_truncated: boolean;
  warnings: string[];
}

// ---- demo ----------------------------------------------------------------------

export interface DemoEntry {
  index: number;
  kind: number;
  kind_name: "loading" | "playback" | "desconhecido";
  title: string;
  flags: number;
  cd_track: number;
  duration_secs: number;
  frames: number;
  offset: number;
  length: number;
}

export interface DemoInfo {
  demo_protocol: number;
  network_protocol: number;
  network_version_hint: string | null;
  map_name: string;
  game_dir: string;
  map_crc: number;
  directory_offset: number;
  entries: DemoEntry[];
  /** soma dos segmentos que não são "loading" */
  duration_secs: number;
  frames: number;
  file_size: number;
  warnings: string[];
}

// ---- arquivos de texto ---------------------------------------------------------

export interface KeyValue {
  key: string;
  value: string;
}

export interface Liblist {
  entries: KeyValue[];
  game: string | null;
  gamedll: string | null;
  gamedll_linux: string | null;
  startmap: string | null;
  trainmap: string | null;
  /** valor da chave `type` */
  game_type: string | null;
  version: string | null;
  url_info: string | null;
  url_dl: string | null;
  warnings: string[];
}

export interface HudSprite {
  name: string;
  resolution: number;
  file: string;
  /** `sprites/<file>.spr` */
  sprite_path: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HudSprites {
  declared_count: number | null;
  entries: HudSprite[];
  skipped: number;
  warnings: string[];
}

export interface DetailTexture {
  texture: string;
  detail: string;
  /** `gfx/<detail>` (o motor acrescenta a extensão, normalmente .tga) */
  detail_path: string;
  x_scale: number;
  y_scale: number;
}

export interface DetailTextures {
  entries: DetailTexture[];
  skipped: number;
  warnings: string[];
}

export interface MapCycleEntry {
  map: string;
  min_players: number | null;
  max_players: number | null;
}

export interface MapCycle {
  entries: MapCycleEntry[];
  warnings: string[];
}

export interface TextDoc {
  text: string;
  line_count: number;
  bytes: number;
  truncated: boolean;
}

// ---- análise por formato -------------------------------------------------------

export type Analysis =
  | ({ type: "audio" } & AudioInfo)
  | ({ type: "nav" } & NavInfo)
  | ({ type: "demo" } & DemoInfo)
  | ({ type: "pak" } & PakListing)
  | ({ type: "liblist" } & Liblist)
  | ({ type: "hud_sprites" } & HudSprites)
  | ({ type: "detail_textures" } & DetailTextures)
  | ({ type: "map_cycle" } & MapCycle)
  | ({ type: "text" } & TextDoc)
  | { type: "none" };

export interface ModFileAnalysis extends ModFileInfo {
  analysis: Analysis;
}

// ---- comandos Tauri sugeridos --------------------------------------------------
// (assinaturas de `invoke`; os comandos ainda não existem em main.rs)
//
//   list_mod_files(modRoot, valveRoot?, includePaks?, maxEntries?) -> ModListing
//   read_mod_file(modRoot, rel, maxBytes, origin?)                 -> FilePreview
//   analyze_mod_file(modRoot, rel, origin?)                        -> ModFileAnalysis
//   read_mod_audio(modRoot, rel, origin?)                          -> AudioData
//   list_pak(modRoot, rel)                                         -> PakListing
//
// A árvore se monta no frontend a partir de `ModListing.entries` (ou no backend com
// `modfiles::build_tree` -> `ModTreeNode`).
