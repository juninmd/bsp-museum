//! Entidades de cena: o que o viewer 3D desenha além da geometria — névoa, sol, luzes,
//! sprites, raios, sons ambiente, portas/plataformas/trens e as rotas dos trens — e as
//! propriedades de render (`rendermode`, `renderamt`, `rendercolor`, `renderfx`) e de
//! rolagem de textura (`scroll*`) das brush entities.
//!
//! Tudo vem do lump de entidades + os limites dos modelos; nada aqui abre arquivo (o
//! `scene_of(path)` é só a casca). Chave malformada vira o padrão do campo, nunca erro.
//!
//! Os padrões e fórmulas do comportamento dos movers (`lip`, `speed`, `wait`, `height`,
//! eixo de rotação, deslocamento da porta) foram escritos de memória do HLSDK
//! (`doors.cpp`, `buttons.cpp`, `plats.cpp`, `bmodels.cpp`, `triggers.cpp`) e do FGD — não
//! conferidos contra a documentação da VDC (que bloqueou o acesso automático) nem contra
//! um mapa real. Os campos brutos (`spawnflags`, `angles`, chaves) vão junto para o
//! frontend poder corrigir a interpretação sem mexer no backend.

use crate::bsp::entities::{self, Entity};
use crate::bsp::Bsp;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// Teto de entidades de cena devolvidas (mapa hostil com 100 mil `light`).
const MAX_ENTITIES: usize = 12_000;
/// Passos máximos ao seguir uma cadeia de `path_corner`.
const MAX_ROUTE: usize = 2048;

// ------------------------------------------------------------------ leitura de chaves

fn parse_nums(raw: &str) -> Vec<f32> {
    raw.split_whitespace().filter_map(|p| p.parse::<f32>().ok()).filter(|v| v.is_finite()).collect()
}

fn vec3_of(e: &Entity, key: &str) -> Option<[f32; 3]> {
    let n = parse_nums(e.get(key)?);
    (n.len() >= 3).then(|| [n[0], n[1], n[2]])
}

fn f32_of(e: &Entity, key: &str) -> Option<f32> {
    e.get(key)?.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

fn i32_of(e: &Entity, key: &str) -> Option<i32> {
    let raw = e.get(key)?.trim();
    raw.parse::<i32>().ok().or_else(|| raw.parse::<f32>().ok().filter(|v| v.is_finite()).map(|v| v as i32))
}

fn str_of(e: &Entity, key: &str) -> Option<String> {
    e.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()).map(str::to_string)
}

fn class_of(e: &Entity) -> &str {
    e.get("classname").map(String::as_str).unwrap_or("")
}

fn rgb(raw: &[f32]) -> [u8; 3] {
    let c = |i: usize| raw.get(i).copied().unwrap_or(0.0).clamp(0.0, 255.0) as u8;
    [c(0), c(1), c(2)]
}

/// Índice `N` de `"model" "*N"`.
pub fn brush_model_of(e: &Entity) -> Option<usize> {
    e.get("model")?.trim().strip_prefix('*')?.parse().ok()
}

/// `(pitch, yaw, roll)` da entidade, e `pitch` sobrescrevendo o primeiro (chave `pitch`).
fn angles_with_pitch(e: &Entity) -> ([f32; 3], Option<f32>) {
    (entities::angles_of(e), f32_of(e, "pitch"))
}

/// Vetor para onde aponta um `(pitch, yaw)` na convenção do Hammer (pitch negativo =
/// para baixo): `(cos p cos y, cos p sin y, sin p)`.
fn hammer_dir(pitch: f32, yaw: f32) -> [f32; 3] {
    let (p, y) = (pitch.to_radians(), yaw.to_radians());
    [p.cos() * y.cos(), p.cos() * y.sin(), p.sin()]
}

/// `SetMovedir` do SDK: `angles = 0 -1 0` é "para cima", `0 -2 0` "para baixo"; o resto é
/// o vetor `forward` do motor (`(cp cy, cp sy, -sp)`).
pub fn movedir_of(angles: [f32; 3]) -> [f32; 3] {
    if angles == [0.0, -1.0, 0.0] {
        return [0.0, 0.0, 1.0];
    }
    if angles == [0.0, -2.0, 0.0] {
        return [0.0, 0.0, -1.0];
    }
    let (p, y) = (angles[0].to_radians(), angles[1].to_radians());
    [p.cos() * y.cos(), p.cos() * y.sin(), -p.sin()]
}

// ------------------------------------------------------------------ propriedades de render

/// `rendermode`, `renderamt`, `rendercolor`, `renderfx` de uma entidade.
///
/// Como o viewer deve aplicar (convenção do GoldSrc):
/// - `0` normal: ignora o resto;
/// - `1` color: pinta com `color`, translucidez `amount`;
/// - `2` texture: textura com alpha `amount / 255`;
/// - `3` glow: sprites/brilho que não é cortado por geometria à frente (para brush, vale como `2`);
/// - `4` solid: teste de alpha (texturas `{`), `amount` ignorado;
/// - `5` additive: soma à cena, intensidade `amount / 255`.
///
/// `fx` é o `kRenderFx*` do `const.h` (pulso, strobe, glow shell…); `fx_name` é o apelido.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderProps {
    pub mode: u8,
    pub mode_name: &'static str,
    /// 0..255; ausente = 0 (como no motor — `rendermode` normal ignora)
    pub amount: f32,
    pub color: [u8; 3],
    pub fx: u8,
    pub fx_name: &'static str,
}

impl RenderProps {
    /// Sem efeito nenhum: o viewer pode desenhar como geometria comum.
    pub fn is_default(&self) -> bool {
        self.mode == 0 && self.fx == 0
    }
}

pub fn render_mode_name(mode: i32) -> &'static str {
    match mode {
        0 => "normal",
        1 => "color",
        2 => "texture",
        3 => "glow",
        4 => "solid",
        5 => "additive",
        _ => "desconhecido",
    }
}

pub fn render_fx_name(fx: i32) -> &'static str {
    match fx {
        0 => "normal",
        1 => "pulso lento",
        2 => "pulso rápido",
        3 => "pulso lento largo",
        4 => "pulso rápido largo",
        5 => "esmaecer lento",
        6 => "esmaecer rápido",
        7 => "solidificar lento",
        8 => "solidificar rápido",
        9 => "strobe lento",
        10 => "strobe rápido",
        11 => "strobe mais rápido",
        12 => "tremer lento",
        13 => "tremer rápido",
        14 => "brilho constante",
        15 => "distorção",
        16 => "holograma",
        17 => "jogador morto",
        18 => "explodir",
        19 => "glow shell",
        20 => "escala mínima fixa",
        _ => "desconhecido",
    }
}

pub fn render_props(e: &Entity) -> RenderProps {
    let mode = i32_of(e, "rendermode").unwrap_or(0).clamp(0, 255);
    let fx = i32_of(e, "renderfx").unwrap_or(0).clamp(0, 255);
    RenderProps {
        mode: mode as u8,
        mode_name: render_mode_name(mode),
        amount: f32_of(e, "renderamt").unwrap_or(0.0).clamp(0.0, 255.0),
        color: e.get("rendercolor").map(|r| rgb(&parse_nums(r))).unwrap_or([0, 0, 0]),
        fx: fx as u8,
        fx_name: render_fx_name(fx),
    }
}

// ------------------------------------------------------------------ rolagem de textura

/// De onde saiu a velocidade de rolagem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollSource {
    /// `rendercolor` da entidade (`R G B`)
    Rendercolor,
    /// `speed` de um `func_conveyor` (que o jogo converte em `rendercolor` ao nascer)
    ConveyorSpeed,
}

/// Rolagem das texturas `scroll*` de um modelo.
///
/// Regra do motor: texturas cujo nome começa com `scroll` rolam em `s` (horizontal) e a
/// velocidade vem do `rendercolor` da entidade: `(G << 8 | B) / 16` texels por segundo,
/// negativa se `R != 0`. O `func_conveyor` calcula esse `rendercolor` a partir de `speed`
/// (`código = |speed| * 16`, `R = speed < 0`, `G = código >> 8`, `B = código & 255`).
/// Positivo = a imagem anda para o lado de `+s`. Conferido de memória (HLSDK `CFuncConveyor`
/// e o desenho de superfície do Xash3D), não contra o GoldSrc original.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScrollInfo {
    /// texels por segundo, com sinal
    pub speed: f32,
    pub source: ScrollSource,
    /// texturas `scroll*` usadas pelas faces do modelo
    pub textures: Vec<String>,
}

/// Textura que rola? (nome começa com `scroll`, sem diferenciar maiúscula)
pub fn is_scroll_texture(name: &str) -> bool {
    name.len() >= 6 && name.as_bytes()[..6].eq_ignore_ascii_case(b"scroll")
}

/// Velocidade de rolagem que a entidade impõe (ou `None` se for zero).
pub fn scroll_speed(e: &Entity) -> Option<(f32, ScrollSource)> {
    if class_of(e) == "func_conveyor" {
        let mut speed = f32_of(e, "speed").unwrap_or(0.0);
        if speed == 0.0 {
            speed = 100.0; // CFuncConveyor::Spawn
        }
        let code = ((speed.abs() * 16.0) as u32).min(0xFFFF);
        let v = code as f32 / 16.0;
        return (code > 0).then_some((if speed < 0.0 { -v } else { v }, ScrollSource::ConveyorSpeed));
    }
    let c = parse_nums(e.get("rendercolor")?);
    if c.len() < 3 {
        return None;
    }
    let (r, g, b) = (c[0].clamp(0.0, 255.0) as u32, c[1].clamp(0.0, 255.0) as u32, c[2].clamp(0.0, 255.0) as u32);
    let code = (g << 8) | b;
    if code == 0 {
        return None;
    }
    let v = code as f32 / 16.0;
    Some((if r != 0 { -v } else { v }, ScrollSource::Rendercolor))
}

/// Brush entity (`model "*N"`) com o que o viewer precisa por modelo: faixa de faces,
/// limites e as propriedades de render/rolagem. Os triângulos de `MeshDetail` apontam a
/// face (`tri_face`); a face pertence ao modelo cuja faixa `first_face..first_face+num_faces`
/// a contém.
#[derive(Debug, Clone, Serialize)]
pub struct BrushModelInfo {
    pub model: u32,
    /// posição da entidade na lista de entidades do mapa
    pub entity: usize,
    pub classname: String,
    pub targetname: Option<String>,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// chave `origin` da entidade (brush com *origin brush*); zero se não há
    pub origin: [f32; 3],
    pub first_face: i32,
    pub num_faces: i32,
    /// só quando difere do padrão (modo != normal ou efeito != 0)
    pub render: Option<RenderProps>,
    pub scroll: Option<ScrollInfo>,
}

pub fn brush_models(bsp: &Bsp, parsed: &[Entity]) -> Vec<BrushModelInfo> {
    let mut seen: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for (entity, e) in parsed.iter().enumerate() {
        let Some(n) = brush_model_of(e) else { continue };
        let Some(m) = bsp.models.get(n) else { continue };
        if n == 0 || !seen.insert(n) || out.len() >= 4096 {
            continue;
        }
        let render = render_props(e);
        let scroll = scroll_speed(e).and_then(|(speed, source)| {
            let first = usize::try_from(m.first_face).ok()?;
            let count = usize::try_from(m.num_faces).ok()?;
            let mut textures: Vec<String> = bsp
                .faces
                .get(first..first.saturating_add(count).min(bsp.faces.len()))?
                .iter()
                .filter_map(|f| bsp.texture_name_of(f))
                .filter(|t| is_scroll_texture(t))
                .map(str::to_string)
                .collect();
            textures.sort();
            textures.dedup();
            (!textures.is_empty()).then_some(ScrollInfo { speed, source, textures })
        });
        out.push(BrushModelInfo {
            model: n as u32,
            entity,
            classname: class_of(e).to_string(),
            targetname: str_of(e, "targetname"),
            mins: m.mins,
            maxs: m.maxs,
            origin: vec3_of(e, "origin").unwrap_or([0.0; 3]),
            first_face: m.first_face,
            num_faces: m.num_faces,
            render: (!render.is_default()).then_some(render),
            scroll,
        });
    }
    out
}

// ------------------------------------------------------------------ cena

/// Campos comuns a toda entidade de cena.
#[derive(Debug, Clone, Serialize)]
pub struct Base {
    /// posição na lista de entidades do mapa (a mesma de `entity_list`)
    pub entity: usize,
    pub classname: String,
    pub targetname: Option<String>,
    /// `origin` em coordenadas do mapa (centro do modelo para brush entity sem `origin`)
    pub origin: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvFog {
    #[serde(flatten)]
    pub base: Base,
    pub color: [u8; 3],
    pub start: f32,
    pub end: f32,
    pub density: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvSky {
    #[serde(flatten)]
    pub base: Base,
    pub skyname: Option<String>,
    pub angles: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct LightEnvironment {
    #[serde(flatten)]
    pub base: Base,
    pub angles: [f32; 3],
    /// chave `pitch` (sobrescreve o pitch dos `angles`), se houver
    pub pitch: Option<f32>,
    /// para onde a luz do sol viaja (unitário), pela convenção do Hammer: pitch negativo = para baixo
    pub direction: [f32; 3],
    /// `_light`: cor do sol
    pub color: [u8; 3],
    pub brightness: f32,
    /// `_diffuse_light`: luz ambiente do céu
    pub ambient_color: Option<[u8; 3]>,
    pub ambient_brightness: Option<f32>,
    /// `SunSpreadAngle` do ZHLT
    pub spread: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Decal {
    #[serde(flatten)]
    pub base: Base,
    /// nome da textura em `decals.wad`
    pub texture: Option<String>,
    /// `decal` (brush) em vez de `infodecal` (ponto)
    pub brush: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LightSource {
    #[serde(flatten)]
    pub base: Base,
    /// `light` ou `light_spot`
    pub spot: bool,
    pub color: [u8; 3],
    pub brightness: f32,
    /// estilo de luz (0 = normal; ver `bsp::lightstyle`)
    pub style: u8,
    /// padrão próprio (`pattern`), só vale para estilo >= 32
    pub pattern: Option<String>,
    /// estilo >= 32 com `targetname`: um gatilho liga/desliga
    pub switchable: bool,
    pub start_off: bool,
    pub target: Option<String>,
    pub angles: [f32; 3],
    /// direção do cone (só `light_spot`): para o `target`, se resolvível; senão pelos ângulos
    pub direction: Option<[f32; 3]>,
    /// `_cone` / `_cone2` (graus) do spot
    pub cone: Option<f32>,
    pub cone2: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpriteEnt {
    #[serde(flatten)]
    pub base: Base,
    /// `env_sprite` ou `env_glow`
    pub glow: bool,
    /// caminho do `.spr` (chave `model`)
    pub model: Option<String>,
    pub scale: f32,
    pub framerate: f32,
    pub render: RenderProps,
}

#[derive(Debug, Clone, Serialize)]
pub struct BeamEnt {
    #[serde(flatten)]
    pub base: Base,
    pub texture: Option<String>,
    /// extremidades resolvidas (`LightningStart`/`LightningEnd` são `targetname`s; vazio = o próprio `origin`)
    pub start: Option<[f32; 3]>,
    pub end: Option<[f32; 3]>,
    pub start_target: Option<String>,
    pub end_target: Option<String>,
    pub width: f32,
    pub noise: f32,
    pub render: RenderProps,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmbientSound {
    #[serde(flatten)]
    pub base: Base,
    /// arquivo de som (`message`)
    pub sound: Option<String>,
    /// volume 0..1 (`health` vai de 0 a 10)
    pub volume: f32,
    /// alcance em unidades; `None` = toca em todo o mapa (flag "play everywhere")
    pub radius: Option<f32>,
    pub spawnflags: i32,
    /// começa silencioso (flag 16)
    pub start_silent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoverKind {
    Door,
    DoorRotating,
    Plat,
    Train,
    TrackTrain,
    Rotating,
    Button,
    RotButton,
    Conveyor,
}

fn mover_kind(class: &str) -> Option<MoverKind> {
    Some(match class {
        "func_door" => MoverKind::Door,
        "func_door_rotating" => MoverKind::DoorRotating,
        "func_plat" | "func_platrot" => MoverKind::Plat,
        "func_train" => MoverKind::Train,
        "func_tracktrain" => MoverKind::TrackTrain,
        "func_rotating" => MoverKind::Rotating,
        "func_button" => MoverKind::Button,
        "func_rot_button" => MoverKind::RotButton,
        "func_conveyor" => MoverKind::Conveyor,
        _ => return None,
    })
}

/// Porta, plataforma, trem, botão, ventilador, esteira.
#[derive(Debug, Clone, Serialize)]
pub struct Mover {
    #[serde(flatten)]
    pub base: Base,
    pub kind: MoverKind,
    /// índice do modelo (`*N`), se resolvido
    pub model: Option<u32>,
    /// limites do modelo (espaço do modelo; some `origin` se a entidade tiver *origin brush*)
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub angles: [f32; 3],
    /// direção de movimento (`SetMovedir`) de portas deslizantes e botões
    pub movedir: [f32; 3],
    /// unidades/s (graus/s nos rotativos); padrão do jogo quando a chave falta
    pub speed: f32,
    pub lip: f32,
    pub wait: f32,
    pub delay: f32,
    /// `height` do `func_plat` (0 = automático)
    pub height: f32,
    /// `distance` (graus) de portas/botões rotativos
    pub distance: f32,
    pub spawnflags: i32,
    pub target: Option<String>,
    /// começa na posição "aberta" (flag 1 em portas)
    pub start_open: bool,
    /// deslocamento da posição fechada para a aberta (portas deslizantes, botões, plataforma)
    pub open_offset: Option<[f32; 3]>,
    /// rotação da posição fechada para a aberta, no espaço de `angles` (pitch, yaw, roll),
    /// ou velocidade angular por segundo para `func_rotating`
    pub rot_delta: Option<[f32; 3]>,
    pub render: Option<RenderProps>,
    /// rolagem de textura (esteira)
    pub scroll_speed: Option<f32>,
}

/// `path_corner` / `path_track`: ponto da rota de um trem.
#[derive(Debug, Clone, Serialize)]
pub struct PathCorner {
    #[serde(flatten)]
    pub base: Base,
    pub target: Option<String>,
    /// nova velocidade do trem ao chegar (0 = mantém)
    pub speed: f32,
    pub wait: f32,
    pub yaw_speed: f32,
    pub spawnflags: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneEntity {
    EnvFog(EnvFog),
    EnvSky(EnvSky),
    LightEnvironment(LightEnvironment),
    Decal(Decal),
    Light(LightSource),
    Sprite(SpriteEnt),
    Beam(BeamEnt),
    Ambient(AmbientSound),
    Mover(Mover),
    PathCorner(PathCorner),
}

impl SceneEntity {
    pub fn type_name(&self) -> &'static str {
        match self {
            SceneEntity::EnvFog(_) => "env_fog",
            SceneEntity::EnvSky(_) => "env_sky",
            SceneEntity::LightEnvironment(_) => "light_environment",
            SceneEntity::Decal(_) => "decal",
            SceneEntity::Light(_) => "light",
            SceneEntity::Sprite(_) => "sprite",
            SceneEntity::Beam(_) => "beam",
            SceneEntity::Ambient(_) => "ambient",
            SceneEntity::Mover(_) => "mover",
            SceneEntity::PathCorner(_) => "path_corner",
        }
    }
}

/// Rota de um trem: a cadeia `target` → `targetname` de `path_corner`s.
///
/// O trem se posiciona com o **centro** da caixa dele sobre o ponto (o jogo faz
/// `origin = corner - (mins + maxs) / 2`), então `points` já é onde o centro passa.
#[derive(Debug, Clone, Serialize)]
pub struct TrainRoute {
    /// posição do trem na lista de entidades
    pub train: usize,
    pub targetname: Option<String>,
    pub points: Vec<[f32; 3]>,
    /// posição de cada ponto na lista de entidades
    pub corners: Vec<usize>,
    /// velocidade pedida por cada ponto (0 = mantém)
    pub speeds: Vec<f32>,
    pub waits: Vec<f32>,
    /// a cadeia volta a um ponto já visitado (circuito)
    pub looped: bool,
    /// a cadeia termina num `target` que não existe
    pub broken: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SceneEntities {
    pub entities: Vec<SceneEntity>,
    pub routes: Vec<TrainRoute>,
    /// quantas entidades de cada tipo (`SceneEntity::type_name`)
    pub counts: BTreeMap<String, usize>,
    /// passou de `MAX_ENTITIES` e a lista foi cortada
    pub truncated: bool,
}

/// Ponto de referência de uma entidade: `origin`, senão o centro do modelo `*N`.
fn center_of(bsp: &Bsp, e: &Entity) -> Option<[f32; 3]> {
    entities::origin_of(e).or_else(|| {
        let m = bsp.models.get(brush_model_of(e)?)?;
        Some([
            (m.mins[0] + m.maxs[0]) / 2.0,
            (m.mins[1] + m.maxs[1]) / 2.0,
            (m.mins[2] + m.maxs[2]) / 2.0,
        ])
    })
}

fn size_of(bsp: &Bsp, e: &Entity) -> ([f32; 3], [f32; 3]) {
    match brush_model_of(e).and_then(|n| bsp.models.get(n)) {
        Some(m) => (m.mins, m.maxs),
        None => ([0.0; 3], [0.0; 3]),
    }
}

/// Raio de audição do `ambient_generic` pelas flags: 1 = em todo o mapa; 2 pequeno
/// (`ATTN_IDLE` 2.0), 4 médio (`ATTN_STATIC` 1.25), 8 grande (`ATTN_NORM` 0.8); nenhuma =
/// médio. O motor silencia a `1000 / atenuação` unidades (`sound_nominal_clip_dist`).
fn ambient_radius(flags: i32) -> Option<f32> {
    if flags & 1 != 0 {
        return None;
    }
    let attn = if flags & 2 != 0 {
        2.0
    } else if flags & 8 != 0 {
        0.8
    } else {
        1.25
    };
    Some(1000.0 / attn)
}

/// Monta a lista de entidades de cena e as rotas dos trens.
pub fn collect(bsp: &Bsp, parsed: &[Entity]) -> SceneEntities {
    // targetname -> primeira entidade
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    for (i, e) in parsed.iter().enumerate() {
        if let Some(t) = e.get("targetname").map(|t| t.trim()).filter(|t| !t.is_empty()) {
            by_name.entry(t).or_insert(i);
        }
    }

    let mut out = SceneEntities::default();
    for (index, e) in parsed.iter().enumerate() {
        if out.entities.len() >= MAX_ENTITIES {
            out.truncated = true;
            break;
        }
        let class = class_of(e);
        let base = || Base {
            entity: index,
            classname: class.to_string(),
            targetname: str_of(e, "targetname"),
            origin: center_of(bsp, e).unwrap_or([0.0; 3]),
        };
        let item = match class {
            "env_fog" => {
                let color = e
                    .get("rendercolor")
                    .or_else(|| e.get("fogcolor"))
                    .map(|r| rgb(&parse_nums(r)))
                    .unwrap_or([128, 128, 128]);
                Some(SceneEntity::EnvFog(EnvFog {
                    base: base(),
                    color,
                    start: f32_of(e, "startdist").unwrap_or(0.0),
                    end: f32_of(e, "enddist").unwrap_or(0.0),
                    density: f32_of(e, "density"),
                }))
            }
            "env_sky" => Some(SceneEntity::EnvSky(EnvSky {
                base: base(),
                skyname: str_of(e, "skyname"),
                angles: entities::angles_of(e),
            })),
            "light_environment" => {
                let (angles, pitch) = angles_with_pitch(e);
                let p = pitch.unwrap_or(angles[0]);
                let light = e.get("_light").map(|r| parse_nums(r)).unwrap_or_default();
                let diffuse = e.get("_diffuse_light").map(|r| parse_nums(r));
                Some(SceneEntity::LightEnvironment(LightEnvironment {
                    base: base(),
                    angles,
                    pitch,
                    direction: hammer_dir(p, angles[1]),
                    color: if light.len() >= 3 { rgb(&light) } else { [255, 255, 255] },
                    brightness: light.get(3).or(light.first().filter(|_| light.len() == 1)).copied().unwrap_or(200.0),
                    ambient_color: diffuse.as_ref().filter(|d| d.len() >= 3).map(|d| rgb(d)),
                    ambient_brightness: diffuse.as_ref().and_then(|d| d.get(3).copied()),
                    spread: f32_of(e, "SunSpreadAngle"),
                }))
            }
            "infodecal" | "decal" => Some(SceneEntity::Decal(Decal {
                base: base(),
                texture: str_of(e, "texture"),
                brush: class == "decal",
            })),
            "light" | "light_spot" => {
                let spot = class == "light_spot";
                let (angles, pitch) = angles_with_pitch(e);
                let l = e.get("_light").map(|r| parse_nums(r)).unwrap_or_default();
                let style = i32_of(e, "style").and_then(|s| u8::try_from(s).ok()).unwrap_or(0);
                let flags = i32_of(e, "spawnflags").unwrap_or(0);
                let target = str_of(e, "target");
                let origin = base().origin;
                let direction = spot.then(|| {
                    let aimed = target
                        .as_deref()
                        .and_then(|t| by_name.get(t))
                        .and_then(|&i| center_of(bsp, &parsed[i]))
                        .map(|p| {
                            let d = [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
                            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                            if len > 1e-3 { [d[0] / len, d[1] / len, d[2] / len] } else { [0.0, 0.0, -1.0] }
                        });
                    aimed.unwrap_or_else(|| hammer_dir(pitch.unwrap_or(angles[0]), angles[1]))
                });
                Some(SceneEntity::Light(LightSource {
                    base: base(),
                    spot,
                    color: if l.len() >= 3 { rgb(&l) } else { [255, 255, 255] },
                    brightness: l.get(3).copied().or(if l.len() == 1 { Some(l[0]) } else { None }).unwrap_or(300.0),
                    style,
                    pattern: str_of(e, "pattern").map(|p| p.to_ascii_lowercase()),
                    switchable: style >= 32 && e.get("targetname").is_some_and(|t| !t.trim().is_empty()),
                    start_off: flags & 1 != 0,
                    target,
                    angles,
                    direction,
                    cone: spot.then(|| f32_of(e, "_cone")).flatten(),
                    cone2: spot.then(|| f32_of(e, "_cone2")).flatten(),
                }))
            }
            "env_sprite" | "env_glow" => Some(SceneEntity::Sprite(SpriteEnt {
                base: base(),
                glow: class == "env_glow",
                model: str_of(e, "model"),
                scale: f32_of(e, "scale").filter(|s| *s > 0.0).unwrap_or(1.0),
                framerate: f32_of(e, "framerate").unwrap_or(10.0),
                render: render_props(e),
            })),
            "env_beam" | "env_lightning" => {
                let start_target = str_of(e, "LightningStart");
                let end_target = str_of(e, "LightningEnd");
                let resolve = |t: &Option<String>| -> Option<[f32; 3]> {
                    match t {
                        Some(name) => by_name.get(name.as_str()).and_then(|&i| center_of(bsp, &parsed[i])),
                        None => center_of(bsp, e),
                    }
                };
                Some(SceneEntity::Beam(BeamEnt {
                    base: base(),
                    texture: str_of(e, "texture"),
                    start: resolve(&start_target),
                    end: resolve(&end_target),
                    start_target,
                    end_target,
                    width: f32_of(e, "BoltWidth").unwrap_or(2.0),
                    noise: f32_of(e, "NoiseAmplitude").unwrap_or(0.0),
                    render: render_props(e),
                }))
            }
            "ambient_generic" => {
                let flags = i32_of(e, "spawnflags").unwrap_or(0);
                Some(SceneEntity::Ambient(AmbientSound {
                    base: base(),
                    sound: str_of(e, "message"),
                    volume: (f32_of(e, "health").unwrap_or(10.0) / 10.0).clamp(0.0, 1.0),
                    radius: ambient_radius(flags),
                    spawnflags: flags,
                    start_silent: flags & 16 != 0,
                }))
            }
            "path_corner" | "path_track" => Some(SceneEntity::PathCorner(PathCorner {
                base: base(),
                target: str_of(e, "target"),
                speed: f32_of(e, "speed").unwrap_or(0.0),
                wait: f32_of(e, "wait").unwrap_or(0.0),
                yaw_speed: f32_of(e, "yaw_speed").unwrap_or(0.0),
                spawnflags: i32_of(e, "spawnflags").unwrap_or(0),
            })),
            _ => mover_kind(class).map(|kind| SceneEntity::Mover(build_mover(bsp, e, kind, base()))),
        };
        if let Some(item) = item {
            *out.counts.entry(item.type_name().to_string()).or_insert(0) += 1;
            out.entities.push(item);
        }
    }

    out.routes = routes(bsp, parsed, &by_name);
    out
}

fn build_mover(bsp: &Bsp, e: &Entity, kind: MoverKind, base: Base) -> Mover {
    let (mins, maxs) = size_of(bsp, e);
    let size = [maxs[0] - mins[0], maxs[1] - mins[1], maxs[2] - mins[2]];
    let angles = entities::angles_of(e);
    let flags = i32_of(e, "spawnflags").unwrap_or(0);
    let movedir = movedir_of(angles);

    let (def_speed, def_wait, def_lip) = match kind {
        MoverKind::Door | MoverKind::DoorRotating => (100.0, 3.0, 0.0),
        MoverKind::Plat => (150.0, 0.0, 0.0),
        MoverKind::Train | MoverKind::TrackTrain => (100.0, 0.0, 0.0),
        MoverKind::Rotating => (100.0, 0.0, 0.0),
        MoverKind::Button => (40.0, 1.0, 4.0),
        MoverKind::RotButton => (40.0, 1.0, 0.0),
        MoverKind::Conveyor => (100.0, 0.0, 0.0),
    };
    let speed = f32_of(e, "speed").filter(|s| *s != 0.0).unwrap_or(def_speed);
    let lip = f32_of(e, "lip").unwrap_or(def_lip);
    let height = f32_of(e, "height").unwrap_or(0.0);
    let distance = f32_of(e, "distance").unwrap_or(90.0);

    // deslocamento do slide: |movedir · size| - lip. Porta tira 2 unidades de cada eixo
    // (`size - 2` em CBaseDoor::Spawn); botão não.
    let slide = |shrink: f32| -> [f32; 3] {
        let travel = (movedir[0] * (size[0] - shrink)).abs()
            + (movedir[1] * (size[1] - shrink)).abs()
            + (movedir[2] * (size[2] - shrink)).abs()
            - lip;
        [movedir[0] * travel, movedir[1] * travel, movedir[2] * travel]
    };
    let open_offset = match kind {
        MoverKind::Door => Some(slide(2.0)),
        MoverKind::Button => Some(slide(0.0)),
        MoverKind::Plat => {
            // plataforma desce `height` (ou o próprio tamanho - 8) da posição mapeada
            let drop = if height != 0.0 { height } else { size[2] - 8.0 };
            Some([0.0, 0.0, -drop])
        }
        _ => None,
    };

    // eixo de rotação no espaço de `angles` (pitch, yaw, roll): padrão yaw (0,1,0)
    let rot_delta = match kind {
        MoverKind::DoorRotating | MoverKind::RotButton => {
            // SF_DOOR_ROTATE_Z = 64 (roll), SF_DOOR_ROTATE_X = 128 (pitch); 2 = sentido inverso
            let axis = if flags & 64 != 0 {
                [0.0, 0.0, 1.0]
            } else if flags & 128 != 0 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let k = if flags & 2 != 0 { -distance } else { distance };
            Some([axis[0] * k, axis[1] * k, axis[2] * k])
        }
        MoverKind::Rotating => {
            // SF_BRUSH_ROTATE_Z_AXIS = 4, SF_BRUSH_ROTATE_X_AXIS = 8; 2 = para trás
            let axis = if flags & 4 != 0 {
                [0.0, 0.0, 1.0]
            } else if flags & 8 != 0 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let k = if flags & 2 != 0 { -speed } else { speed };
            Some([axis[0] * k, axis[1] * k, axis[2] * k])
        }
        _ => None,
    };

    let render = render_props(e);
    Mover {
        base,
        kind,
        model: brush_model_of(e).and_then(|n| u32::try_from(n).ok()),
        mins,
        maxs,
        angles,
        movedir,
        speed,
        lip,
        wait: f32_of(e, "wait").unwrap_or(def_wait),
        delay: f32_of(e, "delay").unwrap_or(0.0),
        height,
        distance,
        spawnflags: flags,
        target: str_of(e, "target"),
        start_open: matches!(kind, MoverKind::Door | MoverKind::DoorRotating) && flags & 1 != 0,
        open_offset,
        rot_delta,
        render: (!render.is_default()).then_some(render),
        scroll_speed: (kind == MoverKind::Conveyor).then(|| scroll_speed(e).map(|s| s.0)).flatten(),
    }
}

fn routes(bsp: &Bsp, parsed: &[Entity], by_name: &HashMap<&str, usize>) -> Vec<TrainRoute> {
    let is_corner = |e: &Entity| matches!(class_of(e), "path_corner" | "path_track");
    let mut out = Vec::new();
    for (train, e) in parsed.iter().enumerate() {
        if !matches!(class_of(e), "func_train" | "func_tracktrain") || out.len() >= 512 {
            continue;
        }
        let mut route = TrainRoute {
            train,
            targetname: str_of(e, "targetname"),
            points: Vec::new(),
            corners: Vec::new(),
            speeds: Vec::new(),
            waits: Vec::new(),
            looped: false,
            broken: false,
        };
        let mut next = str_of(e, "target");
        if next.is_none() {
            continue;
        }
        let mut seen: HashSet<usize> = HashSet::new();
        while let Some(name) = next.take() {
            let Some(&i) = by_name.get(name.as_str()) else {
                route.broken = true;
                break;
            };
            let c = &parsed[i];
            if !is_corner(c) {
                route.broken = true;
                break;
            }
            if !seen.insert(i) {
                route.looped = true;
                break;
            }
            if route.points.len() >= MAX_ROUTE {
                break;
            }
            route.points.push(center_of(bsp, c).unwrap_or([0.0; 3]));
            route.corners.push(i);
            route.speeds.push(f32_of(c, "speed").unwrap_or(0.0));
            route.waits.push(f32_of(c, "wait").unwrap_or(0.0));
            next = str_of(c, "target");
        }
        if !route.points.is_empty() || route.broken {
            out.push(route);
        }
    }
    out
}

/// Lê o mapa e devolve a cena (para o comando `map_scene`).
pub fn scene_of(path: &Path) -> Result<SceneEntities, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    drop(bytes);
    Ok(collect(&bsp, &entities::parse(&bsp.entities_raw)))
}
