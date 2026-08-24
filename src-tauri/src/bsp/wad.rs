//! Leitura de texturas de arquivos `.wad` (Quake/Half-Life).
//!
//! Os mapas GoldSrc referenciam a maioria das texturas de fora do BSP, via WAD.
//! O nome já está no BSP (cada texinfo aponta para uma entrada da tabela); o
//! pixel só pode vir daqui. Este módulo localiza os WADs declarados pelo mapa e
//! resolve o nome de textura em PNG — mesmos mip/paleta, só muda o formato do
//! container (WAD em vez de lump de BSP).

use super::palette;
use super::reader::Cursor;
use super::rgba_png;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub const MIPTEX: u8 = 0x43; // 'C' — lump de textura estilo miptex

/// Conjunto de WADs já carregados, com cache de texturas por nome.
pub struct WadSet {
    cache: HashMap<String, Option<(String, u32, u32)>>,
    wads: Vec<Wad>,
}

struct Wad {
    bytes: Vec<u8>,
    /// nome(minúsculo) -> (pos, len) dentro de `bytes`
    index: HashMap<String, (usize, usize)>,
}

impl WadSet {
    /// Monta o conjunto a partir da pasta do mapa e dos WADs que o worldspawn
    /// declara. Sempre inclui `cstrike.wad` e `valve/halflife.wad` como reforço.
    pub fn for_map(map_path: &Path, declared: &[String]) -> Self {
        let mod_dir =
            map_path.parent().and_then(|p| p.parent()).unwrap_or_else(|| Path::new("."));
        let mut names: Vec<String> = declared.to_vec();
        for fallback in ["cstrike.wad", "halflife.wad"] {
            names.push(fallback.to_string());
        }

        let mut paths: Vec<PathBuf> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for n in &names {
            if let Some(p) = find_wad(mod_dir, n) {
                if seen.insert(p.display().to_string()) {
                    paths.push(p);
                }
            }
        }

        Self {
            cache: HashMap::new(),
            wads: paths.iter().filter_map(|p| load_wad(p)).collect(),
        }
    }

    /// PNG (data URL) para um nome de textura, ou `None` se nenhum WAD a tem.
    pub fn resolve(&mut self, name: &str) -> Option<(String, u32, u32)> {
        let key = name.to_ascii_lowercase();
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let mut found = None;
        for wad in &self.wads {
            if let Some(&(pos, len)) = wad.index.get(&key) {
                if let Some(lump) = wad.bytes.get(pos..pos + len) {
                    if let Some(img) = texture_image(lump) {
                        found = Some(img);
                        break;
                    }
                }
            }
        }
        self.cache.insert(key, found.clone());
        found
    }
}

fn find_wad(mod_dir: &Path, name: &str) -> Option<PathBuf> {
    let in_mod = mod_dir.join(name);
    if in_mod.is_file() {
        return Some(in_mod);
    }
    // Irmão da pasta do mod (`Half-Life/valve/<wad>` quando o mod é `cstrike`).
    if let Some(game) = mod_dir.parent() {
        let in_valve = game.join("valve").join(name);
        if in_valve.is_file() {
            return Some(in_valve);
        }
    }
    None
}

/// Lê o cabeçalho do WAD e indexa os lumps de textura por nome.
fn load_wad(path: &Path) -> Option<Wad> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 12 {
        return None;
    }
    let magic = &bytes[0..4];
    if magic != b"WAD2" && magic != b"WAD3" {
        return None;
    }
    let num_lumps = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let info_ofs = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;

    let mut index = HashMap::new();
    for i in 0..num_lumps {
        let entry = info_ofs + i * 32;
        let Some(lump) = bytes.get(entry..entry + 32) else { break };
        let filepos = u32::from_le_bytes([lump[0], lump[1], lump[2], lump[3]]) as usize;
        // size = tamanho descomprimido (compression é 0 para miptex)
        let size = u32::from_le_bytes([lump[8], lump[9], lump[10], lump[11]]) as usize;
        if lump[12] != MIPTEX {
            continue;
        }
        let mut name_bytes = [0u8; 16];
        name_bytes.copy_from_slice(&lump[16..32]);
        let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(16);
        let name = String::from_utf8_lossy(&name_bytes[..end]).trim().to_string();
        if name.is_empty() {
            continue;
        }
        index.insert(name.to_ascii_lowercase(), (filepos, size));
    }
    Some(Wad { bytes, index })
}

/// Extrai o mip0 de um lump de textura de WAD. A paleta de 256 cores fecha o
/// lump (diferente do BSP, onde a paleta fecha o lump inteiro de texturas).
fn texture_image(lump: &[u8]) -> Option<(String, u32, u32)> {
    if lump.len() < 40 + 768 {
        return None;
    }
    let mut mip = Cursor::new(lump);
    let name = mip.fixed_str(16).ok()?;
    let width = mip.u32().ok()?;
    let height = mip.u32().ok()?;
    let off0 = mip.u32().ok()? as usize;
    if off0 == 0 {
        return None;
    }
    let w = width as usize;
    let h = height as usize;
    let need = w.checked_mul(h)?;
    let end = off0.checked_add(need)?;
    let pixels = lump.get(off0..end)?;

    let pal = &lump[lump.len() - 768..];
    // Mesma convenção do BSP: só textura `{`-prefixada fura no índice 255.
    let rgba = palette::decode_indexed(pixels, pal, name.starts_with('{'))?;
    Some((rgba_png(w, h, &rgba)?, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// miptex de 1x1 pixel + paleta de 256 cores, mesmo layout de um lump `'C'` de WAD.
    fn miptex_lump(name: &str, pixel_idx: u8, idx255_rgb: [u8; 3]) -> Vec<u8> {
        let mut raw_name = [0u8; 16];
        let bytes = name.as_bytes();
        raw_name[..bytes.len().min(15)].copy_from_slice(&bytes[..bytes.len().min(15)]);
        let mut out = Vec::new();
        out.extend_from_slice(&raw_name);
        out.extend_from_slice(&1u32.to_le_bytes()); // width
        out.extend_from_slice(&1u32.to_le_bytes()); // height
        out.extend_from_slice(&40u32.to_le_bytes()); // offset do mip0
        out.extend_from_slice(&[0u8; 12]); // offsets[1..4]
        out.push(pixel_idx);
        let mut palette = vec![0u8; 768];
        palette[765..768].copy_from_slice(&idx255_rgb);
        out.extend_from_slice(&palette);
        out
    }

    /// Só o bastante do PNG que `rgba_png` gera (RGBA 8-bit, filtro "None") pra
    /// checar o canal alfa do único pixel.
    fn decode_png_alpha(data_url: &str) -> u8 {
        use base64::Engine as _;
        let b64 = data_url.strip_prefix("data:image/png;base64,").expect("data URL de PNG");
        let png = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let mut pos = 8usize;
        let mut idat = Vec::new();
        while pos + 8 <= png.len() {
            let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
            let kind = &png[pos + 4..pos + 8];
            if kind == b"IDAT" {
                idat.extend_from_slice(&png[pos + 8..pos + 8 + len]);
            }
            pos += 8 + len + 4;
        }
        let mut scan = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&idat[..]), &mut scan).unwrap();
        scan[1 + 3] // byte de filtro + R,G,B,A do único pixel
    }

    #[test]
    fn textura_de_wad_comum_nao_fura_no_indice_255() {
        let lump = miptex_lump("wall01", 255, [10, 20, 30]);
        let (png, w, h) = texture_image(&lump).expect("decodifica");
        assert_eq!((w, h), (1, 1));
        assert_eq!(decode_png_alpha(&png), 255);
    }

    #[test]
    fn textura_de_wad_chave_fura_no_indice_255() {
        let lump = miptex_lump("{cerca", 255, [10, 20, 30]);
        let (png, _, _) = texture_image(&lump).expect("decodifica");
        assert_eq!(decode_png_alpha(&png), 0);
    }
}
