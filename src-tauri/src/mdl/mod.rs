//! Parser de modelos `.mdl` do GoldSrc (Half-Life/CS 1.6, studiomodel versão 10).
//!
//! Formato irmão do BSP (herda a mesma convenção de textura indexada com paleta
//! de 256 cores), mas é um arquivo/propósito diferente — vive fora de `bsp::`,
//! reusando só o `Cursor` little-endian de `bsp::reader` em vez de duplicá-lo.
//!
//! Esta entrega decodifica só a **pose de repouso** (bind pose): a hierarquia de
//! bones é composta a partir de `mstudiobone_t.value` (posição+rotação fixas do
//! arquivo), sem aplicar nenhuma sequência de animação — `mstudioanim_t` não é
//! lido. O nome de cada sequência ainda é exposto (metadado real, útil pra saber
//! que o modelo tem "idle"/"walk"/etc.), mas escolher uma sequência não muda a
//! geometria desenhada nesta versão.
//!
//! Referências: `studiohdr_t`/`mstudiobone_t`/`mstudiotexture_t`/`mstudiomodel_t`/
//! `mstudiomesh_t`/`mstudioseqdesc_t` de `studio.h` do SDK do Half-Life (layout
//! público, replicado em várias engines/loaders GoldSrc-compatíveis). A
//! composição exata da rotação por bone (ordem dos eixos) não pôde ser
//! confirmada contra um `.mdl` real neste ambiente — ver `rotation_matrix` e o
//! aviso em `README.md`/PR.

use crate::bsp::palette;
use crate::bsp::reader::Cursor;
use std::fmt;

pub const IDENT: &[u8; 4] = b"IDST";
pub const VERSION: i32 = 10;

const HEADER_NAME_LEN: usize = 64;
const BONE_SIZE: usize = 112; // name[32] + parent(4) + flags(4) + bonecontroller[6](24) + value[6](24) + scale[6](24)
const TEXTURE_SIZE: usize = 80; // name[64] + flags(4) + width(4) + height(4) + index(4)
const BODYPART_SIZE: usize = 76; // name[64] + nummodels(4) + base(4) + modelindex(4)
const MODEL_SIZE: usize = 112; // ver `read_bodyparts`
const MESH_SIZE: usize = 20; // numtris(4) + triindex(4) + skinref(4) + numnorms(4) + normindex(4)
const SEQDESC_SIZE: usize = 176;
const STUDIO_NF_MASKED: i32 = 0x0040;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdlError {
    TooSmall { need: usize, have: usize },
    BadMagic,
    BadVersion(i32),
    Malformed(String),
}

impl fmt::Display for MdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MdlError::TooSmall { need, have } => {
                write!(f, "arquivo .mdl pequeno demais: precisa de {need} bytes, tem {have}")
            }
            MdlError::BadMagic => write!(f, "não começa com \"IDST\" — não é .mdl do GoldSrc"),
            MdlError::BadVersion(v) => {
                write!(f, "versão {v} não é studiomodel v10 (GoldSrc retail)")
            }
            MdlError::Malformed(msg) => write!(f, "arquivo .mdl corrompido: {msg}"),
        }
    }
}

impl std::error::Error for MdlError {}

pub type Result<T> = std::result::Result<T, MdlError>;

fn cursor_err(e: crate::bsp::reader::BspError) -> MdlError {
    MdlError::Malformed(e.to_string())
}

/// Textura decodificada (skin), pronta pro frontend — mesmo formato de
/// `bsp::TextureImage`.
#[derive(Debug, Clone)]
pub struct MdlTexture {
    pub name: String,
    pub png: String,
    pub width: u32,
    pub height: u32,
}

/// Malha do modelo já triangulada e em espaço de mundo (pose de repouso) —
/// mesma convenção não-indexada de `catalog::MeshDetail`: 9 floats de posição e
/// 6 de UV por triângulo, um `texindex` por triângulo.
#[derive(Debug, Clone, Default)]
pub struct MdlModel {
    pub positions: Vec<f32>,
    pub uvs: Vec<f32>,
    pub texindex: Vec<u32>,
    pub textures: Vec<MdlTexture>,
    /// nomes das sequências do modelo — só metadado, não muda a geometria
    /// (ver nota do módulo: animação não é decodificada nesta versão).
    pub sequences: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct Transform {
    rot: [[f32; 3]; 3],
    pos: [f32; 3],
}

impl Transform {
    fn identity() -> Self {
        Self { rot: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], pos: [0.0, 0.0, 0.0] }
    }

    fn compose(parent: &Transform, local: &Transform) -> Transform {
        Transform { rot: mat_mul(&parent.rot, &local.rot), pos: add(mat_vec(&parent.rot, local.pos), parent.pos) }
    }

    fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        add(mat_vec(&self.rot, v), self.pos)
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn mat_vec(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn mat_mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

/// Rotação a partir de radianos por eixo local (`bone.value[3..6]`), ordem
/// Rz · Ry · Rx. **Não confirmada contra um `.mdl` real** (a documentação
/// oficial da Valve não pôde ser buscada neste ambiente) — se um modelo com
/// mais de um bone sair torto, é o primeiro lugar a revisar.
fn rotation_matrix(rx: f32, ry: f32, rz: f32) -> [[f32; 3]; 3] {
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz],
        [-sy, sx * cy, cx * cy],
    ]
}

/// Transform de uma instância no mapa (`origin`/`angles` da entidade). Só o
/// `yaw` (rotação em torno de Z, o "para cima" do GoldSrc antes da conversão
/// pro Y-up do Three.js) é aplicado — é o eixo que praticamente todo prop
/// estático usa; pitch/roll ficam de fora nesta versão.
pub fn entity_transform(origin: [f32; 3], angles: [f32; 3]) -> impl Fn([f32; 3]) -> [f32; 3] {
    let yaw = angles[1].to_radians();
    let (sy, cy) = yaw.sin_cos();
    let rot = [[cy, -sy, 0.0], [sy, cy, 0.0], [0.0, 0.0, 1.0]];
    move |v: [f32; 3]| add(mat_vec(&rot, v), origin)
}

struct Bone {
    parent: i32,
    local: Transform,
}

/// Decodifica um `.mdl` v10 completo: bones (pose de repouso), texturas (skins)
/// e a malha de todos os bodyparts/meshes, já em espaço de mundo do modelo.
pub fn parse(data: &[u8]) -> Result<MdlModel> {
    if data.len() < HEADER_NAME_LEN {
        return Err(MdlError::TooSmall { need: HEADER_NAME_LEN, have: data.len() });
    }
    if &data[0..4] != IDENT {
        return Err(MdlError::BadMagic);
    }
    let mut cur = Cursor::new(data);
    cur.skip(4).map_err(cursor_err)?; // ident, já checado acima
    let version = cur.i32().map_err(cursor_err)?;
    if version != VERSION {
        return Err(MdlError::BadVersion(version));
    }
    cur.skip(HEADER_NAME_LEN).map_err(cursor_err)?; // name[64]
    cur.skip(4).map_err(cursor_err)?; // length
    cur.skip(5 * 3 * 4).map_err(cursor_err)?; // eyeposition, min, max, bbmin, bbmax (5 vec3)
    cur.skip(4).map_err(cursor_err)?; // flags

    let num_bones = cur.i32().map_err(cursor_err)? as usize;
    let bone_index = cur.i32().map_err(cursor_err)? as usize;
    cur.skip(4 * 2).map_err(cursor_err)?; // numbonecontrollers, bonecontrollerindex
    cur.skip(4 * 2).map_err(cursor_err)?; // numhitboxes, hitboxindex
    let num_seq = cur.i32().map_err(cursor_err)? as usize;
    let seq_index = cur.i32().map_err(cursor_err)? as usize;
    cur.skip(4 * 2).map_err(cursor_err)?; // numseqgroups, seqgroupindex

    let num_textures = cur.i32().map_err(cursor_err)? as usize;
    let texture_index = cur.i32().map_err(cursor_err)? as usize;
    cur.skip(4).map_err(cursor_err)?; // texturedataindex — informativo, cada mstudiotexture_t já traz seu offset absoluto
    cur.skip(4 * 3).map_err(cursor_err)?; // numskinref, numskinfamilies, skinindex

    let num_bodyparts = cur.i32().map_err(cursor_err)? as usize;
    let bodypart_index = cur.i32().map_err(cursor_err)? as usize;

    let bones = read_bones(data, bone_index, num_bones)?;
    let world_bones = bone_world_transforms(&bones)?;
    let textures = read_textures(data, texture_index, num_textures)?;
    let sequences = read_sequence_names(data, seq_index, num_seq)?;

    let mut model = MdlModel { textures, sequences, ..Default::default() };
    read_bodyparts(data, bodypart_index, num_bodyparts, &world_bones, &mut model)?;
    Ok(model)
}

fn slice_at<'a>(data: &'a [u8], offset: usize, len: usize) -> Result<&'a [u8]> {
    data.get(offset..offset.checked_add(len).ok_or_else(|| {
        MdlError::Malformed(format!("offset {offset} + {len} estoura usize"))
    })?)
    .ok_or_else(|| MdlError::Malformed(format!("offset {offset}..{} fora do arquivo ({} bytes)", offset + len, data.len())))
}

fn read_bones(data: &[u8], index: usize, count: usize) -> Result<Vec<Bone>> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let entry = slice_at(data, index + i * BONE_SIZE, BONE_SIZE)?;
        let mut cur = Cursor::new(entry);
        cur.skip(32).map_err(cursor_err)?; // name — não precisa pra pose
        let parent = cur.i32().map_err(cursor_err)?;
        cur.skip(4).map_err(cursor_err)?; // flags
        cur.skip(4 * 6).map_err(cursor_err)?; // bonecontroller[6]
        let value = {
            let mut v = [0.0f32; 6];
            for slot in &mut v {
                *slot = cur.f32().map_err(cursor_err)?;
            }
            v
        };
        // scale[6] não é lido — não afeta a pose de repouso.
        let local = Transform {
            rot: rotation_matrix(value[3], value[4], value[5]),
            pos: [value[0], value[1], value[2]],
        };
        out.push(Bone { parent, local });
    }
    Ok(out)
}

/// Assume que todo bone vem depois do pai na lista (convenção do formato:
/// `parent < próprio índice`, exceto o bone raiz com `parent == -1`).
fn bone_world_transforms(bones: &[Bone]) -> Result<Vec<Transform>> {
    let mut world = Vec::with_capacity(bones.len());
    for (i, bone) in bones.iter().enumerate() {
        let t = if bone.parent < 0 {
            bone.local
        } else {
            let parent = usize::try_from(bone.parent)
                .ok()
                .and_then(|p| world.get(p).copied())
                .ok_or_else(|| MdlError::Malformed(format!("bone {i} aponta pra um pai inválido")))?;
            Transform::compose(&parent, &bone.local)
        };
        world.push(t);
    }
    Ok(world)
}

fn read_textures(data: &[u8], index: usize, count: usize) -> Result<Vec<MdlTexture>> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let entry = slice_at(data, index + i * TEXTURE_SIZE, TEXTURE_SIZE)?;
        let mut cur = Cursor::new(entry);
        let name = cur.fixed_str(64).map_err(cursor_err)?;
        let flags = cur.i32().map_err(cursor_err)?;
        let width = cur.u32().map_err(cursor_err)?;
        let height = cur.u32().map_err(cursor_err)?;
        let pixel_offset = cur.u32().map_err(cursor_err)? as usize;

        let w = width as usize;
        let h = height as usize;
        let need = w.checked_mul(h).ok_or_else(|| MdlError::Malformed("textura absurdamente grande".into()))?;
        let pixels = slice_at(data, pixel_offset, need)?;
        let palette_bytes = slice_at(data, pixel_offset + need, 768)?;
        let transparent = flags & STUDIO_NF_MASKED != 0;
        let rgba = palette::decode_indexed(pixels, palette_bytes, transparent)
            .ok_or_else(|| MdlError::Malformed(format!("paleta inválida na skin {name}")))?;
        let png = crate::bsp::rgba_png(w, h, &rgba)
            .ok_or_else(|| MdlError::Malformed(format!("falha ao gerar PNG da skin {name}")))?;
        out.push(MdlTexture { name, png, width, height });
    }
    Ok(out)
}

fn read_sequence_names(data: &[u8], index: usize, count: usize) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let entry = slice_at(data, index + i * SEQDESC_SIZE, SEQDESC_SIZE)?;
        let mut cur = Cursor::new(entry);
        out.push(cur.fixed_str(32).map_err(cursor_err)?);
    }
    Ok(out)
}

fn read_bodyparts(
    data: &[u8],
    index: usize,
    count: usize,
    world_bones: &[Transform],
    model: &mut MdlModel,
) -> Result<()> {
    for bp in 0..count {
        let entry = slice_at(data, index + bp * BODYPART_SIZE, BODYPART_SIZE)?;
        let mut cur = Cursor::new(entry);
        cur.skip(64).map_err(cursor_err)?; // name
        let num_models = cur.i32().map_err(cursor_err)? as usize;
        cur.skip(4).map_err(cursor_err)?; // base
        let model_index = cur.i32().map_err(cursor_err)? as usize;

        // Só o primeiro submodel de cada bodypart (índice de bodygroup 0) —
        // suficiente pra pose de repouso; variantes (ex.: cabeça com/sem gorro)
        // ficam de fora nesta versão.
        if num_models == 0 {
            continue;
        }
        read_submodel(data, model_index, world_bones, model)?;
    }
    Ok(())
}

fn read_submodel(data: &[u8], offset: usize, world_bones: &[Transform], model: &mut MdlModel) -> Result<()> {
    let entry = slice_at(data, offset, MODEL_SIZE)?;
    let mut cur = Cursor::new(entry);
    cur.skip(64).map_err(cursor_err)?; // name
    cur.skip(4).map_err(cursor_err)?; // type
    cur.skip(4).map_err(cursor_err)?; // boundingradius
    let num_mesh = cur.i32().map_err(cursor_err)? as usize;
    let mesh_index = cur.i32().map_err(cursor_err)? as usize;
    let num_verts = cur.i32().map_err(cursor_err)? as usize;
    let vert_info_index = cur.i32().map_err(cursor_err)? as usize;
    let vert_index = cur.i32().map_err(cursor_err)? as usize;

    // Vértices em espaço de mundo: cada um pertence a um bone (vertinfo[i]).
    let vert_bones = slice_at(data, vert_info_index, num_verts)?;
    let verts_raw = slice_at(data, vert_index, num_verts * 12)?;
    let mut cur_v = Cursor::new(verts_raw);
    let mut world_verts = Vec::with_capacity(num_verts);
    for i in 0..num_verts {
        let local = cur_v.vec3().map_err(cursor_err)?;
        let bone = *vert_bones.get(i).unwrap_or(&0) as usize;
        let t = world_bones.get(bone).copied().unwrap_or_else(Transform::identity);
        world_verts.push(t.apply(local));
    }

    for m in 0..num_mesh {
        let mesh_entry = slice_at(data, mesh_index + m * MESH_SIZE, MESH_SIZE)?;
        let mut cur_m = Cursor::new(mesh_entry);
        let num_tris = cur_m.i32().map_err(cursor_err)?;
        let tri_index = cur_m.i32().map_err(cursor_err)? as usize;
        let skin_ref = cur_m.i32().map_err(cursor_err)? as usize;
        let _ = num_tris; // o stream é terminado por 0, não usa a contagem diretamente

        let (tex_w, tex_h) = model
            .textures
            .get(skin_ref)
            .map(|t| (t.width as f32, t.height as f32))
            .unwrap_or((1.0, 1.0));
        read_triangle_stream(data, tri_index, &world_verts, skin_ref as u32, tex_w, tex_h, model)?;
    }
    Ok(())
}

/// Comandos de triângulo do GoldSrc: `i16 count` (positivo = fan, negativo =
/// strip, `0` termina o stream), seguido de `|count|` vértices, cada um com
/// `vertindex, normindex, s, t` (4 x i16). `s`/`t` estão em espaço de pixel da
/// textura — normalizados aqui por `tex_w`/`tex_h`.
fn read_triangle_stream(
    data: &[u8],
    offset: usize,
    world_verts: &[[f32; 3]],
    texindex: u32,
    tex_w: f32,
    tex_h: f32,
    model: &mut MdlModel,
) -> Result<()> {
    let mut pos = offset;
    loop {
        let header = slice_at(data, pos, 2)?;
        let count = i16::from_le_bytes([header[0], header[1]]);
        pos += 2;
        if count == 0 {
            break;
        }
        let n = count.unsigned_abs() as usize;
        let mut verts = Vec::with_capacity(n);
        for _ in 0..n {
            let raw = slice_at(data, pos, 8)?;
            pos += 8;
            let vertindex = i16::from_le_bytes([raw[0], raw[1]]) as usize;
            // raw[2..4] = normindex, sem uso na pose de repouso sem shading por normal.
            let s = i16::from_le_bytes([raw[4], raw[5]]) as f32;
            let t = i16::from_le_bytes([raw[6], raw[7]]) as f32;
            let p = *world_verts.get(vertindex).ok_or_else(|| {
                MdlError::Malformed(format!("triângulo aponta pro vértice {vertindex} fora da malha"))
            })?;
            verts.push((p, [s / tex_w.max(1.0), t / tex_h.max(1.0)]));
        }

        if count > 0 {
            // Fan: v0 fixo, (v0,vi,vi+1) pra cada i.
            for i in 1..n.saturating_sub(1) {
                push_triangle(model, verts[0], verts[i], verts[i + 1], texindex);
            }
        } else {
            // Strip: alterna o sentido pra manter o winding.
            for i in 0..n.saturating_sub(2) {
                if i % 2 == 0 {
                    push_triangle(model, verts[i], verts[i + 1], verts[i + 2], texindex);
                } else {
                    push_triangle(model, verts[i + 1], verts[i], verts[i + 2], texindex);
                }
            }
        }
    }
    Ok(())
}

fn push_triangle(
    model: &mut MdlModel,
    a: ([f32; 3], [f32; 2]),
    b: ([f32; 3], [f32; 2]),
    c: ([f32; 3], [f32; 2]),
    texindex: u32,
) {
    for (p, _) in [a, b, c] {
        model.positions.extend_from_slice(&p);
    }
    for (_, uv) in [a, b, c] {
        model.uvs.extend_from_slice(&uv);
    }
    model.texindex.push(texindex);
}

#[cfg(test)]
mod tests;
