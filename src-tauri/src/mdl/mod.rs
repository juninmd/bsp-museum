//! Parser de modelos `.mdl` do GoldSrc (Half-Life/CS 1.6, studiomodel versão 10).
//!
//! Formato irmão do BSP (herda a mesma convenção de textura indexada com paleta
//! de 256 cores), mas é um arquivo/propósito diferente — vive fora de `bsp::`,
//! reusando só o `Cursor` little-endian de `bsp::reader` em vez de duplicá-lo.
//!
//! O que é decodificado:
//! - **pose de repouso** (bind pose): a hierarquia de bones composta a partir de
//!   `mstudiobone_t.value` (posição + ângulos de Euler fixos do arquivo). É a
//!   geometria `positions` (espaço de mundo do modelo) que o mapa e o
//!   visualizador desenham por padrão;
//! - **animação** (`sequence_frames`): `mstudioseqdesc_t` (frames, fps, flags,
//!   blends, offset) e os `mstudioanim_t` com os canais por bone comprimidos em
//!   RLE (`mstudioanimvalue_t`), exatamente como `StudioCalcBonePosition` /
//!   `StudioCalcBoneQuaternion` do `studio_render.cpp` do SDK. Cada quadro sai
//!   como pose de mundo por bone (posição + quaternion), pronta pra skinning;
//! - **skin por vértice**: cada vértice sai também em espaço local do bone
//!   (`local_positions` + `vert_bones`), então o frontend pode skinnar na CPU;
//! - **famílias de skin** (`skinref`): tabela família → textura.
//!
//! Limites assumidos: só o *blend* 0 de cada sequência é decodificado (sem
//! mistura de mira/marcha); controladores de bone (`bonecontroller`) não são
//! aplicados; só o primeiro submodel de cada bodypart. Sequências em grupo
//! externo (`seqgroup` > 0, arquivos `nome01.mdl`) só animam se o chamador
//! entregar esse arquivo (`sequence_frames` recebe um resolvedor).
//!
//! Referências: `studiohdr_t`/`mstudiobone_t`/`mstudiotexture_t`/`mstudiomodel_t`/
//! `mstudiomesh_t`/`mstudioseqdesc_t`/`mstudioanim_t` de `studio.h` do SDK do
//! Half-Life (layout público, replicado em várias engines/loaders
//! GoldSrc-compatíveis). A rotação por bone segue `AngleQuaternion` do SDK
//! (ângulos X/Y/Z = roll/pitch/yaw, equivalente a Rz·Ry·Rx) e a composição
//! hierárquica é `pai · filho` (`ConcatTransforms`). Não há `.mdl` real no
//! repositório: a conferência é feita nos testes com arquivos sintéticos.

use crate::bsp::palette;
use crate::bsp::reader::Cursor;
use serde::Serialize;
use std::fmt;

pub const IDENT: &[u8; 4] = b"IDST";
/// Arquivo de grupo externo de sequências (`nome01.mdl`): só dados de animação.
pub const SEQ_IDENT: &[u8; 4] = b"IDSQ";
pub const VERSION: i32 = 10;

const HEADER_NAME_LEN: usize = 64;
const BONE_SIZE: usize = 112; // name[32] + parent(4) + flags(4) + bonecontroller[6](24) + value[6](24) + scale[6](24)
const TEXTURE_SIZE: usize = 80; // name[64] + flags(4) + width(4) + height(4) + index(4)
const BODYPART_SIZE: usize = 76; // name[64] + nummodels(4) + base(4) + modelindex(4)
const MODEL_SIZE: usize = 112; // ver `read_submodel`
const MESH_SIZE: usize = 20; // numtris(4) + triindex(4) + skinref(4) + numnorms(4) + normindex(4)
const SEQDESC_SIZE: usize = 176;
const ANIM_SIZE: usize = 12; // mstudioanim_t: unsigned short offset[6]
const STUDIO_NF_MASKED: i32 = 0x0040;
const STUDIO_LOOPING: i32 = 0x0001;
/// `motiontype` da sequência: o bone `motionbone` tem a translação zerada nesses
/// eixos (o movimento é aplicado pela entidade, não pelo desenho do modelo).
const STUDIO_X: i32 = 0x0001;
const STUDIO_Y: i32 = 0x0002;
const STUDIO_Z: i32 = 0x0004;

/// Floats por bone por quadro: posição (3) + quaternion xyzw (4).
pub const POSE_STRIDE: usize = 7;
/// Tetos defensivos contra arquivo corrompido (não limitam modelo real: o
/// player do CS tem ~100 bones x ~200 quadros e ~5 mil triângulos).
const MAX_TRIS: usize = 2_000_000;
const MAX_FRAMES: usize = 1024;
const MAX_FRAME_FLOATS: usize = 1_000_000;

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

/// Metadado de uma sequência (`mstudioseqdesc_t`), pro seletor/tocador do frontend.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeqInfo {
    pub name: String,
    pub fps: f32,
    pub frames: u32,
    pub looping: bool,
    /// quantos *blends* (mira/marcha) o arquivo tem — só o 0 é decodificado
    pub blends: u32,
    /// 0 = dados no próprio arquivo; N > 0 = no grupo externo `nome0N.mdl`
    pub group: u32,
}

/// Quadros de uma sequência: pose de mundo por bone, `frames * bones * POSE_STRIDE`
/// floats (`px,py,pz,qx,qy,qz,qw` por bone, bone-major dentro de cada quadro).
#[derive(Debug, Clone, Serialize)]
pub struct SeqFrames {
    pub frames: u32,
    pub bones: u32,
    pub fps: f32,
    pub looping: bool,
    pub data: Vec<f32>,
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
    /// mesmos vértices de `positions`, mas em espaço local do bone dono (9
    /// floats por triângulo) — entrada do skinning
    pub local_positions: Vec<f32>,
    /// bone dono de cada vértice (3 por triângulo)
    pub vert_bones: Vec<u8>,
    pub num_bones: u32,
    /// nomes das sequências do modelo (mesma ordem de `seq_info`)
    pub sequences: Vec<String>,
    pub seq_info: Vec<SeqInfo>,
    /// por família de skin: textura que substitui cada textura da família 0
    /// (índice = textura base; identidade na família 0). Sempre ao menos 1.
    pub skin_families: Vec<Vec<u32>>,
}

#[derive(Debug, Clone, Copy)]
struct Transform {
    /// quaternion xyzw (convenção do SDK: `quaternion[3]` = w)
    q: [f32; 4],
    pos: [f32; 3],
}

impl Transform {
    fn identity() -> Self {
        Self { q: [0.0, 0.0, 0.0, 1.0], pos: [0.0, 0.0, 0.0] }
    }

    /// Pose local de um bone: posição + ângulos de Euler (rad) do SDK.
    fn from_euler(pos: [f32; 3], angles: [f32; 3]) -> Self {
        Self { q: angle_quaternion(angles), pos }
    }

    /// `pai · local` (`ConcatTransforms` do SDK): translação do filho gira com o pai.
    fn compose(parent: &Transform, local: &Transform) -> Transform {
        Transform { q: quat_mul(&parent.q, &local.q), pos: add(quat_rotate(&parent.q, local.pos), parent.pos) }
    }

    fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        add(quat_rotate(&self.q, v), self.pos)
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

/// `AngleQuaternion` do SDK: `angles` = (X, Y, Z) em radianos = roll, pitch,
/// yaw; equivale à matriz Rz(yaw) · Ry(pitch) · Rx(roll) (rotação em torno dos
/// eixos fixos do pai, aplicada ao vetor coluna). Saída xyzw.
fn angle_quaternion(angles: [f32; 3]) -> [f32; 4] {
    let (sr, cr) = (angles[0] * 0.5).sin_cos();
    let (sp, cp) = (angles[1] * 0.5).sin_cos();
    let (sy, cy) = (angles[2] * 0.5).sin_cos();
    [
        sr * cp * cy - cr * sp * sy,
        cr * sp * cy + sr * cp * sy,
        cr * cp * sy - sr * sp * cy,
        cr * cp * cy + sr * sp * sy,
    ]
}

/// Produto de Hamilton `a · b` (aplica `b` primeiro, depois `a`).
fn quat_mul(a: &[f32; 4], b: &[f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn quat_rotate(q: &[f32; 4], v: [f32; 3]) -> [f32; 3] {
    // v' = v + 2w(u x v) + 2 u x (u x v), com u = (x, y, z)
    let u = [q[0], q[1], q[2]];
    let t = cross(u, v).map(|c| 2.0 * c);
    let ut = cross(u, t);
    [v[0] + q[3] * t[0] + ut[0], v[1] + q[3] * t[1] + ut[1], v[2] + q[3] * t[2] + ut[2]]
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
    /// `value[0..3]` = posição, `value[3..6]` = ângulos (rad) do repouso
    value: [f32; 6],
    /// escala dos canais animados (`valor RLE x scale + value`)
    scale: [f32; 6],
}

impl Bone {
    fn bind_local(&self) -> Transform {
        Transform::from_euler([self.value[0], self.value[1], self.value[2]], [self.value[3], self.value[4], self.value[5]])
    }
}

/// Campos do `studiohdr_t` que o parser usa (contagens/offsets já validados como
/// não-negativos).
struct Header {
    num_bones: usize,
    bone_index: usize,
    num_seq: usize,
    seq_index: usize,
    num_textures: usize,
    texture_index: usize,
    num_skinref: usize,
    num_skinfamilies: usize,
    skin_index: usize,
    num_bodyparts: usize,
    bodypart_index: usize,
}

/// i32 do arquivo -> contagem/offset; negativo é corrupção (e `as usize` viraria
/// um número gigante que estoura alocação/aritmética).
fn count(v: i32, what: &str) -> Result<usize> {
    usize::try_from(v).map_err(|_| MdlError::Malformed(format!("{what} negativo ({v})")))
}

fn read_header(data: &[u8]) -> Result<Header> {
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

    let num_bones = count(cur.i32().map_err(cursor_err)?, "numbones")?;
    let bone_index = count(cur.i32().map_err(cursor_err)?, "boneindex")?;
    cur.skip(4 * 2).map_err(cursor_err)?; // numbonecontrollers, bonecontrollerindex
    cur.skip(4 * 2).map_err(cursor_err)?; // numhitboxes, hitboxindex
    let num_seq = count(cur.i32().map_err(cursor_err)?, "numseq")?;
    let seq_index = count(cur.i32().map_err(cursor_err)?, "seqindex")?;
    cur.skip(4 * 2).map_err(cursor_err)?; // numseqgroups, seqgroupindex

    let num_textures = count(cur.i32().map_err(cursor_err)?, "numtextures")?;
    let texture_index = count(cur.i32().map_err(cursor_err)?, "textureindex")?;
    cur.skip(4).map_err(cursor_err)?; // texturedataindex — informativo, cada mstudiotexture_t já traz seu offset absoluto
    let num_skinref = count(cur.i32().map_err(cursor_err)?, "numskinref")?;
    let num_skinfamilies = count(cur.i32().map_err(cursor_err)?, "numskinfamilies")?;
    let skin_index = count(cur.i32().map_err(cursor_err)?, "skinindex")?;

    let num_bodyparts = count(cur.i32().map_err(cursor_err)?, "numbodyparts")?;
    let bodypart_index = count(cur.i32().map_err(cursor_err)?, "bodypartindex")?;

    Ok(Header {
        num_bones,
        bone_index,
        num_seq,
        seq_index,
        num_textures,
        texture_index,
        num_skinref,
        num_skinfamilies,
        skin_index,
        num_bodyparts,
        bodypart_index,
    })
}

/// Decodifica um `.mdl` v10 completo: bones (pose de repouso), texturas (skins),
/// famílias de skin, metadado das sequências e a malha de todos os
/// bodyparts/meshes (em espaço de mundo e em espaço local do bone). A animação
/// em si fica em `sequence_frames`, sob demanda.
pub fn parse(data: &[u8]) -> Result<MdlModel> {
    let h = read_header(data)?;
    let bones = read_bones(data, h.bone_index, h.num_bones)?;
    let bind: Vec<Transform> = bones.iter().map(Bone::bind_local).collect();
    let world_bones = world_transforms(&bones, &bind)?;
    let textures = read_textures(data, h.texture_index, h.num_textures)?;
    let descs = read_seqdescs(data, h.seq_index, h.num_seq)?;
    let (slot_to_tex, skin_families) = read_skin_families(data, &h, textures.len())?;

    let mut model = MdlModel {
        textures,
        num_bones: bones.len() as u32,
        sequences: descs.iter().map(|d| d.info.name.clone()).collect(),
        seq_info: descs.into_iter().map(|d| d.info).collect(),
        skin_families,
        ..Default::default()
    };
    read_bodyparts(data, h.bodypart_index, h.num_bodyparts, &world_bones, &slot_to_tex, &mut model)?;
    Ok(model)
}

/// Fatia `data[offset..offset+len]`, com tudo checado (offset/len vêm do arquivo).
fn slice_at(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| MdlError::Malformed(format!("offset {offset} + {len} estoura usize")))?;
    data.get(offset..end)
        .ok_or_else(|| MdlError::Malformed(format!("offset {offset}..{end} fora do arquivo ({} bytes)", data.len())))
}

/// Tabela de `count` registros de `size` bytes a partir de `index`: valida o
/// tamanho total de uma vez (um `count` absurdo vira erro, não alocação gigante).
fn table(data: &[u8], index: usize, count: usize, size: usize) -> Result<std::slice::ChunksExact<'_, u8>> {
    let total = count
        .checked_mul(size)
        .ok_or_else(|| MdlError::Malformed(format!("tabela de {count} x {size} bytes estoura usize")))?;
    Ok(slice_at(data, index, total)?.chunks_exact(size.max(1)))
}

fn le_i32(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn le_f32(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn read_bones(data: &[u8], index: usize, num: usize) -> Result<Vec<Bone>> {
    let rows = table(data, index, num, BONE_SIZE)?; // antes do with_capacity: `num` pode ser absurdo
    let mut out = Vec::with_capacity(num);
    for (i, entry) in rows.enumerate() {
        // name[32], parent, flags, bonecontroller[6] = 32+4+4+24 = 64 -> value[6], scale[6]
        let parent = le_i32(entry, 32);
        let mut value = [0.0f32; 6];
        let mut scale = [0.0f32; 6];
        for j in 0..6 {
            value[j] = le_f32(entry, 64 + j * 4);
            scale[j] = le_f32(entry, 88 + j * 4);
        }
        // O formato garante pai antes do filho na lista (raiz = parent < 0).
        if parent >= 0 && parent as usize >= i {
            return Err(MdlError::Malformed(format!("bone {i} aponta pra um pai inválido ({parent})")));
        }
        out.push(Bone { parent, value, scale });
    }
    Ok(out)
}

/// Pose de mundo de cada bone a partir das poses locais: como `pai < filho`
/// (checado em `read_bones`), uma passada basta.
fn world_transforms(bones: &[Bone], locals: &[Transform]) -> Result<Vec<Transform>> {
    let mut world: Vec<Transform> = Vec::with_capacity(bones.len());
    for (i, bone) in bones.iter().enumerate() {
        let local = locals.get(i).copied().unwrap_or_else(Transform::identity);
        let t = match usize::try_from(bone.parent) {
            Err(_) => local,
            Ok(p) => {
                let parent = world
                    .get(p)
                    .copied()
                    .ok_or_else(|| MdlError::Malformed(format!("bone {i} aponta pra um pai inválido")))?;
                Transform::compose(&parent, &local)
            }
        };
        world.push(t);
    }
    Ok(world)
}

fn read_textures(data: &[u8], index: usize, num: usize) -> Result<Vec<MdlTexture>> {
    let rows = table(data, index, num, TEXTURE_SIZE)?;
    let mut out = Vec::with_capacity(num);
    for entry in rows {
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

/// Tabela de skins: `short skinref[numskinfamilies][numskinref]`; a skin
/// `skinref` de cada mesh aponta pra coluna, a família escolhe a linha.
/// Retorna (slot -> textura na família 0, remapa de textura por família).
/// Sem tabela (modelos sem famílias) vale a identidade.
fn read_skin_families(data: &[u8], h: &Header, num_textures: usize) -> Result<(Vec<u32>, Vec<Vec<u32>>)> {
    let identity: Vec<u32> = (0..num_textures as u32).collect();
    if h.num_skinref == 0 || h.num_skinfamilies == 0 {
        return Ok((identity.clone(), vec![identity]));
    }
    let total = h
        .num_skinref
        .checked_mul(h.num_skinfamilies)
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(|| MdlError::Malformed("tabela de skins estoura usize".into()))?;
    let raw = slice_at(data, h.skin_index, total)?;
    let mut rows: Vec<Vec<u32>> = Vec::with_capacity(h.num_skinfamilies);
    for row in raw.chunks_exact(h.num_skinref * 2) {
        let mut slots = Vec::with_capacity(h.num_skinref);
        for v in row.chunks_exact(2) {
            let tex = i16::from_le_bytes([v[0], v[1]]);
            slots.push(u32::try_from(tex).map_err(|_| MdlError::Malformed(format!("skinref negativo ({tex})")))?);
        }
        rows.push(slots);
    }
    let base = rows[0].clone();
    let families = rows
        .iter()
        .map(|row| {
            let mut remap = identity.clone();
            for (slot, &tex) in row.iter().enumerate() {
                if let Some(entry) = base.get(slot).and_then(|&b| remap.get_mut(b as usize)) {
                    *entry = tex;
                }
            }
            remap
        })
        .collect();
    Ok((base, families))
}

/// Sequência lida do arquivo: o `SeqInfo` público + os campos de animação.
struct SeqDesc {
    info: SeqInfo,
    motion_type: i32,
    motion_bone: i32,
    anim_index: usize,
}

fn read_seqdescs(data: &[u8], index: usize, num: usize) -> Result<Vec<SeqDesc>> {
    let rows = table(data, index, num, SEQDESC_SIZE)?;
    let mut out = Vec::with_capacity(num);
    for entry in rows {
        let name = Cursor::new(entry).fixed_str(32).map_err(cursor_err)?;
        let fps = le_f32(entry, 32);
        let flags = le_i32(entry, 36);
        out.push(SeqDesc {
            info: SeqInfo {
                name,
                // fps inválido (0/NaN/negativo) viraria divisão por zero no tocador
                fps: if fps.is_finite() && fps > 0.0 { fps.min(1000.0) } else { 30.0 },
                frames: le_i32(entry, 56).max(0) as u32,
                looping: flags & STUDIO_LOOPING != 0,
                blends: le_i32(entry, 120).max(0) as u32,
                group: le_i32(entry, 156).max(0) as u32,
            },
            motion_type: le_i32(entry, 68),
            motion_bone: le_i32(entry, 72),
            anim_index: count(le_i32(entry, 124), "animindex")?,
        });
    }
    Ok(out)
}

/// Valor RLE `frame` de um canal (`mstudioanimvalue_t`): cada trecho começa com
/// `(valid, total)` — `total` quadros cobertos por `valid` valores `i16`; o
/// último valor é mantido nos quadros que sobram. Mesmo laço do SDK, mas com
/// cada leitura checada e `total == 0` recusado (no SDK isso não terminaria).
fn rle_value(buf: &[u8], start: usize, frame: usize) -> Result<i16> {
    let mut pos = start;
    let mut k = frame;
    loop {
        let head = slice_at(buf, pos, 2)?;
        let (valid, total) = (head[0] as usize, head[1] as usize);
        if total == 0 {
            return Err(MdlError::Malformed("trecho RLE de animação com total 0".into()));
        }
        if total <= k {
            k -= total;
            pos = pos
                .checked_add((valid + 1) * 2)
                .ok_or_else(|| MdlError::Malformed("offset RLE estoura usize".into()))?;
            continue;
        }
        if valid == 0 {
            return Ok(0);
        }
        let i = if valid > k { k } else { valid - 1 };
        let v = slice_at(buf, pos + 2 + i * 2, 2)?;
        return Ok(i16::from_le_bytes([v[0], v[1]]));
    }
}

/// Quadros de uma sequência como pose de mundo por bone (blend 0).
/// `external(grupo)` entrega o arquivo `nome0N.mdl` das sequências com
/// `seqgroup` > 0; sem ele (`None`) essas sequências dão erro.
pub fn sequence_frames(data: &[u8], seq: usize, external: &dyn Fn(u32) -> Option<Vec<u8>>) -> Result<SeqFrames> {
    let h = read_header(data)?;
    let bones = read_bones(data, h.bone_index, h.num_bones)?;
    let descs = read_seqdescs(data, h.seq_index, h.num_seq)?;
    let desc = descs
        .get(seq)
        .ok_or_else(|| MdlError::Malformed(format!("sequência {seq} não existe (o modelo tem {})", descs.len())))?;

    let ext;
    let buf: &[u8] = if desc.info.group == 0 {
        data
    } else {
        ext = external(desc.info.group).ok_or_else(|| {
            MdlError::Malformed(format!(
                "animação no arquivo externo do grupo {} (nome0N.mdl), que não foi encontrado",
                desc.info.group
            ))
        })?;
        if ext.len() < 8 || &ext[0..4] != SEQ_IDENT {
            return Err(MdlError::Malformed(format!("grupo externo {} não é um arquivo IDSQ", desc.info.group)));
        }
        &ext
    };

    let frames = (desc.info.frames as usize).max(1);
    let nb = bones.len();
    if nb == 0 {
        return Err(MdlError::Malformed("modelo sem bones".into()));
    }
    let floats = frames.saturating_mul(nb).saturating_mul(POSE_STRIDE);
    if frames > MAX_FRAMES || floats > MAX_FRAME_FLOATS {
        return Err(MdlError::Malformed(format!("sequência grande demais ({frames} quadros x {nb} bones)")));
    }

    // mstudioanim_t de cada bone no blend 0: 6 x u16 (X,Y,Z, rotX,rotY,rotZ),
    // offsets relativos ao início do próprio mstudioanim_t; 0 = canal fixo.
    let anim_table = slice_at(buf, desc.anim_index, nb * ANIM_SIZE)?;
    let mut out = Vec::with_capacity(floats);
    for frame in 0..frames {
        let mut locals = Vec::with_capacity(nb);
        for (i, (bone, entry)) in bones.iter().zip(anim_table.chunks_exact(ANIM_SIZE)).enumerate() {
            let base = desc.anim_index + i * ANIM_SIZE;
            let mut ch = [0.0f32; 6];
            for (j, slot) in ch.iter_mut().enumerate() {
                let offset = u16::from_le_bytes([entry[j * 2], entry[j * 2 + 1]]) as usize;
                *slot = if offset == 0 {
                    bone.value[j]
                } else {
                    bone.value[j] + rle_value(buf, base + offset, frame)? as f32 * bone.scale[j]
                };
            }
            locals.push(Transform::from_euler([ch[0], ch[1], ch[2]], [ch[3], ch[4], ch[5]]));
        }
        // O movimento do bone raiz (andar/correr) é do jogo, não do desenho.
        if let Some(m) = usize::try_from(desc.motion_bone).ok().and_then(|m| locals.get_mut(m)) {
            if desc.motion_type & STUDIO_X != 0 {
                m.pos[0] = 0.0;
            }
            if desc.motion_type & STUDIO_Y != 0 {
                m.pos[1] = 0.0;
            }
            if desc.motion_type & STUDIO_Z != 0 {
                m.pos[2] = 0.0;
            }
        }
        for t in world_transforms(&bones, &locals)? {
            out.extend_from_slice(&t.pos);
            out.extend_from_slice(&t.q);
        }
    }
    Ok(SeqFrames { frames: frames as u32, bones: nb as u32, fps: desc.info.fps, looping: desc.info.looping, data: out })
}

fn read_bodyparts(
    data: &[u8],
    index: usize,
    num: usize,
    world_bones: &[Transform],
    slot_to_tex: &[u32],
    model: &mut MdlModel,
) -> Result<()> {
    for entry in table(data, index, num, BODYPART_SIZE)? {
        let mut cur = Cursor::new(entry);
        cur.skip(64).map_err(cursor_err)?; // name
        let num_models = cur.i32().map_err(cursor_err)?;
        cur.skip(4).map_err(cursor_err)?; // base
        let model_index = count(cur.i32().map_err(cursor_err)?, "modelindex")?;

        // Só o primeiro submodel de cada bodypart (índice de bodygroup 0) —
        // variantes (ex.: cabeça com/sem gorro) ficam de fora nesta versão.
        if num_models <= 0 {
            continue;
        }
        read_submodel(data, model_index, world_bones, slot_to_tex, model)?;
    }
    Ok(())
}

fn read_submodel(
    data: &[u8],
    offset: usize,
    world_bones: &[Transform],
    slot_to_tex: &[u32],
    model: &mut MdlModel,
) -> Result<()> {
    let entry = slice_at(data, offset, MODEL_SIZE)?;
    let mut cur = Cursor::new(entry);
    cur.skip(64).map_err(cursor_err)?; // name
    cur.skip(4).map_err(cursor_err)?; // type
    cur.skip(4).map_err(cursor_err)?; // boundingradius
    let num_mesh = count(cur.i32().map_err(cursor_err)?, "nummesh")?;
    let mesh_index = count(cur.i32().map_err(cursor_err)?, "meshindex")?;
    let num_verts = count(cur.i32().map_err(cursor_err)?, "numverts")?;
    let vert_info_index = count(cur.i32().map_err(cursor_err)?, "vertinfoindex")?;
    let vert_index = count(cur.i32().map_err(cursor_err)?, "vertindex")?;

    // Cada vértice pertence a um bone (vertinfo[i]) e está em espaço local dele.
    let vert_bones = slice_at(data, vert_info_index, num_verts)?;
    let verts_raw = slice_at(
        data,
        vert_index,
        num_verts.checked_mul(12).ok_or_else(|| MdlError::Malformed("vértices estouram usize".into()))?,
    )?;
    let mut cur_v = Cursor::new(verts_raw);
    let mut verts = Vec::with_capacity(num_verts);
    for &bone in vert_bones {
        let local = cur_v.vec3().map_err(cursor_err)?;
        let t = world_bones.get(bone as usize).ok_or_else(|| {
            MdlError::Malformed(format!("vértice referencia o bone {bone}, mas o modelo tem {}", world_bones.len()))
        })?;
        verts.push(Vert { world: t.apply(local), local, bone });
    }

    for entry in table(data, mesh_index, num_mesh, MESH_SIZE)? {
        let mut cur_m = Cursor::new(entry);
        cur_m.skip(4).map_err(cursor_err)?; // numtris — o stream é terminado por 0, não usa a contagem
        let tri_index = count(cur_m.i32().map_err(cursor_err)?, "triindex")?;
        let skin_ref = count(cur_m.i32().map_err(cursor_err)?, "skinref")?;

        // skinref do mesh -> textura da família 0 (nem sempre é a identidade).
        let tex = slot_to_tex.get(skin_ref).copied().unwrap_or(skin_ref as u32);
        let (tex_w, tex_h) = model
            .textures
            .get(tex as usize)
            .map(|t| (t.width as f32, t.height as f32))
            .unwrap_or((1.0, 1.0));
        read_triangle_stream(data, tri_index, &verts, tex, tex_w, tex_h, model)?;
    }
    Ok(())
}

/// Vértice decodificado: posição de mundo (repouso), local do bone e o bone.
struct Vert {
    world: [f32; 3],
    local: [f32; 3],
    bone: u8,
}

/// Comandos de triângulo do GoldSrc: `i16 count` (positivo = fan, negativo =
/// strip, `0` termina o stream), seguido de `|count|` vértices, cada um com
/// `vertindex, normindex, s, t` (4 x i16). `s`/`t` estão em espaço de pixel da
/// textura — normalizados aqui por `tex_w`/`tex_h`.
fn read_triangle_stream(
    data: &[u8],
    offset: usize,
    verts_in: &[Vert],
    texindex: u32,
    tex_w: f32,
    tex_h: f32,
    model: &mut MdlModel,
) -> Result<()> {
    let mut pos = offset;
    loop {
        let header = slice_at(data, pos, 2)?;
        let cmd = i16::from_le_bytes([header[0], header[1]]);
        pos += 2;
        if cmd == 0 {
            break;
        }
        let n = cmd.unsigned_abs() as usize;
        let mut verts = Vec::with_capacity(n);
        for _ in 0..n {
            let raw = slice_at(data, pos, 8)?;
            pos += 8;
            let vertindex = i16::from_le_bytes([raw[0], raw[1]]) as usize;
            // raw[2..4] = normindex, sem uso: o visualizador recalcula as normais.
            let s = i16::from_le_bytes([raw[4], raw[5]]) as f32;
            let t = i16::from_le_bytes([raw[6], raw[7]]) as f32;
            let v = verts_in.get(vertindex).ok_or_else(|| {
                MdlError::Malformed(format!("triângulo aponta pro vértice {vertindex} fora da malha"))
            })?;
            verts.push((v, [s / tex_w.max(1.0), t / tex_h.max(1.0)]));
        }

        if cmd > 0 {
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
        if model.texindex.len() > MAX_TRIS {
            return Err(MdlError::Malformed(format!("mais de {MAX_TRIS} triângulos")));
        }
    }
    Ok(())
}

fn push_triangle(
    model: &mut MdlModel,
    a: (&Vert, [f32; 2]),
    b: (&Vert, [f32; 2]),
    c: (&Vert, [f32; 2]),
    texindex: u32,
) {
    for (v, _) in [a, b, c] {
        model.positions.extend_from_slice(&v.world);
        model.local_positions.extend_from_slice(&v.local);
        model.vert_bones.push(v.bone);
    }
    for (_, uv) in [a, b, c] {
        model.uvs.extend_from_slice(&uv);
    }
    model.texindex.push(texindex);
}

/// Modelo `.mdl` sintético pra teste, montado byte a byte (mesmo espírito dos
/// BSPs de teste): 1 bone raiz (só translação em X), 1 skin 1×1, 1 bodypart
/// com 1 submodel de 1 triângulo, 1 sequência. `pub(crate)` pra ser reusado
/// tanto pelos testes deste módulo quanto pelo teste de integração em
/// `catalog::mesh` (props no mapa).
#[cfg(test)]
pub(crate) fn synthetic_mdl() -> Vec<u8> {
    fn push_str_fixed(out: &mut Vec<u8>, s: &str, n: usize) {
        let mut buf = vec![0u8; n];
        let bytes = s.as_bytes();
        buf[..bytes.len().min(n)].copy_from_slice(&bytes[..bytes.len().min(n)]);
        out.extend_from_slice(&buf);
    }

    let mut out = Vec::new();

    // ---- header ----
    out.extend_from_slice(IDENT);
    out.extend_from_slice(&VERSION.to_le_bytes());
    push_str_fixed(&mut out, "test", 64); // name
    out.extend_from_slice(&0i32.to_le_bytes()); // length
    for _ in 0..5 {
        out.extend_from_slice(&[0.0f32; 3].map(f32::to_le_bytes).concat()); // eyeposition/min/max/bbmin/bbmax
    }
    out.extend_from_slice(&0i32.to_le_bytes()); // flags
    out.extend_from_slice(&1i32.to_le_bytes()); // numbones
    out.extend_from_slice(&212i32.to_le_bytes()); // boneindex
    out.extend_from_slice(&0i32.to_le_bytes()); // numbonecontrollers
    out.extend_from_slice(&0i32.to_le_bytes()); // bonecontrollerindex
    out.extend_from_slice(&0i32.to_le_bytes()); // numhitboxes
    out.extend_from_slice(&0i32.to_le_bytes()); // hitboxindex
    out.extend_from_slice(&1i32.to_le_bytes()); // numseq
    out.extend_from_slice(&1448i32.to_le_bytes()); // seqindex
    out.extend_from_slice(&0i32.to_le_bytes()); // numseqgroups
    out.extend_from_slice(&0i32.to_le_bytes()); // seqgroupindex
    out.extend_from_slice(&1i32.to_le_bytes()); // numtextures
    out.extend_from_slice(&324i32.to_le_bytes()); // textureindex
    out.extend_from_slice(&0i32.to_le_bytes()); // texturedataindex (não usado)
    out.extend_from_slice(&0i32.to_le_bytes()); // numskinref
    out.extend_from_slice(&0i32.to_le_bytes()); // numskinfamilies
    out.extend_from_slice(&0i32.to_le_bytes()); // skinindex
    out.extend_from_slice(&1i32.to_le_bytes()); // numbodyparts
    out.extend_from_slice(&1173i32.to_le_bytes()); // bodypartindex
    assert_eq!(out.len(), 212, "header do teste desalinhou");

    // ---- bone (só translação em X=10, sem rotação) ----
    push_str_fixed(&mut out, "root", 32);
    out.extend_from_slice(&(-1i32).to_le_bytes()); // parent
    out.extend_from_slice(&0i32.to_le_bytes()); // flags
    out.extend_from_slice(&[0u8; 24]); // bonecontroller[6]
    let value = [10.0f32, 0.0, 0.0, 0.0, 0.0, 0.0];
    for v in value {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0u8; 24]); // scale[6]
    assert_eq!(out.len(), 324, "bone do teste desalinhou");

    // ---- textura (entry de 80 bytes; pixels ficam em outro offset) ----
    push_str_fixed(&mut out, "skin1", 64);
    out.extend_from_slice(&0i32.to_le_bytes()); // flags (sem STUDIO_NF_MASKED)
    out.extend_from_slice(&1u32.to_le_bytes()); // width
    out.extend_from_slice(&1u32.to_le_bytes()); // height
    out.extend_from_slice(&404i32.to_le_bytes()); // offset do pixel
    assert_eq!(out.len(), 404, "entrada de textura do teste desalinhou");

    // pixel (índice 5) + paleta de 256 cores (cor 5 = vermelho puro)
    out.push(5);
    let mut palette = vec![0u8; 768];
    palette[15..18].copy_from_slice(&[255, 0, 0]); // índice 5 = offset 15
    out.extend_from_slice(&palette);
    assert_eq!(out.len(), 1173, "bloco de pixels do teste desalinhou");

    // ---- bodypart ----
    push_str_fixed(&mut out, "body", 64);
    out.extend_from_slice(&1i32.to_le_bytes()); // nummodels
    out.extend_from_slice(&0i32.to_le_bytes()); // base
    out.extend_from_slice(&1249i32.to_le_bytes()); // modelindex
    assert_eq!(out.len(), 1249, "bodypart do teste desalinhou");

    // ---- submodel ----
    push_str_fixed(&mut out, "sub", 64);
    out.extend_from_slice(&0i32.to_le_bytes()); // type
    out.extend_from_slice(&0f32.to_le_bytes()); // boundingradius
    out.extend_from_slice(&1i32.to_le_bytes()); // nummesh
    out.extend_from_slice(&1400i32.to_le_bytes()); // meshindex
    out.extend_from_slice(&3i32.to_le_bytes()); // numverts
    out.extend_from_slice(&1361i32.to_le_bytes()); // vertinfoindex
    out.extend_from_slice(&1364i32.to_le_bytes()); // vertindex
    out.extend_from_slice(&[0u8; 112 - 64 - 4 - 4 - 4 - 4 - 4 - 4 - 4]); // resto do struct (numnorms..groupindex)
    assert_eq!(out.len(), 1361, "submodel do teste desalinhou");

    // ---- vertinfo (bone por vértice) ----
    out.extend_from_slice(&[0u8, 0u8, 0u8]);
    assert_eq!(out.len(), 1364, "vertinfo do teste desalinhou");

    // ---- vértices locais: (0,0,0) (1,0,0) (0,1,0) ----
    for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    assert_eq!(out.len(), 1400, "vértices do teste desalinharam");

    // ---- mesh ----
    out.extend_from_slice(&1i32.to_le_bytes()); // numtris
    out.extend_from_slice(&1420i32.to_le_bytes()); // triindex
    out.extend_from_slice(&0i32.to_le_bytes()); // skinref
    out.extend_from_slice(&0i32.to_le_bytes()); // numnorms
    out.extend_from_slice(&0i32.to_le_bytes()); // normindex
    assert_eq!(out.len(), 1420, "mesh do teste desalinhou");

    // ---- stream de triângulo: fan de 3 vértices (0,1,2), depois 0 termina ----
    out.extend_from_slice(&3i16.to_le_bytes());
    for vi in [0i16, 1, 2] {
        out.extend_from_slice(&vi.to_le_bytes()); // vertindex
        out.extend_from_slice(&0i16.to_le_bytes()); // normindex
        out.extend_from_slice(&0i16.to_le_bytes()); // s
        out.extend_from_slice(&0i16.to_le_bytes()); // t
    }
    out.extend_from_slice(&0i16.to_le_bytes()); // terminador
    assert_eq!(out.len(), 1448, "stream de triângulo do teste desalinhou");

    // ---- sequência (só o nome importa aqui) ----
    push_str_fixed(&mut out, "idle", 32);
    out.extend_from_slice(&[0u8; SEQDESC_SIZE - 32]);
    assert_eq!(out.len(), 1624, "sequência do teste desalinhou");

    out
}

#[cfg(test)]
mod tests;
