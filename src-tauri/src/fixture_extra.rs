//! Mapa sintético dos recursos extras (hulls, zonas, render props, texturas animadas,
//! estilos de luz, cena), montado byte a byte. Independente de `fixture.rs`: aquele
//! descreve salas navegáveis; este é só um catálogo de coisas para os testes conferirem.
//!
//! Modelos:
//!
//! | modelo | entidade                | faces                                   |
//! |--------|-------------------------|-----------------------------------------|
//! | 0      | mundo                   | chão (2 estilos), `clip`, `sky`, `aaatrigger`, `+0fan`, `+1fan` |
//! | 1      | `trigger_multiple`      | 1 face `aaatrigger`                     |
//! | 2      | `func_buyzone`          | nenhuma (só clip) -> caixa do AABB      |
//! | 3      | `func_door`             | 1 face `concrete`; aditivo + strobe     |
//! | 4      | `func_conveyor`         | 1 face `scrollbelt`                     |
//! | 5      | `func_wall`             | 1 face `scrollbelt`, `rendercolor 0 1 0`|
//! | 6      | `func_train`            | nenhuma                                 |
//!
//! Hull 1 do mundo: cubo vazio de lado 160 (±80) cercado de sólido; hull 2: ±48; hull 3 ausente.

use crate::bsp::{GOLDSRC_VERSION, LUMP_COUNT};

type V3 = [f32; 3];

pub const TEXTURES: [&str; 9] = ["concrete", "aaatrigger", "clip", "+0fan", "+1fan", "+2fan", "+Afan", "scrollbelt", "sky"];
const TEX_SIZE: usize = 8;

pub struct FxQuad {
    pub origin: V3,
    pub u: V3,
    pub v: V3,
    pub w: f32,
    pub h: f32,
    pub tex: usize,
    pub styles: [u8; 4],
    /// bytes de luz já prontos (um bloco por estilo ativo, em sequência); vazio = sem lightmap
    pub light: Vec<u8>,
}

pub struct FxModel {
    pub first_face: i32,
    pub num_faces: i32,
    pub mins: V3,
    pub maxs: V3,
    pub hull_heads: [i32; 3],
}

pub struct Fx {
    pub quads: Vec<FxQuad>,
    pub models: Vec<FxModel>,
    pub planes: Vec<(V3, f32)>,
    pub clipnodes: Vec<(i32, [i16; 2])>,
    pub entities: String,
}

fn quad(origin: V3, u: V3, v: V3, w: f32, h: f32, tex: usize) -> FxQuad {
    FxQuad { origin, u, v, w, h, tex, styles: [0, 255, 255, 255], light: Vec::new() }
}

/// Cubo vazio de meia-aresta `half` no meio de sólido: 6 clipnodes encadeados.
/// Devolve a raiz. Frente = `d >= 0`.
fn cube_chain(planes: &mut Vec<(V3, f32)>, clips: &mut Vec<(i32, [i16; 2])>, half: f32) -> i32 {
    let root = clips.len() as i32;
    let p0 = planes.len() as i32;
    for axis in 0..3 {
        let mut n = [0.0; 3];
        n[axis] = 1.0;
        planes.push((n, half)); // x = +half
        planes.push((n, -half)); // x = -half
    }
    // x=+h: frente sólido; x=-h: trás sólido; idem y, z. Último: frente do z=-h é vazio.
    let c = |i: i32| (root + i) as i16;
    clips.push((p0, [-2, c(1)]));
    clips.push((p0 + 1, [c(2), -2]));
    clips.push((p0 + 2, [-2, c(3)]));
    clips.push((p0 + 3, [c(4), -2]));
    clips.push((p0 + 4, [-2, c(5)]));
    clips.push((p0 + 5, [-1, -2]));
    root
}

pub fn extra_fx() -> Fx {
    let mut quads: Vec<FxQuad> = Vec::new();
    let mut models: Vec<FxModel> = Vec::new();
    let mut planes = Vec::new();
    let mut clipnodes = Vec::new();

    // ---- modelo 0
    // chão 256x256 com dois estilos: bloco 0 vermelho (estilo 0), bloco 1 verde (estilo 1)
    let mut floor = quad([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 256.0, 256.0, 0);
    floor.styles = [0, 1, 255, 255];
    let texels = 17 * 17;
    floor.light = (0..texels).flat_map(|_| [100u8, 0, 0]).chain((0..texels).flat_map(|_| [0u8, 50, 0])).collect();
    quads.push(floor);
    quads.push(quad([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 256.0, 128.0, 2)); // clip
    quads.push(quad([0.0, 256.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 256.0, 128.0, 8)); // sky
    quads.push(quad([300.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 64.0, 64.0, 1)); // aaatrigger no mundo
    quads.push(quad([400.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 64.0, 64.0, 3)); // +0fan
    let mut q = quad([500.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 64.0, 64.0, 4); // +1fan
    q.styles = [5, 255, 255, 255];
    q.light = (0..5 * 5).flat_map(|_| [10u8, 20, 30]).collect();
    quads.push(q);
    let h1 = cube_chain(&mut planes, &mut clipnodes, 80.0);
    let h2 = cube_chain(&mut planes, &mut clipnodes, 48.0);
    models.push(FxModel { first_face: 0, num_faces: 6, mins: [-256.0, -256.0, -256.0], maxs: [600.0, 300.0, 256.0], hull_heads: [h1, h2, -1] });

    // ---- modelos 1..6
    let none = [-1, -1, -1];
    quads.push(quad([1000.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 32.0, 32.0, 1));
    models.push(FxModel { first_face: 6, num_faces: 1, mins: [1000.0, 0.0, 0.0], maxs: [1032.0, 32.0, 64.0], hull_heads: none });
    models.push(FxModel { first_face: 7, num_faces: 0, mins: [-64.0, -64.0, 0.0], maxs: [64.0, 64.0, 128.0], hull_heads: none });
    quads.push(quad([2000.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 64.0, 16.0, 0));
    models.push(FxModel { first_face: 7, num_faces: 1, mins: [2000.0, 0.0, 0.0], maxs: [2064.0, 16.0, 96.0], hull_heads: none });
    quads.push(quad([3000.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 64.0, 64.0, 7));
    models.push(FxModel { first_face: 8, num_faces: 1, mins: [3000.0, 0.0, 0.0], maxs: [3064.0, 64.0, 16.0], hull_heads: none });
    quads.push(quad([4000.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 64.0, 64.0, 7));
    models.push(FxModel { first_face: 9, num_faces: 1, mins: [4000.0, 0.0, 0.0], maxs: [4064.0, 64.0, 16.0], hull_heads: none });
    models.push(FxModel { first_face: 10, num_faces: 0, mins: [-16.0, -16.0, -16.0], maxs: [16.0, 16.0, 16.0], hull_heads: none });

    let entities = r#"{
"classname" "worldspawn"
"message" "Extras"
}
{
"classname" "info_player_start"
"origin" "32 32 36"
}
{
"classname" "env_fog"
"rendercolor" "10 20 30"
"startdist" "100"
"enddist" "900"
}
{
"classname" "light_environment"
"origin" "0 0 512"
"angles" "0 45 0"
"pitch" "-60"
"_light" "255 240 200 300"
"_diffuse_light" "100 110 120 80"
}
{
"classname" "infodecal"
"origin" "10 20 30"
"texture" "{shot1"
}
{
"classname" "light"
"origin" "128 128 200"
"_light" "255 200 100 400"
"style" "1"
}
{
"classname" "light"
"origin" "64 64 200"
"_light" "200 200 255 250"
"style" "33"
"targetname" "lampada"
"pattern" "az"
}
{
"classname" "light_spot"
"origin" "0 0 100"
"_light" "255 255 255 500"
"target" "alvo"
"_cone" "30"
"_cone2" "45"
}
{
"classname" "info_target"
"targetname" "alvo"
"origin" "0 0 0"
}
{
"classname" "env_sprite"
"origin" "50 60 70"
"model" "sprites/glow01.spr"
"scale" "0.5"
"rendermode" "5"
"renderamt" "200"
"rendercolor" "255 128 0"
}
{
"classname" "info_target"
"targetname" "ponta_a"
"origin" "0 0 50"
}
{
"classname" "info_target"
"targetname" "ponta_b"
"origin" "100 0 50"
}
{
"classname" "env_beam"
"origin" "0 0 0"
"LightningStart" "ponta_a"
"LightningEnd" "ponta_b"
"texture" "sprites/laserbeam.spr"
"BoltWidth" "8"
"NoiseAmplitude" "2"
"rendermode" "5"
"renderamt" "255"
}
{
"classname" "ambient_generic"
"origin" "128 0 64"
"message" "ambience/hum.wav"
"health" "5"
"spawnflags" "4"
}
{
"classname" "trigger_multiple"
"model" "*1"
"target" "alvo"
"targetname" "gatilho1"
}
{
"classname" "func_buyzone"
"model" "*2"
}
{
"classname" "func_door"
"model" "*3"
"targetname" "porta"
"angles" "0 90 0"
"speed" "50"
"lip" "8"
"rendermode" "5"
"renderamt" "128"
"renderfx" "10"
"rendercolor" "255 0 0"
}
{
"classname" "func_conveyor"
"model" "*4"
"speed" "32"
}
{
"classname" "func_wall"
"model" "*5"
"rendercolor" "1 1 0"
}
{
"classname" "func_train"
"model" "*6"
"targetname" "trem"
"target" "c1"
"speed" "80"
}
{
"classname" "path_corner"
"targetname" "c1"
"origin" "0 0 0"
"target" "c2"
"speed" "120"
}
{
"classname" "path_corner"
"targetname" "c2"
"origin" "100 0 0"
"target" "c3"
"wait" "2"
}
{
"classname" "path_corner"
"targetname" "c3"
"origin" "100 100 0"
"target" "c1"
}
"#
    .to_string();

    Fx { quads, models, planes, clipnodes, entities }
}

fn palette() -> Vec<u8> {
    (0..256usize).flat_map(|i| [i as u8, (255 - i) as u8, 128]).collect()
}

pub fn build(fx: &Fx) -> Vec<u8> {
    let mut lumps: Vec<Vec<u8>> = vec![Vec::new(); LUMP_COUNT];
    let mut ents = fx.entities.clone().into_bytes();
    ents.push(0);
    lumps[0] = ents;

    // texturas embutidas 8x8
    {
        let count = TEXTURES.len();
        let table = 4 + count * 4;
        let mip_len = 40 + TEX_SIZE * TEX_SIZE;
        let t = &mut lumps[2];
        t.extend_from_slice(&(count as u32).to_le_bytes());
        for i in 0..count {
            t.extend_from_slice(&((table + i * mip_len) as i32).to_le_bytes());
        }
        for (i, name) in TEXTURES.iter().enumerate() {
            let mut raw = [0u8; 16];
            raw[..name.len()].copy_from_slice(name.as_bytes());
            t.extend_from_slice(&raw);
            t.extend_from_slice(&(TEX_SIZE as u32).to_le_bytes());
            t.extend_from_slice(&(TEX_SIZE as u32).to_le_bytes());
            t.extend_from_slice(&40u32.to_le_bytes());
            t.extend_from_slice(&[0u8; 12]);
            t.extend(std::iter::repeat((i * 20) as u8).take(TEX_SIZE * TEX_SIZE));
        }
        t.extend_from_slice(&palette());
    }

    lumps[1] = fx
        .planes
        .iter()
        .flat_map(|(n, d)| {
            let mut b = Vec::new();
            for k in n {
                b.extend_from_slice(&k.to_le_bytes());
            }
            b.extend_from_slice(&d.to_le_bytes());
            b.extend_from_slice(&0i32.to_le_bytes());
            b
        })
        .collect();
    lumps[9] = fx
        .clipnodes
        .iter()
        .flat_map(|(p, c)| {
            let mut b = p.to_le_bytes().to_vec();
            b.extend_from_slice(&c[0].to_le_bytes());
            b.extend_from_slice(&c[1].to_le_bytes());
            b
        })
        .collect();

    // aresta 0 reservada
    lumps[12].extend_from_slice(&[0u8; 4]);
    let mut edge_count = 0i32;
    for (fi, q) in fx.quads.iter().enumerate() {
        let base = (lumps[3].len() / 12) as u16;
        let at = |a: f32, b: f32| -> V3 {
            [
                q.origin[0] + q.u[0] * a + q.v[0] * b,
                q.origin[1] + q.u[1] * a + q.v[1] * b,
                q.origin[2] + q.u[2] * a + q.v[2] * b,
            ]
        };
        for c in [at(0.0, 0.0), at(q.w, 0.0), at(q.w, q.h), at(0.0, q.h)] {
            for k in c {
                lumps[3].extend_from_slice(&k.to_le_bytes());
            }
        }
        for e in 0..4u16 {
            lumps[12].extend_from_slice(&(base + e).to_le_bytes());
            lumps[12].extend_from_slice(&(base + (e + 1) % 4).to_le_bytes());
        }
        let first_surfedge = (lumps[13].len() / 4) as i32;
        for e in 0..4 {
            lumps[13].extend_from_slice(&(edge_count + 1 + e).to_le_bytes());
        }
        edge_count += 4;

        let dotv = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        for k in [q.u[0], q.u[1], q.u[2], -dotv(q.u, q.origin), q.v[0], q.v[1], q.v[2], -dotv(q.v, q.origin)] {
            lumps[6].extend_from_slice(&k.to_le_bytes());
        }
        lumps[6].extend_from_slice(&(q.tex as u32).to_le_bytes());
        lumps[6].extend_from_slice(&0u32.to_le_bytes());

        let lightofs: i32 = if q.light.is_empty() {
            -1
        } else {
            let at = lumps[8].len() as i32;
            lumps[8].extend_from_slice(&q.light);
            at
        };
        let f = &mut lumps[7];
        f.extend_from_slice(&0u16.to_le_bytes());
        f.extend_from_slice(&0u16.to_le_bytes());
        f.extend_from_slice(&first_surfedge.to_le_bytes());
        f.extend_from_slice(&4u16.to_le_bytes());
        f.extend_from_slice(&(fi as u16).to_le_bytes());
        f.extend_from_slice(&q.styles);
        f.extend_from_slice(&lightofs.to_le_bytes());
    }

    for m in &fx.models {
        let b = &mut lumps[14];
        for k in m.mins.iter().chain(m.maxs.iter()) {
            b.extend_from_slice(&k.to_le_bytes());
        }
        b.extend_from_slice(&[0u8; 12]); // origin
        b.extend_from_slice(&0i32.to_le_bytes()); // headnode[0]
        for h in m.hull_heads {
            b.extend_from_slice(&h.to_le_bytes());
        }
        b.extend_from_slice(&0i32.to_le_bytes()); // visleafs
        b.extend_from_slice(&m.first_face.to_le_bytes());
        b.extend_from_slice(&m.num_faces.to_le_bytes());
    }

    let header = 4 + LUMP_COUNT * 8;
    let mut out = Vec::new();
    out.extend_from_slice(&GOLDSRC_VERSION.to_le_bytes());
    let mut offset = header;
    for l in &lumps {
        out.extend_from_slice(&(offset as i32).to_le_bytes());
        out.extend_from_slice(&(l.len() as i32).to_le_bytes());
        offset += l.len();
    }
    for l in &lumps {
        out.extend_from_slice(l);
    }
    out
}

pub fn extra_bsp() -> Vec<u8> {
    build(&extra_fx())
}
