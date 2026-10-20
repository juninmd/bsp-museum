//! Navegador de `.wad` (WAD2 do Quake / WAD3 do Half-Life): listar o conteúdo, ver uma
//! textura (mip0 e, se pedido, mips 1-3) e descobrir quais mapas usam cada textura.
//!
//! Reaproveita as convenções de `bsp/wad.rs` (diretório de 32 bytes por lump, paleta de 256
//! cores fechando o lump de textura, transparência pelo prefixo `{`) sem alterar a API dele:
//! lá o WAD é carregado inteiro para resolver texturas de um mapa; aqui o arquivo é lido por
//! `seek` (um `cstrike.wad` tem ~20 MB e o navegador só quer o diretório ou um lump).
//!
//! Tipos de lump (byte `type` do diretório):
//!
//! | código | WAD3 (Half-Life)        | WAD2 (Quake)            | aqui      |
//! |--------|-------------------------|-------------------------|-----------|
//! | `0x40` | decal (`tempdecal.wad`) | paleta                  | decal / palette |
//! | `0x42` | qpic (figura simples)   | qpic                    | qpic      |
//! | `0x43` | miptex                  | —                       | miptex    |
//! | `0x44` | —                       | miptex (sem paleta)     | miptex    |
//! | `0x45` | —                       | figura do console       | console   |
//! | `0x46` | fonte (256 glifos)      | —                       | font      |
//!
//! Fonte, console e paleta só são **listados** (o tipo é devolvido, nada é decodificado).
//! O arquivo é tratado como hostil: tetos de lumps/bytes, leitura checada, nunca panic.

use crate::bsp::palette::decode_indexed;
use crate::bsp::reader::{BspError, Cursor};
use crate::bsp::{rgba_png, Bsp, LUMP_COUNT, LUMP_TEXTURES};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// WAD maior que isto não é WAD de mod (o `cstrike.wad` do jogo tem ~10 MB).
pub const MAX_WAD_BYTES: u64 = 512 * 1024 * 1024;
/// Entradas no diretório (cada uma ocupa 32 bytes).
pub const MAX_LUMPS: usize = 100_000;
/// Um lump de textura a decodificar passa disto? Está mentindo (4096² já são 16 MB).
pub const MAX_LUMP_BYTES: usize = 16 * 1024 * 1024;
/// Lado máximo de textura a decodificar.
pub const MAX_SIDE: usize = 4096;

/// Teto de mapas lidos em `wad_usage` e de nomes de mapa listados por textura.
pub const MAX_MAPS_SCANNED: usize = 5000;
pub const MAX_MAPS_PER_TEXTURE: usize = 100;
/// Texturas num único BSP (o HLCSG limita a 512; acima de muito mais é arquivo quebrado).
const MAX_BSP_TEXTURES: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WadKind {
    Miptex,
    Decal,
    Qpic,
    Font,
    Console,
    Palette,
    Other,
}

impl WadKind {
    fn from_code(code: u8, magic: &[u8; 4]) -> Self {
        match code {
            0x43 | 0x44 => WadKind::Miptex,
            0x40 if magic == b"WAD2" => WadKind::Palette,
            0x40 => WadKind::Decal,
            0x42 => WadKind::Qpic,
            0x45 => WadKind::Console,
            0x46 => WadKind::Font,
            _ => WadKind::Other,
        }
    }

    /// textura com cadeia de mips (`name, w, h, offsets[4]`)
    fn is_texture(self) -> bool {
        matches!(self, WadKind::Miptex | WadKind::Decal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WadError {
    Io(String),
    Truncated(BspError),
    BadMagic,
    Invalid(String),
}

impl fmt::Display for WadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WadError::Io(e) => write!(f, "erro de leitura do WAD: {e}"),
            WadError::Truncated(e) => write!(f, "WAD truncado: {e}"),
            WadError::BadMagic => write!(f, "não é um WAD (assinatura WAD2/WAD3 ausente)"),
            WadError::Invalid(msg) => write!(f, "WAD inválido: {msg}"),
        }
    }
}

impl std::error::Error for WadError {}

impl From<BspError> for WadError {
    fn from(e: BspError) -> Self {
        WadError::Truncated(e)
    }
}

impl From<std::io::Error> for WadError {
    fn from(e: std::io::Error) -> Self {
        WadError::Io(e.to_string())
    }
}

/// Uma entrada do diretório do WAD.
#[derive(Debug, Clone, Serialize)]
pub struct WadEntry {
    pub name: String,
    pub kind: WadKind,
    /// byte `type` cru do diretório (`0x43`, `0x40`, `0x42`, `0x46`...)
    pub type_code: u8,
    /// 0 quando o tipo não tem dimensões legíveis (paleta, desconhecido, comprimido)
    pub width: u32,
    pub height: u32,
    /// textura de miptex/decal cujo nome começa com `{` (índice 255 = buraco)
    pub transparent: bool,
    /// bytes do lump no arquivo
    pub size: u32,
    /// lump comprimido (LZSS): o diretório lista, mas não é decodificado
    pub compressed: bool,
    /// `wad_texture` consegue gerar PNG deste lump (miptex/decal/qpic, sem compressão)
    pub decodable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WadListing {
    pub path: String,
    /// nome do arquivo (`halflife.wad`)
    pub name: String,
    pub file_size: u64,
    /// `WAD2` ou `WAD3`
    pub magic: String,
    /// todas as entradas do diretório
    pub lump_count: usize,
    /// quantas são texturas com mips (miptex + decal)
    pub texture_count: usize,
    pub entries: Vec<WadEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WadMip {
    /// 1, 2 ou 3 (o mip0 está no `png` da textura)
    pub level: u8,
    pub width: u32,
    pub height: u32,
    pub png: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WadTexture {
    pub name: String,
    pub kind: WadKind,
    pub width: u32,
    pub height: u32,
    pub transparent: bool,
    /// mip0: `data:image/png;base64,...`
    pub png: String,
    /// mips 1-3 presentes no lump (vazio se não foram pedidos)
    pub mips: Vec<WadMip>,
}

/// Textura do WAD e os mapas da pasta que a usam.
#[derive(Debug, Clone, Serialize)]
pub struct TextureUsage {
    pub name: String,
    /// quantos mapas usam (exato, mesmo que `maps` esteja cortado)
    pub map_count: usize,
    /// nomes dos mapas (sem pasta nem `.bsp`), ordenados, no máximo `MAX_MAPS_PER_TEXTURE`
    pub maps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WadUsage {
    pub wad: String,
    pub maps_dir: String,
    /// BSPs cujo índice de texturas foi lido
    pub maps_scanned: usize,
    /// BSPs que não abriram / têm cabeçalho ou lump de texturas inválido
    pub maps_unreadable: usize,
    /// havia mais de `MAX_MAPS_SCANNED` mapas: só os primeiros (por caminho) foram lidos
    pub truncated: bool,
    /// uma entrada por textura do WAD, ordenada por nome (`map_count` 0 = não usada)
    pub textures: Vec<TextureUsage>,
    pub unused_count: usize,
}

// ---------------------------------------------------------------------------- diretório

struct RawEntry {
    pos: u64,
    disk_size: u32,
    type_code: u8,
    compression: u8,
    name: String,
}

struct Dir {
    magic: [u8; 4],
    entries: Vec<RawEntry>,
}

fn read_at<R: Read + Seek>(r: &mut R, pos: u64, len: usize) -> Result<Vec<u8>, WadError> {
    r.seek(SeekFrom::Start(pos))?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Lê cabeçalho + diretório. `file_len` é o tamanho real do arquivo (para recusar diretório fora dele).
fn read_dir<R: Read + Seek>(r: &mut R, file_len: u64) -> Result<Dir, WadError> {
    if file_len < 12 {
        return Err(BspError::TooSmall { need: 12, have: file_len as usize }.into());
    }
    let head = read_at(r, 0, 12)?;
    let mut cur = Cursor::new(&head);
    let magic: [u8; 4] = cur.bytes(4)?.try_into().map_err(|_| WadError::BadMagic)?;
    if &magic != b"WAD2" && &magic != b"WAD3" {
        return Err(WadError::BadMagic);
    }
    let count = cur.u32()? as usize;
    let info_ofs = cur.u32()? as u64;
    if count > MAX_LUMPS {
        return Err(WadError::Invalid(format!("{count} lumps no diretório (máximo {MAX_LUMPS})")));
    }
    let dir_len = count as u64 * 32;
    if info_ofs.checked_add(dir_len).map_or(true, |end| end > file_len) {
        return Err(WadError::Invalid(format!(
            "diretório em {info_ofs}..{} fora do arquivo de {file_len} bytes",
            info_ofs.saturating_add(dir_len)
        )));
    }
    let raw = read_at(r, info_ofs, dir_len as usize)?;
    let mut entries = Vec::with_capacity(count);
    for chunk in raw.chunks_exact(32) {
        let mut c = Cursor::new(chunk);
        let pos = c.u32()? as u64;
        let disk_size = c.u32()?;
        let _size = c.u32()?;
        let type_code = c.u8()?;
        let compression = c.u8()?;
        c.skip(2)?;
        let name = c.fixed_str(16)?;
        entries.push(RawEntry { pos, disk_size, type_code, compression, name });
    }
    Ok(Dir { magic, entries })
}

/// Até `max` bytes do começo do lump, sem passar do fim do arquivo (lump que aponta pra fora
/// devolve menos — ou nada — em vez de falhar).
fn lump_prefix<R: Read + Seek>(r: &mut R, e: &RawEntry, file_len: u64, max: usize) -> Result<Vec<u8>, WadError> {
    if e.pos >= file_len {
        return Ok(Vec::new());
    }
    let n = (e.disk_size as u64).min(file_len - e.pos).min(max as u64) as usize;
    read_at(r, e.pos, n)
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Monta a listagem a partir de qualquer leitor com `seek` (arquivo ou memória).
pub fn list_from_reader<R: Read + Seek>(
    r: &mut R,
    file_len: u64,
    path: &str,
    name: &str,
) -> Result<WadListing, WadError> {
    let dir = read_dir(r, file_len)?;
    let mut entries = Vec::with_capacity(dir.entries.len());
    let mut texture_count = 0usize;
    for raw in &dir.entries {
        let kind = WadKind::from_code(raw.type_code, &dir.magic);
        let compressed = raw.compression != 0;
        let (width, height) = if compressed || matches!(kind, WadKind::Palette | WadKind::Other) {
            (0, 0)
        } else {
            let head = lump_prefix(r, raw, file_len, 24)?;
            if kind.is_texture() {
                (u32_at(&head, 16), u32_at(&head, 20))
            } else {
                // qpic, console e fonte começam por `i32 largura, i32 altura`
                (u32_at(&head, 0), u32_at(&head, 4))
            }
        };
        if kind.is_texture() {
            texture_count += 1;
        }
        entries.push(WadEntry {
            name: raw.name.clone(),
            kind,
            type_code: raw.type_code,
            width,
            height,
            transparent: kind.is_texture() && raw.name.starts_with('{'),
            size: raw.disk_size,
            compressed,
            decodable: !compressed && (kind.is_texture() || kind == WadKind::Qpic),
        });
    }
    Ok(WadListing {
        path: path.to_string(),
        name: name.to_string(),
        file_size: file_len,
        magic: String::from_utf8_lossy(&dir.magic).to_string(),
        lump_count: entries.len(),
        texture_count,
        entries,
    })
}

/// Lista o conteúdo de um `.wad` (só o diretório + 24 bytes de cada lump para as dimensões).
pub fn list_wad(path: &Path) -> Result<WadListing, String> {
    let size = crate::util::check_file(path, &["wad"], MAX_WAD_BYTES)?;
    let mut file = File::open(path).map_err(|e| format!("não abriu {}: {e}", path.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    list_from_reader(&mut file, size, &path.to_string_lossy(), &name).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------- textura

/// PNG de um nível de mip a partir do lump inteiro.
fn mip_png(lump: &[u8], offset: usize, w: usize, h: usize, palette: &[u8], transparent: bool) -> Option<String> {
    if w == 0 || h == 0 {
        return None;
    }
    let pixels = lump.get(offset..offset.checked_add(w.checked_mul(h)?)?)?;
    let rgba = decode_indexed(pixels, palette, transparent)?;
    rgba_png(w, h, &rgba)
}

/// Decodifica um lump `miptex`/decal (já lido) em mip0 + mips pedidos.
fn decode_texture_lump(
    lump: &[u8],
    name: &str,
    kind: WadKind,
    type_code: u8,
    with_mips: bool,
) -> Result<WadTexture, WadError> {
    let mut cur = Cursor::new(lump);
    cur.skip(16)?;
    let width = cur.u32()?;
    let height = cur.u32()?;
    let mut offsets = [0usize; 4];
    for o in &mut offsets {
        *o = cur.u32()? as usize;
    }
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(WadError::Invalid(format!("textura {name} de {w}x{h} (máximo {MAX_SIDE} por lado)")));
    }
    if offsets[0] == 0 {
        return Err(WadError::Invalid(format!("textura {name} sem pixels no lump (mip ausente)")));
    }

    // WAD3: a paleta de 256 cores fecha o lump. WAD2 (0x44): miptex do Quake não leva paleta
    // (a global do jogo não vem no WAD), então os índices viram tons de cinza.
    let gray: Vec<u8>;
    let palette: &[u8] = if type_code == 0x44 {
        gray = (0..=255u8).flat_map(|v| [v, v, v]).collect();
        &gray
    } else {
        if lump.len() < 40 + 768 {
            return Err(WadError::Invalid(format!("lump de {name} curto demais para ter paleta")));
        }
        &lump[lump.len() - 768..]
    };

    let transparent = name.starts_with('{');
    let png = mip_png(lump, offsets[0], w, h, palette, transparent)
        .ok_or_else(|| WadError::Invalid(format!("pixels de {name} fora do lump")))?;

    let mut mips = Vec::new();
    if with_mips {
        for (level, &offset) in offsets.iter().enumerate().skip(1) {
            let (mw, mh) = (w >> level, h >> level);
            // mip que não cabe no lump é simplesmente omitido: o mip0 já é a textura
            if let Some(png) = mip_png(lump, offset, mw, mh, palette, transparent) {
                mips.push(WadMip { level: level as u8, width: mw as u32, height: mh as u32, png });
            }
        }
    }
    Ok(WadTexture { name: name.to_string(), kind, width, height, transparent, png, mips })
}

/// Gera o PNG de uma textura (case-insensitive) a partir de um leitor com `seek`.
pub fn texture_from_reader<R: Read + Seek>(
    r: &mut R,
    file_len: u64,
    name: &str,
    with_mips: bool,
) -> Result<WadTexture, WadError> {
    let dir = read_dir(r, file_len)?;
    let wanted = name.trim().to_ascii_lowercase();
    let raw = dir
        .entries
        .iter()
        .find(|e| e.name.to_ascii_lowercase() == wanted)
        .ok_or_else(|| WadError::Invalid(format!("textura {name} não existe neste WAD")))?;
    let kind = WadKind::from_code(raw.type_code, &dir.magic);
    if raw.compression != 0 {
        return Err(WadError::Invalid(format!("lump {} está comprimido (não suportado)", raw.name)));
    }
    if !(kind.is_texture() || kind == WadKind::Qpic) {
        return Err(WadError::Invalid(format!(
            "lump {} é do tipo {:?} (0x{:02X}); só miptex, decal e qpic são decodificados",
            raw.name, kind, raw.type_code
        )));
    }
    if raw.disk_size as usize > MAX_LUMP_BYTES {
        return Err(WadError::Invalid(format!("lump {} com {} bytes (máximo {MAX_LUMP_BYTES})", raw.name, raw.disk_size)));
    }
    if raw.pos.checked_add(raw.disk_size as u64).map_or(true, |end| end > file_len) {
        return Err(WadError::Invalid(format!("lump {} aponta para fora do arquivo", raw.name)));
    }
    let lump = read_at(r, raw.pos, raw.disk_size as usize)?;
    if kind == WadKind::Qpic {
        let (png, width, height) = crate::image::qpic_png(&lump).map_err(|e| WadError::Invalid(e.to_string()))?;
        return Ok(WadTexture { name: raw.name.clone(), kind, width, height, transparent: false, png, mips: Vec::new() });
    }
    decode_texture_lump(&lump, &raw.name, kind, raw.type_code, with_mips)
}

/// PNG do mip0 da textura `name` do WAD (e dos mips 1-3 se `with_mips`).
pub fn wad_texture(path: &Path, name: &str, with_mips: bool) -> Result<WadTexture, String> {
    let size = crate::util::check_file(path, &["wad"], MAX_WAD_BYTES)?;
    let mut file = File::open(path).map_err(|e| format!("não abriu {}: {e}", path.display()))?;
    texture_from_reader(&mut file, size, name, with_mips).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------- uso

/// Limites da varredura de `wad_usage` (parametrizados para os testes).
#[derive(Debug, Clone, Copy)]
pub struct UsageLimits {
    pub max_maps: usize,
    pub max_maps_per_texture: usize,
}

impl Default for UsageLimits {
    fn default() -> Self {
        Self { max_maps: MAX_MAPS_SCANNED, max_maps_per_texture: MAX_MAPS_PER_TEXTURE }
    }
}

/// Nomes (minúsculos, sem repetição) do lump de texturas de um BSP, lendo só a tabela de
/// offsets e 16 bytes por textura — o mesmo cuidado da varredura do catálogo, que não lê o mapa
/// inteiro. `None` = arquivo ilegível ou lump torto.
fn bsp_texture_names(path: &Path) -> Option<Vec<String>> {
    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let mut header = [0u8; 4 + LUMP_COUNT * 8];
    file.read_exact(&mut header).ok()?;
    let (_, lumps) = Bsp::header(&header).ok()?;
    let lump = lumps[LUMP_TEXTURES];
    if lump.length == 0 {
        return Some(Vec::new());
    }
    if (lump.offset as u64).checked_add(lump.length as u64)? > file_len || lump.length < 4 {
        return None;
    }
    let count = u32::from_le_bytes(read_at(&mut file, lump.offset as u64, 4).ok()?.try_into().ok()?) as usize;
    if count > MAX_BSP_TEXTURES || 4 + count * 4 > lump.length {
        return None;
    }
    let table = read_at(&mut file, lump.offset as u64 + 4, count * 4).ok()?;
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for chunk in table.chunks_exact(4) {
        let off = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        // -1 = entrada ausente; miptex que não cabe no lump = lixo
        if off < 0 || off as usize + 16 > lump.length {
            continue;
        }
        let raw = read_at(&mut file, lump.offset as u64 + off as u64, 16).ok()?;
        let name = Cursor::new(&raw).fixed_str(16).ok()?.to_ascii_lowercase();
        if !name.is_empty() && seen.insert(name.clone()) {
            names.push(name);
        }
    }
    Some(names)
}

fn map_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Quais mapas de `maps_dir` (recursivo) usam cada textura de `wad_path`, pelo índice de nomes do
/// lump de texturas dos BSPs. Texturas externas e embutidas contam igual: o nome basta.
pub fn wad_usage(wad_path: &Path, maps_dir: &Path) -> Result<WadUsage, String> {
    wad_usage_with(wad_path, maps_dir, UsageLimits::default())
}

pub fn wad_usage_with(wad_path: &Path, maps_dir: &Path, limits: UsageLimits) -> Result<WadUsage, String> {
    use std::path::Component;
    if maps_dir.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("caminho com `..` não é aceito: {}", maps_dir.display()));
    }
    if !maps_dir.is_dir() {
        return Err(format!("não é uma pasta: {}", maps_dir.display()));
    }
    let listing = list_wad(wad_path)?;
    // minúsculo -> nome como está no WAD
    let wad_names: HashMap<String, String> = listing
        .entries
        .iter()
        .filter(|e| e.kind.is_texture() && !e.name.is_empty())
        .map(|e| (e.name.to_ascii_lowercase(), e.name.clone()))
        .collect();

    let mut files: Vec<PathBuf> = crate::catalog::find_bsp_files(maps_dir);
    let truncated = files.len() > limits.max_maps;
    files.truncate(limits.max_maps);

    // mesmo esquema de `catalog::scan`: fatias por thread, sem dependência externa
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let read_chunk = |slice: &[PathBuf]| -> Vec<(String, Option<Vec<String>>)> {
        slice.iter().map(|p| (map_name(p), bsp_texture_names(p))).collect()
    };
    let per_map: Vec<(String, Option<Vec<String>>)> = if files.len() < 2 || workers < 2 {
        read_chunk(&files)
    } else {
        let chunk = files.len().div_ceil(workers);
        let mut parts: Vec<Vec<(String, Option<Vec<String>>)>> = Vec::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = files.chunks(chunk).map(|slice| scope.spawn(move || read_chunk(slice))).collect();
            for handle in handles {
                parts.push(handle.join().unwrap_or_default());
            }
        });
        parts.into_iter().flatten().collect()
    };

    let mut users: HashMap<&str, Vec<String>> = HashMap::new();
    let (mut scanned, mut unreadable) = (0usize, 0usize);
    for (map, names) in &per_map {
        let Some(names) = names else {
            unreadable += 1;
            continue;
        };
        scanned += 1;
        for n in names {
            if let Some(key) = wad_names.get_key_value(n.as_str()) {
                users.entry(key.0.as_str()).or_default().push(map.clone());
            }
        }
    }

    let mut textures: Vec<TextureUsage> = wad_names
        .iter()
        .map(|(lower, original)| {
            let mut maps = users.get(lower.as_str()).cloned().unwrap_or_default();
            maps.sort();
            let map_count = maps.len();
            maps.truncate(limits.max_maps_per_texture);
            TextureUsage { name: original.clone(), map_count, maps }
        })
        .collect();
    textures.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()).then(a.name.cmp(&b.name)));
    let unused_count = textures.iter().filter(|t| t.map_count == 0).count();

    Ok(WadUsage {
        wad: wad_path.to_string_lossy().to_string(),
        maps_dir: maps_dir.to_string_lossy().to_string(),
        maps_scanned: scanned,
        maps_unreadable: unreadable,
        truncated,
        textures,
        unused_count,
    })
}

// ---------------------------------------------------------------------------- testes

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::decode_png_rgba;
    use std::io::Cursor as MemCursor;

    /// Paleta de teste: índice i -> (i, 100, 255 - i), exceto o 255 -> (250, 5, 5).
    fn palette() -> Vec<u8> {
        let mut p: Vec<u8> = (0..=255u8).flat_map(|i| [i, 100, 255 - i]).collect();
        p[765..768].copy_from_slice(&[250, 5, 5]);
        p
    }

    fn name16(name: &str) -> [u8; 16] {
        let mut raw = [0u8; 16];
        let b = name.as_bytes();
        raw[..b.len().min(15)].copy_from_slice(&b[..b.len().min(15)]);
        raw
    }

    /// Lump miptex completo: cabeçalho de 40 bytes, 4 mips (pixels = `fill(i, x, y)`), u16=256 e paleta.
    /// `with_palette = false` imita o miptex do Quake (WAD2).
    fn miptex(name: &str, w: u32, h: u32, fill: impl Fn(usize, usize) -> u8, with_palette: bool) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&name16(name));
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        let mut offset = 40u32;
        let mut mip_bytes = Vec::new();
        for level in 0..4u32 {
            out.extend_from_slice(&offset.to_le_bytes());
            let n = ((w >> level) * (h >> level)) as usize;
            offset += n as u32;
            for i in 0..n {
                mip_bytes.push(fill(level as usize, i));
            }
        }
        out.extend_from_slice(&mip_bytes);
        if with_palette {
            out.extend_from_slice(&256u16.to_le_bytes());
            out.extend_from_slice(&palette());
        }
        out
    }

    fn qpic(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(px);
        out.extend_from_slice(&256u16.to_le_bytes());
        out.extend_from_slice(&palette());
        out
    }

    struct Lump {
        name: &'static str,
        code: u8,
        compression: u8,
        data: Vec<u8>,
    }

    fn lump(name: &'static str, code: u8, data: Vec<u8>) -> Lump {
        Lump { name, code, compression: 0, data }
    }

    /// WAD byte a byte: cabeçalho, lumps em sequência, diretório no fim.
    fn wad(magic: &[u8; 4], lumps: &[Lump]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(magic);
        out.extend_from_slice(&(lumps.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // preenchido depois
        let mut positions = Vec::new();
        for l in lumps {
            positions.push(out.len() as u32);
            out.extend_from_slice(&l.data);
        }
        let info = out.len() as u32;
        out[8..12].copy_from_slice(&info.to_le_bytes());
        for (l, pos) in lumps.iter().zip(positions) {
            out.extend_from_slice(&pos.to_le_bytes());
            out.extend_from_slice(&(l.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(l.data.len() as u32).to_le_bytes());
            out.push(l.code);
            out.push(l.compression);
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(&name16(l.name));
        }
        out
    }

    fn list(bytes: &[u8]) -> Result<WadListing, WadError> {
        list_from_reader(&mut MemCursor::new(bytes), bytes.len() as u64, "mem.wad", "mem.wad")
    }

    fn tex(bytes: &[u8], name: &str, mips: bool) -> Result<WadTexture, WadError> {
        texture_from_reader(&mut MemCursor::new(bytes), bytes.len() as u64, name, mips)
    }

    fn sample_wad() -> Vec<u8> {
        let mut font = Vec::new();
        font.extend_from_slice(&256u32.to_le_bytes());
        font.extend_from_slice(&128u32.to_le_bytes());
        font.extend_from_slice(&[0; 40]);
        wad(
            b"WAD3",
            &[
                // 16x16, mip i preenchido com o valor i*10 + 5 (dá para reconhecer o nível)
                lump("WALL01", 0x43, miptex("WALL01", 16, 16, |lvl, _| (lvl * 10 + 5) as u8, true)),
                // 16x16, todo 255: buraco por ser `{`
                lump("{FENCE", 0x43, miptex("{FENCE", 16, 16, |_, _| 255, true)),
                lump("{shot1", 0x40, miptex("{shot1", 16, 16, |_, _| 7, true)),
                lump("logo", 0x42, qpic(2, 1, &[1, 2])),
                lump("fonts", 0x46, font),
                lump("conpic", 0x45, qpic(1, 1, &[9])),
                lump("misterio", 0x99, vec![1, 2, 3]),
            ],
        )
    }

    #[test]
    fn lista_todos_os_tipos_com_dimensoes_e_flags() {
        let bytes = sample_wad();
        let l = list(&bytes).unwrap();
        assert_eq!(l.magic, "WAD3");
        assert_eq!(l.file_size, bytes.len() as u64);
        assert_eq!(l.lump_count, 7);
        assert_eq!(l.texture_count, 3, "miptex + decal");

        let by = |n: &str| l.entries.iter().find(|e| e.name == n).unwrap();
        let wall = by("WALL01");
        assert_eq!((wall.kind, wall.type_code, wall.width, wall.height), (WadKind::Miptex, 0x43, 16, 16));
        assert!(!wall.transparent && wall.decodable && !wall.compressed);
        assert!(by("{FENCE").transparent);
        let decal = by("{shot1");
        assert_eq!((decal.kind, decal.type_code), (WadKind::Decal, 0x40));
        assert!(decal.transparent);
        let logo = by("logo");
        assert_eq!((logo.kind, logo.width, logo.height), (WadKind::Qpic, 2, 1));
        assert!(logo.decodable && !logo.transparent);
        let font = by("fonts");
        assert_eq!((font.kind, font.type_code, font.width, font.height), (WadKind::Font, 0x46, 256, 128));
        assert!(!font.decodable);
        assert_eq!(by("conpic").kind, WadKind::Console);
        let other = by("misterio");
        assert_eq!((other.kind, other.width, other.height, other.decodable), (WadKind::Other, 0, 0, false));
    }

    #[test]
    fn serializa_com_os_nomes_do_contrato() {
        let l = list(&sample_wad()).unwrap();
        let json = serde_json::to_value(&l).unwrap();
        assert_eq!(json["magic"], "WAD3");
        assert_eq!(json["texture_count"], 3);
        assert_eq!(json["entries"][0]["kind"], "miptex");
        assert_eq!(json["entries"][0]["type_code"], 0x43);
        assert_eq!(json["entries"][4]["kind"], "font");
    }

    #[test]
    fn wad2_distingue_paleta_de_decal() {
        let bytes = wad(b"WAD2", &[lump("palette", 0x40, vec![0; 768]), lump("tex", 0x44, miptex("tex", 16, 16, |_, i| i as u8, false))]);
        let l = list(&bytes).unwrap();
        assert_eq!(l.magic, "WAD2");
        assert_eq!(l.entries[0].kind, WadKind::Palette);
        assert_eq!(l.entries[1].kind, WadKind::Miptex);
        assert_eq!(l.texture_count, 1);
        assert!(!l.entries[0].decodable);
    }

    #[test]
    fn miptex_mip0_com_a_paleta_do_lump() {
        let bytes = sample_wad();
        let t = tex(&bytes, "wall01", false).unwrap(); // case-insensitive
        assert_eq!((t.name.as_str(), t.kind, t.width, t.height, t.transparent), ("WALL01", WadKind::Miptex, 16, 16, false));
        assert!(t.mips.is_empty());
        let (w, h, px) = decode_png_rgba(&t.png);
        assert_eq!((w, h), (16, 16));
        // mip0 = índice 5 -> (5, 100, 250)
        assert_eq!(&px[0..4], &[5, 100, 250, 255]);
    }

    #[test]
    fn mips_1_a_3_saem_com_metade_das_dimensoes() {
        let bytes = sample_wad();
        let t = tex(&bytes, "WALL01", true).unwrap();
        assert_eq!(t.mips.len(), 3);
        for (m, (lvl, dim, idx)) in t.mips.iter().zip([(1u8, 8u32, 15u8), (2, 4, 25), (3, 2, 35)]) {
            assert_eq!((m.level, m.width, m.height), (lvl, dim, dim));
            let (w, h, px) = decode_png_rgba(&m.png);
            assert_eq!((w, h), (dim, dim));
            assert_eq!(&px[0..3], &[idx, 100, 255 - idx], "nível {lvl}");
        }
    }

    #[test]
    fn prefixo_chave_fura_o_indice_255_e_so_ele() {
        let bytes = sample_wad();
        let t = tex(&bytes, "{fence", true).unwrap();
        assert!(t.transparent);
        let (_, _, px) = decode_png_rgba(&t.png);
        assert_eq!(&px[0..4], &[250, 5, 5, 0]);
        // decal usa a mesma regra do miptex (índice 7 não fura)
        let d = tex(&bytes, "{shot1", false).unwrap();
        assert_eq!(d.kind, WadKind::Decal);
        let (_, _, px) = decode_png_rgba(&d.png);
        assert_eq!(&px[0..4], &[7, 100, 248, 255]);
        // sem `{`, o 255 é cor
        let bytes = wad(b"WAD3", &[lump("solida", 0x43, miptex("solida", 16, 16, |_, _| 255, true))]);
        let (_, _, px) = decode_png_rgba(&tex(&bytes, "solida", false).unwrap().png);
        assert_eq!(&px[0..4], &[250, 5, 5, 255]);
    }

    #[test]
    fn qpic_do_wad_decodifica_e_o_resto_so_lista() {
        let bytes = sample_wad();
        let t = tex(&bytes, "logo", true).unwrap();
        assert_eq!((t.kind, t.width, t.height), (WadKind::Qpic, 2, 1));
        let (_, _, px) = decode_png_rgba(&t.png);
        assert_eq!(&px[0..8], &[1, 100, 254, 255, 2, 100, 253, 255]);

        for name in ["fonts", "conpic", "misterio"] {
            let err = tex(&bytes, name, false).unwrap_err().to_string();
            assert!(err.contains("só miptex, decal e qpic"), "{name}: {err}");
        }
        assert!(tex(&bytes, "nao-existe", false).unwrap_err().to_string().contains("não existe"));
    }

    #[test]
    fn miptex_do_quake_vira_cinza() {
        let bytes = wad(b"WAD2", &[lump("tex", 0x44, miptex("tex", 16, 16, |_, _| 77, false))]);
        let t = tex(&bytes, "tex", false).unwrap();
        let (_, _, px) = decode_png_rgba(&t.png);
        assert_eq!(&px[0..4], &[77, 77, 77, 255]);
    }

    #[test]
    fn lump_comprimido_e_listado_mas_nao_decodificado() {
        let mut l = lump("zip", 0x43, miptex("zip", 16, 16, |_, _| 1, true));
        l.compression = 1;
        let bytes = wad(b"WAD3", &[l]);
        let e = &list(&bytes).unwrap().entries[0];
        assert!(e.compressed && !e.decodable);
        assert_eq!((e.width, e.height), (0, 0));
        assert!(tex(&bytes, "zip", false).unwrap_err().to_string().contains("comprimido"));
    }

    #[test]
    fn textura_externa_e_dimensao_absurda_dao_erro() {
        // offset do mip0 zerado
        let mut data = miptex("ext", 16, 16, |_, _| 1, true);
        data[24..28].copy_from_slice(&0u32.to_le_bytes());
        let bytes = wad(b"WAD3", &[lump("ext", 0x43, data)]);
        assert!(tex(&bytes, "ext", false).unwrap_err().to_string().contains("mip ausente"));
        // 100000x100000
        let mut data = miptex("big", 16, 16, |_, _| 1, true);
        data[16..20].copy_from_slice(&100_000u32.to_le_bytes());
        data[20..24].copy_from_slice(&100_000u32.to_le_bytes());
        let bytes = wad(b"WAD3", &[lump("big", 0x43, data)]);
        assert!(tex(&bytes, "big", false).is_err());
        // mip0 apontando para fora do lump
        let mut data = miptex("out", 16, 16, |_, _| 1, true);
        data[24..28].copy_from_slice(&900_000u32.to_le_bytes());
        let bytes = wad(b"WAD3", &[lump("out", 0x43, data)]);
        assert!(tex(&bytes, "out", false).is_err());
    }

    #[test]
    fn cabecalho_invalido() {
        assert_eq!(list(b"").unwrap_err(), WadError::Truncated(BspError::TooSmall { need: 12, have: 0 }));
        assert_eq!(list(b"WAD9\0\0\0\0\0\0\0\0").unwrap_err(), WadError::BadMagic);
        // diretório fora do arquivo
        let mut bytes = sample_wad();
        bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(list(&bytes), Err(WadError::Invalid(_))));
        // lumps demais
        let mut bytes = sample_wad();
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(list(&bytes), Err(WadError::Invalid(_))));
    }

    #[test]
    fn lump_apontando_para_fora_e_listado_sem_dimensoes() {
        let mut bytes = sample_wad();
        // filepos do primeiro lump do diretório -> além do arquivo
        let info = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        bytes[info..info + 4].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
        let l = list(&bytes).unwrap();
        assert_eq!((l.entries[0].width, l.entries[0].height), (0, 0));
        assert!(tex(&bytes, "WALL01", false).is_err());
    }

    #[test]
    fn truncado_em_qualquer_ponto_nao_da_panic() {
        let bytes = sample_wad();
        for cut in 0..bytes.len() {
            let slice = &bytes[..cut];
            let _ = list(slice);
            for name in ["WALL01", "{fence", "logo"] {
                let _ = tex(slice, name, true);
            }
        }
    }

    #[test]
    fn lixo_aleatorio_nao_da_panic() {
        let mut s = 0xDEAD_BEEF_CAFE_F00Du64;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for len in [0usize, 5, 12, 44, 200, 1500] {
            for _ in 0..40 {
                let mut buf: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                let _ = list(&buf);
                if len >= 12 {
                    buf[0..4].copy_from_slice(if next() % 2 == 0 { b"WAD3" } else { b"WAD2" });
                    buf[4..8].copy_from_slice(&((next() % 8) as u32).to_le_bytes());
                    buf[8..12].copy_from_slice(&((next() % (len as u64 + 40)) as u32).to_le_bytes());
                    let _ = list(&buf);
                    let _ = tex(&buf, "x", true);
                }
            }
        }
    }

    // ---- disco

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bspm-wadlist-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_wad_e_wad_texture_leem_do_disco_e_conferem_o_caminho() {
        let dir = temp_dir("disco");
        let path = dir.join("Teste.WAD");
        let bytes = sample_wad();
        std::fs::write(&path, &bytes).unwrap();
        let l = list_wad(&path).unwrap();
        assert_eq!((l.name.as_str(), l.file_size, l.texture_count), ("Teste.WAD", bytes.len() as u64, 3));
        let t = wad_texture(&path, "wall01", true).unwrap();
        assert_eq!(t.mips.len(), 3);

        let txt = dir.join("a.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert!(list_wad(&txt).unwrap_err().contains("extensão"));
        assert!(list_wad(&dir.join("..").join("Teste.WAD")).is_err());
        assert!(list_wad(&dir.join("nao-existe.wad")).is_err());
        std::fs::write(dir.join("lixo.wad"), b"isto nao e um wad, mas tem mais de doze bytes").unwrap();
        assert!(list_wad(&dir.join("lixo.wad")).unwrap_err().contains("WAD2/WAD3"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BSP mínimo: só cabeçalho + lump de texturas (nomes; `None` = entrada ausente, offset -1).
    fn bsp_with_textures(names: &[Option<&str>]) -> Vec<u8> {
        let mut tex = Vec::new();
        tex.extend_from_slice(&(names.len() as u32).to_le_bytes());
        let table_end = 4 + names.len() * 4;
        let mut body = Vec::new();
        for n in names {
            match n {
                Some(n) => {
                    tex.extend_from_slice(&((table_end + body.len()) as i32).to_le_bytes());
                    body.extend_from_slice(&name16(n));
                    body.extend_from_slice(&[0; 24]); // w, h, offsets[4]: sem pixels = textura externa
                }
                None => tex.extend_from_slice(&(-1i32).to_le_bytes()),
            }
        }
        tex.extend_from_slice(&body);

        let header_len = 4 + LUMP_COUNT * 8;
        let mut out = Vec::new();
        out.extend_from_slice(&30i32.to_le_bytes());
        for i in 0..LUMP_COUNT {
            let (off, len) = if i == LUMP_TEXTURES { (header_len, tex.len()) } else { (0, 0) };
            out.extend_from_slice(&(off as i32).to_le_bytes());
            out.extend_from_slice(&(len as i32).to_le_bytes());
        }
        out.extend_from_slice(&tex);
        out
    }

    fn usage_fixture(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = temp_dir(tag);
        let maps = root.join("maps");
        std::fs::create_dir_all(maps.join("sub")).unwrap();
        let wad_path = root.join("game.wad");
        std::fs::write(&wad_path, sample_wad()).unwrap();
        std::fs::write(maps.join("a.bsp"), bsp_with_textures(&[Some("wall01"), Some("{Fence"), Some("so_do_mapa"), None])).unwrap();
        std::fs::write(maps.join("b.bsp"), bsp_with_textures(&[Some("WALL01"), Some("wall01")])).unwrap();
        std::fs::write(maps.join("sub").join("d.bsp"), bsp_with_textures(&[Some("wall01")])).unwrap();
        std::fs::write(maps.join("quebrado.bsp"), b"nao sou um bsp").unwrap();
        std::fs::write(maps.join("vazio.bsp"), bsp_with_textures(&[])).unwrap();
        (root, maps, wad_path)
    }

    #[test]
    fn wad_usage_cruza_nomes_do_wad_com_os_bsps() {
        let (root, maps, wad_path) = usage_fixture("uso");
        let u = wad_usage(&wad_path, &maps).unwrap();
        assert_eq!(u.maps_scanned, 4, "a, b, d e vazio");
        assert_eq!(u.maps_unreadable, 1, "quebrado.bsp");
        assert!(!u.truncated);
        // uma entrada por textura (miptex + decal) do WAD, ordenada por nome
        let names: Vec<_> = u.textures.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["WALL01", "{FENCE", "{shot1"], "ordem sem caixa: o `{{` vem depois das letras no ASCII");
        let wall = u.textures.iter().find(|t| t.name == "WALL01").unwrap();
        assert_eq!(wall.map_count, 3);
        assert_eq!(wall.maps, vec!["a", "b", "d"], "b repete a textura, mas conta uma vez");
        let fence = u.textures.iter().find(|t| t.name == "{FENCE").unwrap();
        assert_eq!((fence.map_count, fence.maps.clone()), (1, vec!["a".to_string()]));
        let shot = u.textures.iter().find(|t| t.name == "{shot1").unwrap();
        assert_eq!(shot.map_count, 0);
        assert_eq!(u.unused_count, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wad_usage_respeita_os_limites() {
        let (root, maps, wad_path) = usage_fixture("limite");
        let u = wad_usage_with(&wad_path, &maps, UsageLimits { max_maps: 2, max_maps_per_texture: 1 }).unwrap();
        assert!(u.truncated);
        assert_eq!(u.maps_scanned + u.maps_unreadable, 2);
        let u = wad_usage_with(&wad_path, &maps, UsageLimits { max_maps: 100, max_maps_per_texture: 1 }).unwrap();
        let wall = u.textures.iter().find(|t| t.name == "WALL01").unwrap();
        assert_eq!(wall.map_count, 3, "a contagem é exata");
        assert_eq!(wall.maps.len(), 1, "a lista é cortada");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wad_usage_confere_os_caminhos() {
        let (root, maps, wad_path) = usage_fixture("caminhos");
        assert!(wad_usage(&wad_path, &maps.join("a.bsp")).unwrap_err().contains("não é uma pasta"));
        assert!(wad_usage(&wad_path, &maps.join("..").join("maps")).is_err());
        assert!(wad_usage(&maps.join("a.bsp"), &maps).unwrap_err().contains("extensão"));
        assert!(wad_usage(&root.join("sumiu.wad"), &maps).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bsp_hostil_nao_da_panic_nem_estoura_memoria() {
        let dir = temp_dir("bsp-hostil");
        let write = |name: &str, bytes: Vec<u8>| {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };
        let good = bsp_with_textures(&[Some("a"), Some("b")]);
        assert_eq!(bsp_texture_names(&write("ok.bsp", good.clone())), Some(vec!["a".to_string(), "b".to_string()]));

        // contagem de texturas gigante
        let mut huge = good.clone();
        let tex_off = 4 + LUMP_COUNT * 8;
        huge[tex_off..tex_off + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(bsp_texture_names(&write("huge.bsp", huge)), None);
        // lump de texturas além do arquivo
        let mut out = good.clone();
        let lump_at = 4 + LUMP_TEXTURES * 8;
        out[lump_at + 4..lump_at + 8].copy_from_slice(&i32::MAX.to_le_bytes());
        assert_eq!(bsp_texture_names(&write("fora.bsp", out)), None);
        // offset de miptex além do lump / negativo: ignorado
        let mut weird = bsp_with_textures(&[Some("a"), Some("b")]);
        weird[tex_off + 4..tex_off + 8].copy_from_slice(&0x7FFF_0000i32.to_le_bytes());
        assert_eq!(bsp_texture_names(&write("estranho.bsp", weird)), Some(vec!["b".to_string()]));
        // truncado em todo tamanho
        for cut in 0..good.len() {
            let _ = bsp_texture_names(&write("cut.bsp", good[..cut].to_vec()));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
