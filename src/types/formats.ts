// Contrato dos visualizadores de formatos do mod (sprites .spr, WAD, imagens soltas).
// Espelha, campo a campo, o JSON que o backend em Rust serializa
// (`spr.rs`, `wadlist.rs`, `image.rs`) — nomes em snake_case, enums em snake_case.
//
// Comandos Tauri sugeridos (argumentos em snake_case no invoke):
//   load_sprite(path)                      -> SpriteInfo
//   list_wad(path)                         -> WadListing
//   wad_texture(path, name, with_mips)     -> WadTexture
//   wad_usage(wad_path, maps_dir)          -> WadUsage
//   load_image(path)                       -> ImageInfo
// Erros chegam como string legível (rejeição do invoke).

/** `data:image/png;base64,...` */
export type PngDataUrl = string;

// ------------------------------------------------------------------ sprites (.spr)

export type SpriteType =
  | "parallel_upright"
  | "facing_upright"
  | "parallel"
  | "oriented"
  | "parallel_oriented";

export type SpriteTexFormat = "normal" | "additive" | "index_alpha" | "alpha_test";

export type SpriteSyncType = "sync" | "random";

export type SpriteFrameKind = "single" | "group";

/** Uma imagem do sprite, com a transparência da `tex_format` já aplicada no PNG. */
export interface SpriteImage {
  /** deslocamento do canto superior esquerdo em relação à origem do sprite (costuma ser negativo) */
  origin_x: number;
  origin_y: number;
  width: number;
  height: number;
  png: PngDataUrl;
}

export interface SpriteFrame {
  kind: SpriteFrameKind;
  /** segundos que cada imagem do grupo fica na tela; vazio em frame simples */
  durations: number[];
  /** 1 imagem em frame simples; N num grupo */
  images: SpriteImage[];
}

export interface SpriteInfo {
  version: number;
  sprite_type: SpriteType;
  tex_format: SpriteTexFormat;
  bounding_radius: number;
  max_width: number;
  max_height: number;
  frame_count: number;
  beam_length: number;
  sync_type: SpriteSyncType;
  /** `tex_format === "additive"`: o shader deve tratar preto como transparente */
  additive: boolean;
  palette_colors: number;
  /** soma das durações dos grupos, em segundos (0 sem grupo) */
  total_duration: number;
  frames: SpriteFrame[];
}

// ------------------------------------------------------------------ WAD

export type WadKind = "miptex" | "decal" | "qpic" | "font" | "console" | "palette" | "other";

export interface WadEntry {
  name: string;
  kind: WadKind;
  /** byte `type` cru do diretório (0x43 miptex, 0x40 decal, 0x42 qpic, 0x46 fonte...) */
  type_code: number;
  /** 0 quando o tipo não tem dimensões legíveis */
  width: number;
  height: number;
  /** nome começa com `{` (miptex/decal): índice 255 é buraco */
  transparent: boolean;
  /** bytes do lump */
  size: number;
  compressed: boolean;
  /** `wad_texture` consegue gerar PNG (miptex/decal/qpic sem compressão) */
  decodable: boolean;
}

export interface WadListing {
  path: string;
  name: string;
  file_size: number;
  /** "WAD2" | "WAD3" */
  magic: string;
  lump_count: number;
  /** miptex + decal */
  texture_count: number;
  entries: WadEntry[];
}

export interface WadMip {
  /** 1, 2 ou 3 (o mip0 está em `WadTexture.png`) */
  level: number;
  width: number;
  height: number;
  png: PngDataUrl;
}

export interface WadTexture {
  name: string;
  kind: WadKind;
  width: number;
  height: number;
  transparent: boolean;
  /** mip0 */
  png: PngDataUrl;
  /** mips 1-3; vazio se `with_mips` foi falso */
  mips: WadMip[];
}

export interface TextureUsage {
  name: string;
  /** quantos mapas usam (exato) */
  map_count: number;
  /** nomes de mapa (sem pasta/.bsp), ordenados, cortados em 100 */
  maps: string[];
}

export interface WadUsage {
  wad: string;
  maps_dir: string;
  maps_scanned: number;
  maps_unreadable: number;
  /** a pasta tem mais de 5000 mapas: só os primeiros foram lidos */
  truncated: boolean;
  /** uma entrada por textura do WAD, por nome; `map_count` 0 = sem uso */
  textures: TextureUsage[];
  unused_count: number;
}

// ------------------------------------------------------------------ imagem solta

export type ImageFormat = "tga" | "bmp" | "lmp";

/** De onde vieram as cores: sem paleta, paleta do arquivo, ou índices em cinza (.lmp do Quake). */
export type ImagePaletteKind = "none" | "embedded" | "grayscale";

export interface ImageInfo {
  format: ImageFormat;
  width: number;
  height: number;
  /** bits por pixel do arquivo (8, 16, 24 ou 32) */
  bits_per_pixel: number;
  has_alpha: boolean;
  /** BMP RLE8 / TGA com RLE */
  rle: boolean;
  palette: ImagePaletteKind;
  png: PngDataUrl;
}
