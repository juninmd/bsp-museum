pub mod entities;
pub mod palette;
pub mod reader;
pub mod render;
pub mod sky;
pub mod wad;

use base64::Engine as _;
use reader::{BspError, Cursor, Result};
use serde::Serialize;

/// BSP do GoldSrc (Half-Life, Counter-Strike 1.6).
pub const GOLDSRC_VERSION: i32 = 30;
pub const LUMP_COUNT: usize = 15;
const HEADER_SIZE: usize = 4 + LUMP_COUNT * 8;

pub const LUMP_ENTITIES: usize = 0;
pub const LUMP_PLANES: usize = 1;
pub const LUMP_TEXTURES: usize = 2;
pub const LUMP_VERTEXES: usize = 3;
pub const LUMP_VISIBILITY: usize = 4;
pub const LUMP_NODES: usize = 5;
pub const LUMP_TEXINFO: usize = 6;
pub const LUMP_FACES: usize = 7;
pub const LUMP_LIGHTING: usize = 8;
pub const LUMP_CLIPNODES: usize = 9;
pub const LUMP_LEAVES: usize = 10;
pub const LUMP_MARKSURFACES: usize = 11;
pub const LUMP_EDGES: usize = 12;
pub const LUMP_SURFEDGES: usize = 13;
pub const LUMP_MODELS: usize = 14;

pub const LUMP_NAMES: [&str; LUMP_COUNT] = [
    "entities",
    "planes",
    "textures",
    "vertexes",
    "visibility",
    "nodes",
    "texinfo",
    "faces",
    "lighting",
    "clipnodes",
    "leaves",
    "marksurfaces",
    "edges",
    "surfedges",
    "models",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Lump {
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Face {
    pub first_edge: i32,
    pub num_edges: u16,
    pub texinfo: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct Texture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// textura embutida no BSP (não depende de WAD externo)
    pub embedded: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Model {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub origin: [f32; 3],
    pub first_face: i32,
    pub num_faces: i32,
}

/// Geometria e metadados de um BSP, já validados.
#[derive(Debug, Clone)]
pub struct Bsp {
    pub version: i32,
    pub lumps: [Lump; LUMP_COUNT],
    pub entities_raw: String,
    pub vertices: Vec<[f32; 3]>,
    pub edges: Vec<(u16, u16)>,
    pub surfedges: Vec<i32>,
    pub faces: Vec<Face>,
    pub texinfo_miptex: Vec<u32>,
    /// eixos de textura (vecs[2][4]) de cada texinfo — é o que mapeia um
    /// vértice do mundo em coordenada UV da textura.
    pub texinfo_vecs: Vec<[[f32; 4]; 2]>,
    pub textures: Vec<Texture>,
    /// nome de cada textura da tabela crua (índice do texinfo). Vazio quando a
    /// entrada não existe no BSP (`offset -1`) — o nome vive no WAD.
    pub raw_texture_name: Vec<String>,
    pub models: Vec<Model>,
}

fn lump_slice<'a>(data: &'a [u8], lump: Lump, name: &'static str) -> Result<&'a [u8]> {
    let end = lump.offset.checked_add(lump.length).ok_or(BspError::LumpOutOfBounds {
        lump: name,
        offset: lump.offset,
        len: lump.length,
        file: data.len(),
    })?;
    data.get(lump.offset..end).ok_or(BspError::LumpOutOfBounds {
        lump: name,
        offset: lump.offset,
        len: lump.length,
        file: data.len(),
    })
}

fn check_stride(lump: Lump, name: &'static str, stride: usize) -> Result<usize> {
    if lump.length % stride != 0 {
        return Err(BspError::LumpMisaligned { lump: name, len: lump.length, stride });
    }
    Ok(lump.length / stride)
}

impl Bsp {
    /// Lê só o cabeçalho: barato o bastante para varrer centenas de mapas.
    pub fn header(data: &[u8]) -> Result<(i32, [Lump; LUMP_COUNT])> {
        if data.len() < HEADER_SIZE {
            return Err(BspError::TooSmall { need: HEADER_SIZE, have: data.len() });
        }
        let mut cur = Cursor::new(data);
        let version = cur.i32()?;
        if version != GOLDSRC_VERSION {
            return Err(BspError::BadVersion(version));
        }
        let mut lumps = [Lump { offset: 0, length: 0 }; LUMP_COUNT];
        for lump in lumps.iter_mut() {
            let offset = cur.i32()?;
            let length = cur.i32()?;
            if offset < 0 || length < 0 {
                return Err(BspError::LumpOutOfBounds {
                    lump: "header",
                    offset: offset.unsigned_abs() as usize,
                    len: length.unsigned_abs() as usize,
                    file: data.len(),
                });
            }
            *lump = Lump { offset: offset as usize, length: length as usize };
        }
        Ok((version, lumps))
    }

    /// Só o lump de entidades e o modelo 0 — o suficiente para o catálogo.
    pub fn parse_light(data: &[u8]) -> Result<(String, Option<Model>, [Lump; LUMP_COUNT])> {
        let (_, lumps) = Self::header(data)?;
        let entities_raw = read_entities(data, lumps[LUMP_ENTITIES])?;
        let models = read_models(data, lumps[LUMP_MODELS])?;
        Ok((entities_raw, models.first().copied(), lumps))
    }

    pub fn parse(data: &[u8]) -> Result<Bsp> {
        let (version, lumps) = Self::header(data)?;

        let entities_raw = read_entities(data, lumps[LUMP_ENTITIES])?;

        let vtx = lumps[LUMP_VERTEXES];
        let count = check_stride(vtx, "vertexes", 12)?;
        let mut cur = Cursor::new(lump_slice(data, vtx, "vertexes")?);
        let mut vertices = Vec::with_capacity(count);
        for _ in 0..count {
            vertices.push(cur.vec3()?);
        }

        let edg = lumps[LUMP_EDGES];
        let count = check_stride(edg, "edges", 4)?;
        let mut cur = Cursor::new(lump_slice(data, edg, "edges")?);
        let mut edges = Vec::with_capacity(count);
        for _ in 0..count {
            edges.push((cur.u16()?, cur.u16()?));
        }

        let sfe = lumps[LUMP_SURFEDGES];
        let count = check_stride(sfe, "surfedges", 4)?;
        let mut cur = Cursor::new(lump_slice(data, sfe, "surfedges")?);
        let mut surfedges = Vec::with_capacity(count);
        for _ in 0..count {
            surfedges.push(cur.i32()?);
        }

        let fac = lumps[LUMP_FACES];
        let count = check_stride(fac, "faces", 20)?;
        let mut cur = Cursor::new(lump_slice(data, fac, "faces")?);
        let mut faces = Vec::with_capacity(count);
        for _ in 0..count {
            cur.skip(2)?; // planenum
            cur.skip(2)?; // side
            let first_edge = cur.i32()?;
            let num_edges = cur.u16()?;
            let texinfo = cur.u16()?;
            cur.skip(4)?; // styles[4]
            cur.skip(4)?; // lightofs
            faces.push(Face { first_edge, num_edges, texinfo });
        }

        let tin = lumps[LUMP_TEXINFO];
        let count = check_stride(tin, "texinfo", 40)?;
        let mut cur = Cursor::new(lump_slice(data, tin, "texinfo")?);
        let mut texinfo_miptex = Vec::with_capacity(count);
        let mut texinfo_vecs = Vec::with_capacity(count);
        for _ in 0..count {
            let s_axis = cur.vec4()?;
            let t_axis = cur.vec4()?;
            texinfo_vecs.push([s_axis, t_axis]);
            texinfo_miptex.push(cur.u32()?);
            cur.skip(4)?; // flags
        }

        let (textures, raw_texture_name) = read_textures(data, lumps[LUMP_TEXTURES])?;
        let models = read_models(data, lumps[LUMP_MODELS])?;

        Ok(Bsp {
            version,
            lumps,
            entities_raw,
            vertices,
            edges,
            surfedges,
            faces,
            texinfo_miptex,
            texinfo_vecs,
            textures,
            raw_texture_name,
            models,
        })
    }

    /// Vértices de uma face, seguindo surfedges (negativo = aresta invertida).
    pub fn face_polygon(&self, face: &Face) -> Option<Vec<[f32; 3]>> {
        let n = face.num_edges as usize;
        if n < 3 {
            return None;
        }
        let start = usize::try_from(face.first_edge).ok()?;
        let mut points = Vec::with_capacity(n);
        for i in 0..n {
            let se = *self.surfedges.get(start + i)?;
            let (a, b) = *self.edges.get(se.unsigned_abs() as usize)?;
            let vi = if se >= 0 { a } else { b };
            points.push(*self.vertices.get(vi as usize)?);
        }
        Some(points)
    }

    pub fn texture_of(&self, face: &Face) -> Option<&Texture> {
        let miptex = *self.texinfo_miptex.get(face.texinfo as usize)? as usize;
        self.textures.get(miptex)
    }

    pub fn bounds(&self) -> Option<Model> {
        self.models.first().copied()
    }
}

fn read_entities(data: &[u8], lump: Lump) -> Result<String> {
    let raw = lump_slice(data, lump, "entities")?;
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    Ok(String::from_utf8_lossy(&raw[..end]).into_owned())
}

fn read_models(data: &[u8], lump: Lump) -> Result<Vec<Model>> {
    let count = check_stride(lump, "models", 64)?;
    let mut cur = Cursor::new(lump_slice(data, lump, "models")?);
    let mut models = Vec::with_capacity(count);
    for _ in 0..count {
        let mins = cur.vec3()?;
        let maxs = cur.vec3()?;
        let origin = cur.vec3()?;
        cur.skip(16)?; // headnode[4]
        cur.skip(4)?; // visleafs
        let first_face = cur.i32()?;
        let num_faces = cur.i32()?;
        models.push(Model { mins, maxs, origin, first_face, num_faces });
    }
    Ok(models)
}

/// Lump de texturas: tabela de offsets + miptex. Offset -1 marca entrada ausente.
///
/// Devolve a lista compacta (só entradas com bloco no BSP) e, alinhada à tabela
/// crua, o nome de cada textura — inclusive as que vêm de WAD (mip ausente), o
/// que é o que permite resolver o pixel no WAD lá fora.
fn read_textures(data: &[u8], lump: Lump) -> Result<(Vec<Texture>, Vec<String>)> {
    if lump.length == 0 {
        return Ok((Vec::new(), Vec::new()));
    }
    let raw = lump_slice(data, lump, "textures")?;
    let mut cur = Cursor::new(raw);
    let count = cur.u32()? as usize;
    // Cada entrada é um i32; a tabela precisa caber no lump.
    if 4 + count * 4 > raw.len() {
        return Err(BspError::LumpOutOfBounds {
            lump: "textures",
            offset: lump.offset,
            len: 4 + count * 4,
            file: raw.len(),
        });
    }
    let mut offsets = Vec::with_capacity(count);
    for _ in 0..count {
        offsets.push(cur.i32()?);
    }

    let mut textures = Vec::with_capacity(count);
    let mut raw_texture_name = Vec::with_capacity(count);
    for offset in offsets {
        if offset < 0 {
            raw_texture_name.push(String::new());
            continue;
        }
        let at = offset as usize;
        let Some(entry) = raw.get(at..) else {
            raw_texture_name.push(String::new());
            continue;
        };
        let mut mip = Cursor::new(entry);
        let name = mip.fixed_str(16).unwrap_or_default();
        // entrada truncada perto do fim do lump: mantém o vetor alinhado à tabela crua,
        // senão todo texindex seguinte aponta pro nome errado.
        let (Ok(width), Ok(height), Ok(first_offset)) = (mip.u32(), mip.u32(), mip.u32()) else {
            raw_texture_name.push(String::new());
            continue;
        };
        // offset de pixel 0 = textura vem de WAD externo; != 0 = embutida no BSP.
        textures.push(Texture { name: name.clone(), width, height, embedded: first_offset != 0 });
        raw_texture_name.push(name);
    }
    Ok((textures, raw_texture_name))
}

/// Imagem de uma textura, decodificada e comprimida, pronta para o WebView.
#[derive(Debug, Clone)]
pub struct TextureImage {
    pub name: String,
    /// PNG em `data:image/png;base64,...` — o `TextureLoader` do Three.js lê direto.
    pub png: String,
    pub width: u32,
    pub height: u32,
}

/// Extrai os pixels do mip0 de uma textura e devolve como PNG.
///
/// `texindex` é o índice **cru** da tabela de texturas (mesmo número que o
/// texinfo guarda), não a posição na lista compactada — que pula WADs externos.
/// Textura com offset `-1` (vem de WAD) ou com mip ausente devolve `None`.
pub fn texture_image(data: &[u8], lump: Lump, texindex: usize) -> Option<TextureImage> {
    if lump.length == 0 {
        return None;
    }
    let raw = lump_slice(data, lump, "textures").ok()?;
    let mut cur = Cursor::new(raw);
    let count = cur.u32().ok()? as usize;
    if texindex >= count {
        return None;
    }
    // Anda até a entrada `texindex` da tabela de offsets. `cur` já está em 4
    // (logo depois do `count`), então falta andar só `texindex * 4` — não
    // `4 + texindex * 4`: esse `+4` a mais lia a entrada seguinte da tabela
    // (textura 0 saía com os pixels da textura 1, e a última textura saía
    // vazia ou com lixo). Bug real, não só de transparência.
    cur.skip(texindex * 4).ok()?;
    let offset = cur.i32().ok()?;
    if offset < 0 {
        return None;
    }
    let at = offset as usize;
    let entry = raw.get(at..)?;
    let mut mip = Cursor::new(entry);
    let name = mip.fixed_str(16).ok()?;
    let width = mip.u32().ok()?;
    let height = mip.u32().ok()?;
    let off0 = mip.u32().ok()?;
    if off0 == 0 {
        return None; // mip ausente → textura de WAD externo
    }

    let w = width as usize;
    let h = height as usize;
    let need = w.checked_mul(h)?;
    let mip0 = at.checked_add(off0 as usize)?;
    let pixels = raw.get(mip0..mip0 + need)?.to_vec();

    // Paleta de 256 cores termina o lump: os bytes do mip0 são índices nela.
    if raw.len() < 768 {
        return None;
    }
    let palette = &raw[raw.len() - 768..];

    // Só textura `{`-prefixada usa o índice 255 como buraco (convenção do
    // Quake/GoldSrc — grade, cerca, vidro); numa textura comum é só mais uma cor.
    let transparent = name.starts_with('{');
    let rgba = palette::decode_indexed(&pixels, palette, transparent)?;

    let png = png_encode(w, h, &rgba)?;
    let png = base64::engine::general_purpose::STANDARD.encode(&png);
    Some(TextureImage {
        name,
        png: format!("data:image/png;base64,{png}"),
        width,
        height,
    })
}

/// `data:image/png;base64,...` a partir de pixels RGBA. Usado pela textura do
/// BSP, pelos WADs e pelo skybox — tudo vira PNG para o `TextureLoader`.
pub(crate) fn rgba_png(width: usize, height: usize, rgba: &[u8]) -> Option<String> {
    let png = png_encode(width, height, rgba)?;
    let png = base64::engine::general_purpose::STANDARD.encode(&png);
    Some(format!("data:image/png;base64,{png}"))
}

fn png_encode(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    let stride = width.checked_mul(4)?;
    let mut scan = Vec::with_capacity((stride + 1) * height);
    for y in 0..height {
        scan.push(0); // filtro "None" para a linha inteira
        let row = y.checked_mul(stride)?;
        scan.extend_from_slice(rgba.get(row..row + stride)?);
    }
    let idat = zlib_compress(&scan);

    let mut out = Vec::with_capacity(idat.len() + 64);
    out.extend_from_slice(&[0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type: RGBA
    ihdr.extend_from_slice(&[0, 0, 0]); // compression, filter, interlace
    write_png_chunk(&mut out, b"IHDR", &ihdr)?;
    write_png_chunk(&mut out, b"IDAT", &idat)?;
    write_png_chunk(&mut out, b"IEND", &[])?;
    Some(out)
}

fn write_png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Option<()> {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(kind.iter().copied().chain(data.iter().copied()));
    out.extend_from_slice(&crc.to_be_bytes());
    Some(())
}

/// zlib (RFC 1950) via `flate2`: comprime de verdade, então um mapa com dezenas
/// de texturas não vira dezenas de MB no IPC.
fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = std::io::Write::write_all(&mut enc, data);
    enc.finish().unwrap_or_default()
}

fn crc32(data: impl Iterator<Item = u8>) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
