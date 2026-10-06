//! Mapa sintético completo — salas, árvore BSP, PVS, lightmaps e texturas com pixel —
//! montado byte a byte para os testes de integração e para gerar as capturas de
//! tela do README sem distribuir nenhum mapa da Valve.
//!
//! Layout (vista de cima, X para a direita, Y para cima):
//!
//! ```text
//!  y=+256 ┌──────────┬──────────┬──────────┐
//!         │  sala A  │  sala B  │  sala C  │   portas em x=0 e x=512
//!  y=-256 └──────────┴──────────┴──────────┘
//!        x=-512      0         512        1024
//! ```
//!
//! A vê B, B vê A e C, C vê B: da sala A a sala C some do PVS.

use crate::bsp::{LUMP_COUNT, GOLDSRC_VERSION};

const LUMP_ENTITIES: usize = 0;
const LUMP_PLANES: usize = 1;
const LUMP_TEXTURES: usize = 2;
const LUMP_VERTEXES: usize = 3;
const LUMP_VISIBILITY: usize = 4;
const LUMP_NODES: usize = 5;
const LUMP_TEXINFO: usize = 6;
const LUMP_FACES: usize = 7;
const LUMP_LIGHTING: usize = 8;
const LUMP_LEAVES: usize = 10;
const LUMP_MARKSURFACES: usize = 11;
const LUMP_EDGES: usize = 12;
const LUMP_SURFEDGES: usize = 13;
const LUMP_MODELS: usize = 14;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-6);
    scale(a, 1.0 / l)
}

/// Retângulo no espaço: `origin + u*[0,w] + v*[0,h]`, normal = u × v.
struct Quad {
    origin: V3,
    u: V3,
    v: V3,
    w: f32,
    h: f32,
    tex: usize,
    /// folhas (1=A, 2=B, 3=C) que enxergam esta face
    leaves: &'static [usize],
    /// 255 = sem lightmap (como o céu)
    lit: bool,
}

pub const TEXTURES: [&str; 5] = ["concrete", "brick", "crate", "plaster", "{grate"];
const TEX_SIZE: usize = 64;

/// Paleta de 256 cores: rampas de cinza, vermelho-tijolo, madeira e azul-gelo.
/// O índice 255 é azul puro — só vira buraco em textura `{`.
fn palette() -> Vec<u8> {
    let mut p = vec![0u8; 768];
    for i in 0..256usize {
        let (r, g, b) = match i {
            0..=63 => {
                let v = 70 + i * 2;
                (v, v, v + 4)
            }
            64..=127 => {
                let k = i - 64;
                (110 + k * 2, 50 + k, 38 + k / 2)
            }
            128..=191 => {
                let k = i - 128;
                (96 + k, 66 + k, 34 + k / 2)
            }
            192..=254 => {
                let k = i - 192;
                (200 + k / 2, 205 + k / 2, 210 + k / 3)
            }
            _ => (0, 0, 255),
        };
        p[i * 3] = r.min(255) as u8;
        p[i * 3 + 1] = g.min(255) as u8;
        p[i * 3 + 2] = b.min(255) as u8;
    }
    p
}

fn texture_pixels(kind: usize) -> Vec<u8> {
    let mut px = vec![0u8; TEX_SIZE * TEX_SIZE];
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let noise = ((x * 73 + y * 151 + x * y * 7) % 11) as u8;
            px[y * TEX_SIZE + x] = match kind {
                // concreto: cinza com granulação
                0 => 20 + noise * 2 + if (x / 32 + y / 32) % 2 == 0 { 8 } else { 0 },
                // tijolo: fileiras de 16px, argamassa clara entre elas
                1 => {
                    let row = y / 16;
                    let off = if row % 2 == 0 { 0 } else { 16 };
                    if y % 16 == 0 || (x + off) % 32 == 0 {
                        200 + noise
                    } else {
                        64 + noise * 3 + (row as u8 * 3)
                    }
                }
                // caixote: tábuas de madeira com cantoneira
                2 => {
                    if x < 3 || x >= 61 || y < 3 || y >= 61 || (x as i32 - y as i32).abs() < 2 {
                        140 + noise
                    } else {
                        128 + (y % 8) as u8 * 3 + noise
                    }
                }
                // reboco claro
                3 => 192 + noise * 2 + ((x + y) % 5) as u8,
                // grade vazada: barras de metal, resto é buraco (índice 255)
                _ => {
                    if x % 16 < 3 || y % 16 < 3 {
                        8 + noise
                    } else {
                        255
                    }
                }
            };
        }
    }
    px
}

fn textures_lump() -> Vec<u8> {
    let count = TEXTURES.len();
    let table = 4 + count * 4;
    let mip_len = 40 + TEX_SIZE * TEX_SIZE;
    let mut out = Vec::new();
    out.extend_from_slice(&(count as u32).to_le_bytes());
    for i in 0..count {
        out.extend_from_slice(&((table + i * mip_len) as i32).to_le_bytes());
    }
    for (i, name) in TEXTURES.iter().enumerate() {
        let mut raw = [0u8; 16];
        raw[..name.len()].copy_from_slice(name.as_bytes());
        out.extend_from_slice(&raw);
        out.extend_from_slice(&(TEX_SIZE as u32).to_le_bytes());
        out.extend_from_slice(&(TEX_SIZE as u32).to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes()); // mip0 logo depois do cabeçalho
        out.extend_from_slice(&[0u8; 12]);
        out.extend_from_slice(&texture_pixels(i));
    }
    out.extend_from_slice(&[0u8; 2]); // preenchimento do formato
    out.truncate(out.len() - 2);
    out.extend_from_slice(&palette());
    out
}

fn room_quads() -> Vec<Quad> {
    let (y0, y1, z0, z1) = (-256.0f32, 256.0f32, 0.0f32, 256.0f32);
    let mut q: Vec<Quad> = Vec::new();
    // chão e teto de cada sala: (x0, x1, folha)
    for (x0, x1, leaf, floor_tex) in [(-512.0f32, 0.0f32, 1usize, 0usize), (0.0, 512.0, 2, 0), (512.0, 1024.0, 3, 0)] {
        let leaves: &'static [usize] = match leaf {
            1 => &[1],
            2 => &[2],
            _ => &[3],
        };
        // chão: normal +Z  (u=+X, v=+Y)
        q.push(Quad { origin: [x0, y0, z0], u: [1.0, 0.0, 0.0], v: [0.0, 1.0, 0.0], w: x1 - x0, h: y1 - y0, tex: floor_tex, leaves, lit: true });
        // teto: normal -Z (u=+Y, v=+X => u×v = -Z)
        q.push(Quad { origin: [x0, y0, z1], u: [0.0, 1.0, 0.0], v: [1.0, 0.0, 0.0], w: y1 - y0, h: x1 - x0, tex: 3, leaves, lit: true });
        // parede sul (y=-256), normal +Y: u=+Z? z×x=+Y => u=Z, v=X
        q.push(Quad { origin: [x0, y0, z0], u: [0.0, 0.0, 1.0], v: [1.0, 0.0, 0.0], w: z1 - z0, h: x1 - x0, tex: 1, leaves, lit: true });
        // parede norte (y=+256), normal -Y: u=X, v=Z (x×z = -Y)
        q.push(Quad { origin: [x0, y1, z0], u: [1.0, 0.0, 0.0], v: [0.0, 0.0, 1.0], w: x1 - x0, h: z1 - z0, tex: 1, leaves, lit: true });
    }
    // parede do fundo da sala A (x=-512), normal +X: u=Y, v=Z (y×z = +X)
    q.push(Quad { origin: [-512.0, y0, z0], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0], w: y1 - y0, h: z1 - z0, tex: 1, leaves: &[1], lit: true });
    // parede do fundo da sala C (x=1024), normal -X: u=Z, v=Y (z×y = -X)
    q.push(Quad { origin: [1024.0, y0, z0], u: [0.0, 0.0, 1.0], v: [0.0, 1.0, 0.0], w: z1 - z0, h: y1 - y0, tex: 1, leaves: &[3], lit: true });
    // paredes com porta (vão de 128 de largura, 160 de altura) em x=0 (A|B) e x=512 (B|C)
    for (x, leaves) in [(0.0f32, &[1usize, 2][..]), (512.0, &[2, 3][..])] {
        // dois pedaços laterais (normal +X: u=Y, v=Z)
        q.push(Quad { origin: [x, y0, z0], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0], w: 192.0, h: z1 - z0, tex: 1, leaves, lit: true });
        q.push(Quad { origin: [x, 64.0, z0], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0], w: y1 - 64.0, h: z1 - z0, tex: 1, leaves, lit: true });
        // verga acima do vão
        q.push(Quad { origin: [x, -64.0, 160.0], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0], w: 128.0, h: z1 - 160.0, tex: 1, leaves, lit: true });
        // grade vazada dentro do vão da segunda porta
        if x > 100.0 {
            q.push(Quad { origin: [x + 4.0, -64.0, z0], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0], w: 128.0, h: 160.0, tex: 4, leaves, lit: false });
        }
    }
    // caixote no meio da sala B: tampo + 4 lados (cubo de 96)
    let (cx, cy, s) = (192.0f32, -48.0f32, 96.0f32);
    q.push(Quad { origin: [cx, cy, s], u: [1.0, 0.0, 0.0], v: [0.0, 1.0, 0.0], w: s, h: s, tex: 2, leaves: &[2], lit: true });
    q.push(Quad { origin: [cx, cy, 0.0], u: [0.0, 0.0, 1.0], v: [1.0, 0.0, 0.0], w: s, h: s, tex: 2, leaves: &[2], lit: true });
    q.push(Quad { origin: [cx, cy + s, 0.0], u: [1.0, 0.0, 0.0], v: [0.0, 0.0, 1.0], w: s, h: s, tex: 2, leaves: &[2], lit: true });
    q.push(Quad { origin: [cx, cy, 0.0], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0], w: s, h: s, tex: 2, leaves: &[2], lit: true });
    q.push(Quad { origin: [cx + s, cy, 0.0], u: [0.0, 0.0, 1.0], v: [0.0, 1.0, 0.0], w: s, h: s, tex: 2, leaves: &[2], lit: true });
    q
}

/// luz pontual: posição, cor (0..1) e raio
const LIGHTS: [(usize, V3, V3, f32); 4] = [
    (1, [-256.0, 0.0, 200.0], [1.0, 0.82, 0.55], 520.0),
    (2, [256.0, 0.0, 210.0], [0.75, 0.88, 1.0], 560.0),
    (2, [64.0, 120.0, 90.0], [1.0, 0.6, 0.4], 260.0),
    (3, [768.0, 0.0, 200.0], [0.7, 1.0, 0.7], 520.0),
];

fn lightmap_of(quad: &Quad) -> (usize, usize, Vec<u8>) {
    let lw = (quad.w / 16.0).ceil() as usize + 1;
    let lh = (quad.h / 16.0).ceil() as usize + 1;
    let n = norm(cross(quad.u, quad.v));
    let mut out = Vec::with_capacity(lw * lh * 3);
    for j in 0..lh {
        for i in 0..lw {
            let p = add(quad.origin, add(scale(quad.u, (i * 16) as f32), scale(quad.v, (j * 16) as f32)));
            let mut rgb = [0.16f32, 0.17f32, 0.2f32]; // ambiente
            for (leaf, pos, color, radius) in LIGHTS {
                if !quad.leaves.contains(&leaf) {
                    continue;
                }
                let to = sub(pos, p);
                let dist = dot(to, to).sqrt();
                if dist > radius {
                    continue;
                }
                let lambert = dot(n, scale(to, 1.0 / dist.max(1e-3))).max(0.0);
                let fall = (1.0 - dist / radius).powi(2);
                for c in 0..3 {
                    rgb[c] += color[c] * lambert * fall * 1.1;
                }
            }
            for c in rgb {
                out.push((c.clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
    }
    (lw, lh, out)
}

#[derive(Clone, Copy)]
pub struct Options {
    /// grava o lump de visibilidade (sem ele o mapa "vazou")
    pub vis: bool,
    pub lighting: bool,
    /// põe um spawn dentro da parede da sala A
    pub spawn_in_wall: bool,
    pub version: i32,
}

impl Default for Options {
    fn default() -> Self {
        Self { vis: true, lighting: true, spawn_in_wall: false, version: GOLDSRC_VERSION }
    }
}

pub fn entities_text(opts: Options) -> String {
    let mut t = String::new();
    t.push_str("{\n\"classname\" \"worldspawn\"\n\"message\" \"Museu de Testes\"\n\"skyname\" \"desert\"\n\"wad\" \"\\half-life\\valve\\halflife.wad\"\n}\n");
    for i in 0..8 {
        let y = -200 + i * 56;
        t.push_str(&format!("{{\n\"classname\" \"info_player_start\"\n\"origin\" \"{} {} 36\"\n\"angles\" \"0 0 0\"\n}}\n", -420 + (i % 2) * 70, y));
    }
    for i in 0..8 {
        let y = -200 + i * 56;
        t.push_str(&format!("{{\n\"classname\" \"info_player_deathmatch\"\n\"origin\" \"{} {} 36\"\n\"angles\" \"0 180 0\"\n}}\n", 940 - (i % 2) * 70, y));
    }
    if opts.spawn_in_wall {
        t.push_str("{\n\"classname\" \"info_player_start\"\n\"origin\" \"-300 400 36\"\n}\n");
    }
    t.push_str("{\n\"classname\" \"func_buyzone\"\n\"targetname\" \"loja_ct\"\n\"origin\" \"-256 0 64\"\n}\n");
    t.push_str("{\n\"classname\" \"func_bomb_target\"\n\"targetname\" \"alvo_a\"\n\"origin\" \"768 0 64\"\n}\n");
    for (i, (x, y)) in [(-256, 0), (256, 0), (64, 120), (768, 0)].iter().enumerate() {
        t.push_str(&format!("{{\n\"classname\" \"light\"\n\"origin\" \"{x} {y} 200\"\n\"_light\" \"255 220 180 300\"\n\"targetname\" \"luz{i}\"\n}}\n"));
    }
    t.push_str("{\n\"classname\" \"ambient_generic\"\n\"message\" \"ambience/hum.wav\"\n\"origin\" \"256 0 128\"\n}\n");
    t
}

pub fn museum_bsp(opts: Options) -> Vec<u8> {
    let quads = room_quads();
    let mut lumps: Vec<Vec<u8>> = vec![Vec::new(); LUMP_COUNT];

    let mut ents = entities_text(opts).into_bytes();
    ents.push(0);
    lumps[LUMP_ENTITIES] = ents;

    let mut edge_count = 0i32;
    let mut leaf_marks: [Vec<u16>; 4] = Default::default();
    for (fi, q) in quads.iter().enumerate() {
        let base = (lumps[LUMP_VERTEXES].len() / 12) as u16;
        let corners = [
            q.origin,
            add(q.origin, scale(q.u, q.w)),
            add(add(q.origin, scale(q.u, q.w)), scale(q.v, q.h)),
            add(q.origin, scale(q.v, q.h)),
        ];
        for c in corners {
            for k in c {
                lumps[LUMP_VERTEXES].extend_from_slice(&k.to_le_bytes());
            }
        }
        // aresta 0 é reservada (surfedge 0 não tem sinal): começamos em 1
        for e in 0..4u16 {
            lumps[LUMP_EDGES].extend_from_slice(&(base + e).to_le_bytes());
            lumps[LUMP_EDGES].extend_from_slice(&(base + (e + 1) % 4).to_le_bytes());
        }
        let first_surfedge = (lumps[LUMP_SURFEDGES].len() / 4) as i32;
        for e in 0..4 {
            lumps[LUMP_SURFEDGES].extend_from_slice(&(edge_count + 1 + e).to_le_bytes());
        }
        edge_count += 4;

        // texinfo: s = u·p + shift, t = v·p + shift (UV sai de graça 1 unidade = 1 pixel)
        let s_shift = -dot(q.u, q.origin);
        let t_shift = -dot(q.v, q.origin);
        for k in [q.u[0], q.u[1], q.u[2], s_shift, q.v[0], q.v[1], q.v[2], t_shift] {
            lumps[LUMP_TEXINFO].extend_from_slice(&k.to_le_bytes());
        }
        lumps[LUMP_TEXINFO].extend_from_slice(&(q.tex as u32).to_le_bytes());
        lumps[LUMP_TEXINFO].extend_from_slice(&0u32.to_le_bytes());

        let lightofs: i32 = if opts.lighting && q.lit {
            let (_, _, data) = lightmap_of(q);
            let at = lumps[LUMP_LIGHTING].len() as i32;
            lumps[LUMP_LIGHTING].extend_from_slice(&data);
            at
        } else {
            -1
        };
        let f = &mut lumps[LUMP_FACES];
        f.extend_from_slice(&0u16.to_le_bytes()); // planenum (não usado pelo parser)
        f.extend_from_slice(&0u16.to_le_bytes());
        f.extend_from_slice(&first_surfedge.to_le_bytes());
        f.extend_from_slice(&4u16.to_le_bytes());
        f.extend_from_slice(&(fi as u16).to_le_bytes()); // texinfo = índice da face
        f.extend_from_slice(&[0, 255, 255, 255]);
        f.extend_from_slice(&lightofs.to_le_bytes());
        for &leaf in q.leaves {
            leaf_marks[leaf].push(fi as u16);
        }
    }
    // aresta 0 reservada (par degenerado) para que surfedge positivo comece em 1
    let mut edges = vec![0u8; 4];
    edges.extend_from_slice(&lumps[LUMP_EDGES]);
    lumps[LUMP_EDGES] = edges;

    lumps[LUMP_TEXTURES] = textures_lump();

    // planos: normais +Y +Y +Z +Z +X +X +X +X  em (256, -256, 256, 0, 1024, -512, 0, 512)
    let planes: [(V3, f32); 8] = [
        ([0.0, 1.0, 0.0], 256.0),
        ([0.0, 1.0, 0.0], -256.0),
        ([0.0, 0.0, 1.0], 256.0),
        ([0.0, 0.0, 1.0], 0.0),
        ([1.0, 0.0, 0.0], 1024.0),
        ([1.0, 0.0, 0.0], -512.0),
        ([1.0, 0.0, 0.0], 0.0),
        ([1.0, 0.0, 0.0], 512.0),
    ];
    for (n, d) in planes {
        for k in n {
            lumps[LUMP_PLANES].extend_from_slice(&k.to_le_bytes());
        }
        lumps[LUMP_PLANES].extend_from_slice(&d.to_le_bytes());
        lumps[LUMP_PLANES].extend_from_slice(&0i32.to_le_bytes());
    }
    // folhas: 0=sólido(fora), 1=A, 2=B, 3=C. filho de folha L = -1 - L.
    let (out, a, b, c) = (-1i16, -2i16, -3i16, -4i16);
    // [plano, frente(d>=0), trás(d<0)]
    let nodes: [(i32, i16, i16); 8] = [
        (0, out, 1),
        (1, 2, out),
        (2, out, 3),
        (3, 4, out),
        (4, out, 5),
        (5, 6, out),
        (6, 7, a),
        (7, c, b),
    ];
    for (plane, front, back) in nodes {
        let n = &mut lumps[LUMP_NODES];
        n.extend_from_slice(&plane.to_le_bytes());
        n.extend_from_slice(&front.to_le_bytes());
        n.extend_from_slice(&back.to_le_bytes());
        n.extend_from_slice(&[0u8; 12]);
        n.extend_from_slice(&[0u8; 4]);
    }
    // marksurfaces + folhas
    let mut first_marks = [0u16; 4];
    for leaf in 1..4 {
        first_marks[leaf] = (lumps[LUMP_MARKSURFACES].len() / 2) as u16;
        for m in &leaf_marks[leaf] {
            lumps[LUMP_MARKSURFACES].extend_from_slice(&m.to_le_bytes());
        }
    }
    for leaf in 0..4usize {
        let l = &mut lumps[LUMP_LEAVES];
        let contents: i32 = if leaf == 0 { -2 } else { -1 };
        let visofs: i32 = if leaf == 0 || !opts.vis { -1 } else { (leaf - 1) as i32 };
        l.extend_from_slice(&contents.to_le_bytes());
        l.extend_from_slice(&visofs.to_le_bytes());
        l.extend_from_slice(&[0u8; 12]);
        l.extend_from_slice(&first_marks[leaf].to_le_bytes());
        l.extend_from_slice(&(leaf_marks[leaf].len() as u16).to_le_bytes());
        l.extend_from_slice(&[0u8; 4]);
    }
    if opts.vis {
        // linhas de 1 byte: bit i = folha i+1
        lumps[LUMP_VISIBILITY] = vec![0b011, 0b111, 0b110];
    }

    // modelo 0
    {
        let m = &mut lumps[LUMP_MODELS];
        for k in [-512.0f32, -256.0, 0.0, 1024.0, 256.0, 256.0] {
            m.extend_from_slice(&k.to_le_bytes());
        }
        m.extend_from_slice(&[0u8; 12]); // origin
        m.extend_from_slice(&0i32.to_le_bytes()); // headnode[0]
        m.extend_from_slice(&[0u8; 12]);
        m.extend_from_slice(&3i32.to_le_bytes()); // visleafs
        m.extend_from_slice(&0i32.to_le_bytes());
        m.extend_from_slice(&(quads.len() as i32).to_le_bytes());
    }

    let header = 4 + LUMP_COUNT * 8;
    let mut out = Vec::new();
    out.extend_from_slice(&opts.version.to_le_bytes());
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
