pub mod entities;
pub mod light;
pub mod palette;
pub mod reader;
pub mod render;
pub mod sky;
pub mod tree;
pub mod wad;

use base64::Engine as _;
use reader::{BspError, Cursor, Result};
use serde::Serialize;

/// BSP do GoldSrc (Half-Life, Counter-Strike 1.6).
pub const GOLDSRC_VERSION: i32 = 30;
/// BSP do Quake 1 (e Half-Life alpha): mesmo layout de lumps, mas sem paleta
/// embutida nas texturas e com lightmap monocromático (1 byte por texel).
pub const QUAKE_VERSION: i32 = 29;
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Lump {
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Face {
    pub first_edge: i32,
    pub num_edges: u16,
    pub texinfo: u16,
    /// offset do lightmap no lump de iluminação; `-1` = face sem lightmap
    pub lightofs: i32,
    /// estilos de luz (0 = luz estática normal, 255 = sem estilo)
    pub styles: [u8; 4],
}

/// Plano do BSP (`dplane_t`): normal, distância e eixo dominante.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Plane {
    pub normal: [f32; 3],
    pub dist: f32,
}

/// Nó da árvore BSP (`dnode_t`): filhos negativos são folhas (`-1 - filho`).
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Node {
    pub plane: i32,
    pub children: [i16; 2],
}

/// Folha da árvore BSP (`dleaf_t`).
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Leaf {
    /// -1 vazio, -2 sólido, -3 água, -4 lodo, -5 lava, -6 céu…
    pub contents: i32,
    /// offset da linha de visibilidade (PVS) comprimida; `-1` = sem vis
    pub visofs: i32,
    pub first_marksurface: u16,
    pub num_marksurfaces: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct Texture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// textura embutida no BSP (não depende de WAD externo)
    pub embedded: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Model {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub origin: [f32; 3],
    /// raiz da árvore do hull 0 (visual) — de onde se classifica um ponto
    pub headnode: i32,
    /// folhas visíveis do modelo (define o tamanho da linha de PVS)
    pub visleafs: i32,
    pub first_face: i32,
    pub num_faces: i32,
}

/// Geometria e metadados de um BSP, já validados.
#[derive(Debug, Clone, Default)]
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
    /// flags de cada texinfo (bit 0 = `TEX_SPECIAL`: céu/água, sem lightmap)
    pub texinfo_flags: Vec<u32>,
    pub textures: Vec<Texture>,
    /// nome de cada textura da tabela crua (índice do texinfo). Vazio quando a
    /// entrada não existe no BSP (`offset -1`) — o nome vive no WAD.
    pub raw_texture_name: Vec<String>,
    pub models: Vec<Model>,
    pub planes: Vec<Plane>,
    pub nodes: Vec<Node>,
    pub leaves: Vec<Leaf>,
    pub marksurfaces: Vec<u16>,
    /// lump de visibilidade (PVS comprimido por RLE de zeros)
    pub visibility: Vec<u8>,
    /// lump de iluminação (RGB por texel no GoldSrc, 1 byte no Quake)
    pub lighting: Vec<u8>,
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
        if version != GOLDSRC_VERSION && version != QUAKE_VERSION {
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
            let styles = [cur.u8()?, cur.u8()?, cur.u8()?, cur.u8()?];
            let lightofs = cur.i32()?;
            faces.push(Face { first_edge, num_edges, texinfo, lightofs, styles });
        }

        let tin = lumps[LUMP_TEXINFO];
        let count = check_stride(tin, "texinfo", 40)?;
        let mut cur = Cursor::new(lump_slice(data, tin, "texinfo")?);
        let mut texinfo_miptex = Vec::with_capacity(count);
        let mut texinfo_vecs = Vec::with_capacity(count);
        let mut texinfo_flags = Vec::with_capacity(count);
        for _ in 0..count {
            let s_axis = cur.vec4()?;
            let t_axis = cur.vec4()?;
            texinfo_vecs.push([s_axis, t_axis]);
            texinfo_miptex.push(cur.u32()?);
            texinfo_flags.push(cur.u32()?);
        }

        let (textures, raw_texture_name) = read_textures(data, lumps[LUMP_TEXTURES])?;
        let models = read_models(data, lumps[LUMP_MODELS])?;

        // Lumps de árvore/visibilidade/luz só alimentam diagnóstico e o viewer:
        // arquivo com um deles torto ainda abre (planta e 3D seguem funcionando).
        let planes = read_planes(data, lumps[LUMP_PLANES]).unwrap_or_default();
        let nodes = read_nodes(data, lumps[LUMP_NODES]).unwrap_or_default();
        let leaves = read_leaves(data, lumps[LUMP_LEAVES]).unwrap_or_default();
        let marksurfaces = read_marksurfaces(data, lumps[LUMP_MARKSURFACES]).unwrap_or_default();
        let visibility = lump_slice(data, lumps[LUMP_VISIBILITY], "visibility").map(<[u8]>::to_vec).unwrap_or_default();
        let lighting = lump_slice(data, lumps[LUMP_LIGHTING], "lighting").map(<[u8]>::to_vec).unwrap_or_default();

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
            texinfo_flags,
            textures,
            raw_texture_name,
            models,
            planes,
            nodes,
            leaves,
            marksurfaces,
            visibility,
            lighting,
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

    /// BSP do Quake (v29): sem paleta embutida e lightmap de 1 canal.
    pub fn is_quake(&self) -> bool {
        self.version == QUAKE_VERSION
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
        let headnode = cur.i32()?; // headnode[0]: hull visual
        cur.skip(12)?; // headnode[1..4]: hulls de colisão
        let visleafs = cur.i32()?;
        let first_face = cur.i32()?;
        let num_faces = cur.i32()?;
        models.push(Model { mins, maxs, origin, headnode, visleafs, first_face, num_faces });
    }
    Ok(models)
}

fn read_planes(data: &[u8], lump: Lump) -> Result<Vec<Plane>> {
    let count = check_stride(lump, "planes", 20)?;
    let mut cur = Cursor::new(lump_slice(data, lump, "planes")?);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let normal = cur.vec3()?;
        let dist = cur.f32()?;
        cur.skip(4)?; // type
        out.push(Plane { normal, dist });
    }
    Ok(out)
}

fn read_nodes(data: &[u8], lump: Lump) -> Result<Vec<Node>> {
    let count = check_stride(lump, "nodes", 24)?;
    let mut cur = Cursor::new(lump_slice(data, lump, "nodes")?);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let plane = cur.i32()?;
        let children = [cur.i16()?, cur.i16()?];
        cur.skip(12)?; // mins[3], maxs[3] (i16)
        cur.skip(4)?; // firstface, numfaces
        out.push(Node { plane, children });
    }
    Ok(out)
}

fn read_leaves(data: &[u8], lump: Lump) -> Result<Vec<Leaf>> {
    let count = check_stride(lump, "leaves", 28)?;
    let mut cur = Cursor::new(lump_slice(data, lump, "leaves")?);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let contents = cur.i32()?;
        let visofs = cur.i32()?;
        cur.skip(12)?; // mins[3], maxs[3] (i16)
        let first_marksurface = cur.u16()?;
        let num_marksurfaces = cur.u16()?;
        cur.skip(4)?; // ambient_level[4]
        out.push(Leaf { contents, visofs, first_marksurface, num_marksurfaces });
    }
    Ok(out)
}

fn read_marksurfaces(data: &[u8], lump: Lump) -> Result<Vec<u16>> {
    let count = check_stride(lump, "marksurfaces", 2)?;
    let mut cur = Cursor::new(lump_slice(data, lump, "marksurfaces")?);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(cur.u16()?);
    }
    Ok(out)
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
#[cfg(test)]
pub fn texture_image(data: &[u8], lump: Lump, texindex: usize) -> Option<TextureImage> {
    texture_image_for(data, lump, texindex, false)
}

/// Variante que sabe do Quake: o miptex v29 não carrega paleta própria (o jogo
/// usa uma global, que não é distribuída com o mapa), então os pixels viram
/// tons de cinza — dá para reconhecer o padrão da textura, mas não a cor.
pub fn texture_image_for(data: &[u8], lump: Lump, texindex: usize, quake: bool) -> Option<TextureImage> {
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
    let gray_palette: Vec<u8>;
    let palette: &[u8] = if quake {
        gray_palette = (0..=255u8).flat_map(|v| [v, v, v]).collect();
        &gray_palette
    } else {
        if raw.len() < 768 {
            return None;
        }
        &raw[raw.len() - 768..]
    };

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
