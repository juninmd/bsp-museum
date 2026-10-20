/**
 * Contrato do backend para os recursos extras do viewer (v0.7): hulls de colisão, zonas e
 * volumes invisíveis, propriedades de render, texturas animadas, estilos de luz e entidades
 * de cena. Espelha o JSON do serde em `src-tauri/src` — mesmos nomes de campo.
 *
 * Os campos novos de `MeshDetail`/`MapDetail` são aditivos: o que o frontend já consome
 * continua igual. `Option<T>` do Rust chega como `T | null`; `[f32; 3]` como `Vec3`.
 *
 * Comandos Tauri sugeridos (a ligar em `main.rs`):
 *   map_hulls(path, hull, model?, offset?, limit?) -> HullMesh        (hulls::hull_mesh_with)
 *   map_volumes(path)                              -> VolumeReport    (volumes::volumes)
 *   map_scene(path)                                -> SceneEntities   (scene::scene_of)
 * `scene`, `brush_models`, `texture_anims`, `lightstyles`, `face_styles` e `lightmap_layers`
 * já vêm dentro do `MeshDetail` (comando de malha existente); `extra` vem no `MapDetail`.
 */

export type Vec3 = [number, number, number];
export type Rgb = [number, number, number];

// ---------------------------------------------------------------- 1. hulls de colisão

/** Caixa do corpo de um hull. A árvore de clipnodes já vem expandida por ela. */
export interface HullSize {
  /** 1 = em pé, 2 = grande, 3 = agachado */
  hull: 1 | 2 | 3;
  name: string;
  mins: Vec3;
  maxs: Vec3;
}

/**
 * Janela da malha de um hull. Pagine com `next_offset` até ser `null`; cada janela traz
 * vértices próprios (índices locais), então cada uma vira uma `BufferGeometry`.
 */
export interface HullMesh {
  hull: number;
  model: number;
  size: HullSize;
  /** xyz por vértice (3 floats) */
  positions: number[];
  /** 3 índices por triângulo, locais à janela; anti-horário visto do lado vazio */
  indices: number[];
  triangles: number;
  /** triângulos do hull inteiro */
  total_triangles: number;
  offset: number;
  next_offset: number | null;
  /** o orçamento de trabalho cortou a varredura: o hull pode estar incompleto */
  truncated: boolean;
}

export interface HullInfo {
  hull: 1 | 2 | 3;
  name: string;
  mins: Vec3;
  maxs: Vec3;
  /** o modelo 0 tem raiz de clipnodes válida para este hull */
  available: boolean;
}

// ---------------------------------------------------------------- 2. zonas e volumes invisíveis

export type ZoneKind =
  | "trigger"
  | "buy_zone"
  | "bomb_target"
  | "hostage_rescue"
  | "vip_safety"
  | "escape_zone"
  | "ladder"
  | "water";

/** Brush entity classificada (`trigger_*`, `func_buyzone`, `func_bomb_target`…). */
export interface Volume {
  /** posição na lista de entidades do mapa (a mesma de `entity_list`) */
  entity: number;
  /** índice do modelo `*N` */
  model: number;
  classname: string;
  kind: ZoneKind;
  kind_label: string;
  /** `#rrggbb` sugerido */
  color: string;
  /** `message` › `targetname` › `target` › classname */
  label: string;
  target: string | null;
  targetname: string | null;
  message: string | null;
  mins: Vec3;
  maxs: Vec3;
  /** triângulos soltos, 9 floats cada */
  positions: number[];
  triangles: number;
  /** o modelo não tem faces (só clip): `positions` é a caixa do AABB */
  from_box: boolean;
}

/** Faces de uma textura invisível (`clip`, `aaatrigger`, `null`, `origin`, `hint`, `skip`, `sky`…). */
export interface InvisibleSurface {
  texture: string;
  color: string;
  faces: number;
  positions: number[];
  triangles: number;
}

export interface ZoneLegend {
  kind: ZoneKind;
  label: string;
  color: string;
}

export interface VolumeReport {
  volumes: Volume[];
  invisible: InvisibleSurface[];
  legend: ZoneLegend[];
  /** zonas por classname */
  counts: Record<string, number>;
  /** algum limite de triângulos cortou a malha */
  truncated: boolean;
}

// ---------------------------------------------------------------- 3. render props e rolagem

/**
 * Como aplicar (convenção do GoldSrc):
 * - 0 normal: ignora o resto;
 * - 1 color: pinta com `color`, translucidez `amount`;
 * - 2 texture: textura com alpha `amount / 255`;
 * - 3 glow: sprite/brilho sem oclusão (para brush vale como 2);
 * - 4 solid: teste de alpha (texturas `{`), `amount` ignorado;
 * - 5 additive: soma à cena, intensidade `amount / 255`.
 * `fx` é o `kRenderFx*` do `const.h` (1–2 pulso, 9–11 strobe, 12–13 tremer, 14 brilho
 * constante, 19 glow shell…).
 */
export interface RenderProps {
  mode: number;
  mode_name: string;
  /** 0..255; ausente no mapa = 0 */
  amount: number;
  color: Rgb;
  fx: number;
  fx_name: string;
}

export type ScrollSource = "rendercolor" | "conveyor_speed";

/**
 * Rolagem horizontal das texturas `scroll*` de um modelo. `speed` em texels/s, com sinal;
 * positivo = a imagem anda para `+s`. Regra: `(G << 8 | B) / 16`, negativa se `R != 0`
 * (o `func_conveyor` deriva isso de `speed`). Deslocamento de UV: `-speed * t / largura`.
 */
export interface ScrollInfo {
  speed: number;
  source: ScrollSource;
  textures: string[];
}

/**
 * Brush entity (`*N`, N >= 1). Uma face pertence ao modelo cuja faixa
 * `first_face .. first_face + num_faces` a contém (`MeshDetail.tri_face` dá a face de cada triângulo).
 */
export interface BrushModelInfo {
  model: number;
  entity: number;
  classname: string;
  targetname: string | null;
  mins: Vec3;
  maxs: Vec3;
  /** chave `origin` da entidade (brush com *origin brush*); zero se não há */
  origin: Vec3;
  first_face: number;
  num_faces: number;
  /** só quando difere do padrão (modo != normal ou efeito != 0) */
  render: RenderProps | null;
  scroll: ScrollInfo | null;
}

// ---------------------------------------------------------------- 4. texturas animadas

/**
 * `+0nome`…`+9nome` giram a cada `interval` s (10 quadros/s); com a entidade "ativa"
 * (botão apertado) vale a sequência `+Anome`…`+Jnome`. As faces usam `texture`.
 * `frame_slots`/`alt_slots` indexam `MeshDetail.textures` (-1 = quadro sem imagem).
 */
export interface TextureAnim {
  /** nome-base, minúsculo, sem o `+N` */
  name: string;
  texture: string;
  frames: string[];
  alt_frames: string[];
  interval: number;
  /** false = buraco na numeração dos quadros */
  complete: boolean;
  frame_slots: number[];
  alt_slots: number[];
}

// ---------------------------------------------------------------- 5. estilos de luz

export type StyleSource = "engine" | "light_entity" | "unknown";

/**
 * Padrão de um estilo. Quadro atual: `values[floor(t * fps) % values.length]` (`m` = 1.0;
 * a letra `c` vale `(c - 'a') * 22 / 264`). Estilos 0–12 e 63 são do motor; 32+ são de
 * luzes ligáveis (`switchable`: um gatilho liga/desliga — o viewer decide o estado).
 */
export interface LightStyleDef {
  id: number;
  name: string;
  pattern: string;
  values: number[];
  fps: number;
  source: StyleSource;
  switchable: boolean;
  targetname: string | null;
  /** começa apagada (`spawnflags & 1`) */
  start_off: boolean;
  /** faces que usam o estilo */
  faces: number;
}

/**
 * Camada extra do lightmap: o slot de estilo `slot` (1..3) das faces, com o mesmo layout e
 * as mesmas UVs (`lm_uvs`) do atlas `lightmap`. Fundo preto: o shader soma
 *   luz = atlas0 * mult(styles[0]) + Σ camada[k] * mult(styles[k])
 * onde `styles = face_styles[4*face .. 4*face+4]` (255 = slot vazio) e `mult` vem do padrão.
 * O atlas `lightmap` continua sendo o slot 0 de todas as faces, como antes.
 */
export interface LightLayer {
  slot: 1 | 2 | 3;
  /** `data:image/png;base64,...` */
  png: string;
}

// ---------------------------------------------------------------- 6. entidades de cena

/** Campos comuns a toda entidade de cena (achatados no mesmo objeto). */
export interface SceneBase {
  /** posição na lista de entidades do mapa */
  entity: number;
  classname: string;
  targetname: string | null;
  /** `origin`; centro do modelo para brush entity sem `origin` */
  origin: Vec3;
}

export interface EnvFog extends SceneBase {
  type: "env_fog";
  color: Rgb;
  start: number;
  end: number;
  density: number | null;
}

export interface EnvSky extends SceneBase {
  type: "env_sky";
  skyname: string | null;
  angles: Vec3;
}

export interface LightEnvironment extends SceneBase {
  type: "light_environment";
  angles: Vec3;
  /** chave `pitch` (sobrescreve o pitch dos `angles`) */
  pitch: number | null;
  /** para onde a luz viaja (unitário); convenção do Hammer: pitch negativo = para baixo */
  direction: Vec3;
  color: Rgb;
  brightness: number;
  ambient_color: Rgb | null;
  ambient_brightness: number | null;
  spread: number | null;
}

export interface Decal extends SceneBase {
  type: "decal";
  texture: string | null;
  /** `decal` (brush) em vez de `infodecal` (ponto) */
  brush: boolean;
}

export interface LightSource extends SceneBase {
  type: "light";
  /** `light_spot` */
  spot: boolean;
  color: Rgb;
  brightness: number;
  /** estilo de luz (0 = normal) */
  style: number;
  pattern: string | null;
  switchable: boolean;
  start_off: boolean;
  target: string | null;
  angles: Vec3;
  /** direção do cone (só spot) */
  direction: Vec3 | null;
  cone: number | null;
  cone2: number | null;
}

export interface SpriteEnt extends SceneBase {
  type: "sprite";
  /** `env_glow` em vez de `env_sprite` */
  glow: boolean;
  /** caminho do `.spr` */
  model: string | null;
  scale: number;
  framerate: number;
  render: RenderProps;
}

export interface BeamEnt extends SceneBase {
  type: "beam";
  texture: string | null;
  start: Vec3 | null;
  end: Vec3 | null;
  start_target: string | null;
  end_target: string | null;
  width: number;
  noise: number;
  render: RenderProps;
}

export interface AmbientSound extends SceneBase {
  type: "ambient";
  sound: string | null;
  /** 0..1 */
  volume: number;
  /** unidades; `null` = toca em todo o mapa */
  radius: number | null;
  spawnflags: number;
  start_silent: boolean;
}

export type MoverKind =
  | "door"
  | "door_rotating"
  | "plat"
  | "train"
  | "track_train"
  | "rotating"
  | "button"
  | "rot_button"
  | "conveyor";

/** Porta, plataforma, trem, botão, ventilador ou esteira. */
export interface Mover extends SceneBase {
  type: "mover";
  kind: MoverKind;
  /** índice do modelo `*N` (casa com `BrushModelInfo.model`) */
  model: number | null;
  /** limites do modelo (espaço do modelo; some `origin` se há *origin brush*) */
  mins: Vec3;
  maxs: Vec3;
  angles: Vec3;
  movedir: Vec3;
  /** unidades/s (graus/s nos rotativos) */
  speed: number;
  lip: number;
  wait: number;
  delay: number;
  height: number;
  distance: number;
  spawnflags: number;
  target: string | null;
  start_open: boolean;
  /** deslocamento da posição fechada para a aberta (porta, botão, plataforma) */
  open_offset: Vec3 | null;
  /** (pitch, yaw, roll): rotação até aberta, ou graus/s no `func_rotating` */
  rot_delta: Vec3 | null;
  render: RenderProps | null;
  /** texels/s da esteira */
  scroll_speed: number | null;
}

export interface PathCorner extends SceneBase {
  type: "path_corner";
  target: string | null;
  /** nova velocidade do trem ao chegar (0 = mantém) */
  speed: number;
  wait: number;
  yaw_speed: number;
  spawnflags: number;
}

export type SceneEntity =
  | EnvFog
  | EnvSky
  | LightEnvironment
  | Decal
  | LightSource
  | SpriteEnt
  | BeamEnt
  | AmbientSound
  | Mover
  | PathCorner;

/** Rota de um trem: o **centro** da caixa dele passa por `points`. */
export interface TrainRoute {
  /** posição do trem na lista de entidades */
  train: number;
  targetname: string | null;
  points: Vec3[];
  corners: number[];
  speeds: number[];
  waits: number[];
  /** a cadeia volta a um ponto já visitado (circuito) */
  looped: boolean;
  /** a cadeia termina num `target` que não existe */
  broken: boolean;
}

export interface SceneEntities {
  entities: SceneEntity[];
  routes: TrainRoute[];
  /** quantas entidades de cada tipo (`type`) */
  counts: Record<string, number>;
  truncated: boolean;
}

// ---------------------------------------------------------------- campos novos dos tipos existentes

/** Campos acrescentados a `MeshDetail` (`src/types.ts`). */
export interface MeshDetailExtra {
  /** 4 estilos por face do BSP (255 = vazio; tudo 255 = sem lightmap); vazio sem atlas */
  face_styles: number[];
  lightmap_layers: LightLayer[];
  lightstyles: LightStyleDef[];
  texture_anims: TextureAnim[];
  brush_models: BrushModelInfo[];
  scene: SceneEntities;
}

/** Contagens baratas que o `MapDetail` carrega; o conteúdo vem sob demanda. */
export interface ExtraSummary {
  clipnodes: number;
  hulls: HullInfo[];
  /** zonas por classname */
  zones: Record<string, number>;
  invisible_faces: number;
  animated_textures: number;
  lightstyles: number[];
  switchable_lights: number;
  /** entidades de cena por tipo */
  scene: Record<string, number>;
  rendered_models: number;
  scrolling_models: number;
  train_routes: number;
}

/** Campo acrescentado a `MapDetail` (`src/types.ts`). */
export interface MapDetailExtra {
  extra: ExtraSummary;
}
