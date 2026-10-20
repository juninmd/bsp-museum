//! Decodificação dos 6 lados do céu do GoldSrc.
//!
//! O céu de um mapa é um conjunto de 6 imagens em `gfx/env/<skyname>_*.tga`
//! (também `.bmp` em alguns jogos). O motor monta uma caixa com elas. Aqui
//! decodificamos o lado pedido em PNG para o WebView.

use super::rgba_png;

fn u16le(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]))
}

fn u32le(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *data.get(at)?,
        *data.get(at + 1)?,
        *data.get(at + 2)?,
        *data.get(at + 3)?,
    ]))
}

fn i32le(data: &[u8], at: usize) -> Option<i32> {
    u32le(data, at).map(|v| v as i32)
}

/// Imagem TGA já decodificada para RGBA (linha 0 = topo).
pub struct TgaImage {
    pub width: usize,
    pub height: usize,
    /// bits por pixel do arquivo (8, 16, 24 ou 32)
    pub depth: u8,
    pub rle: bool,
    /// índices + tabela de cores (tipos 1/9)
    pub paletted: bool,
    pub has_alpha: bool,
    pub rgba: Vec<u8>,
}

/// Lado máximo aceito no céu: céu de mapa não passa de poucos milhares de pixels por lado.
const SKY_MAX_SIDE: usize = 8192;

/// TGA truecolor/grayscale/paletizado, com ou sem RLE. Devolve PNG (data URL).
pub fn tga_png(data: &[u8]) -> Option<String> {
    let img = tga_decode(data, SKY_MAX_SIDE)?;
    rgba_png(img.width, img.height, &img.rgba)
}

/// Decodifica um TGA (tipos 1, 2, 3, 9, 10 e 11; 8/16/24/32 bits) para RGBA.
///
/// `max_side` limita largura e altura: dimensões absurdas só existem em arquivo
/// corrompido/malicioso e não podem virar alocação gigante. Nunca dá panic — qualquer
/// inconsistência (cabeçalho, dados curtos demais) devolve `None`.
pub fn tga_decode(data: &[u8], max_side: usize) -> Option<TgaImage> {
    if data.len() < 18 {
        return None;
    }
    let id_length = data[0] as usize;
    let colormap_type = data[1];
    let image_type = data[2];
    let cmap_first = u16le(data, 3)? as usize;
    let cmap_len = u16le(data, 5)? as usize;
    let cmap_depth = data[7] as usize;
    let width = u16le(data, 12)? as usize;
    let height = u16le(data, 14)? as usize;
    let depth = data[16] as usize;
    let descriptor = data[17];
    let top_down = descriptor & 0x20 != 0;

    if width == 0 || height == 0 || width > max_side || height > max_side {
        return None;
    }
    let paletted = matches!(image_type, 1 | 9);
    let gray = matches!(image_type, 3 | 11);
    let rle = matches!(image_type, 9..=11);
    if !paletted && !gray && !matches!(image_type, 2 | 10) {
        return None;
    }
    // profundidades que o decodificador entende
    let ok_depth = match image_type {
        1 | 9 | 3 | 11 => depth == 8,
        _ => matches!(depth, 16 | 24 | 32),
    };
    if !ok_depth {
        return None;
    }
    let bpp = depth / 8;

    // tabela de cores (bytes logo depois do campo de ID); os tipos truecolor com tabela
    // só precisam pulá-la
    let mut pos = 18usize.checked_add(id_length)?;
    let mut table: Vec<[u8; 4]> = Vec::new();
    if colormap_type == 1 {
        if !matches!(cmap_depth, 15 | 16 | 24 | 32) {
            return None;
        }
        let entry = cmap_depth.div_ceil(8);
        let bytes = data.get(pos..pos.checked_add(cmap_len.checked_mul(entry)?)?)?;
        pos += cmap_len * entry;
        for e in bytes.chunks_exact(entry) {
            table.push(match entry {
                2 => {
                    let v = u16::from_le_bytes([e[0], e[1]]);
                    [five_to_eight(v >> 10), five_to_eight(v >> 5), five_to_eight(v), 255]
                }
                3 => [e[2], e[1], e[0], 255],
                _ => [e[2], e[1], e[0], e[3]],
            });
        }
    } else if paletted {
        return None; // tipo paletizado sem tabela
    }

    let total = width.checked_mul(height)?;
    let raw_len = total.checked_mul(bpp)?;
    let avail = data.len().checked_sub(pos)?;
    let mut raw = Vec::new();
    if rle {
        // um pacote de RLE expande no máximo 128 pixels por (1 + bpp) bytes: se nem assim
        // o arquivo alcança o tamanho declarado, é mentira — recusa antes de alocar
        if raw_len > (avail / (1 + bpp)).checked_mul(128 * bpp)? {
            return None;
        }
        raw.reserve_exact(raw_len);
        while raw.len() < raw_len {
            let header = *data.get(pos)?;
            pos += 1;
            let count = ((header & 0x7f) as usize) + 1;
            let want = (count * bpp).min(raw_len - raw.len());
            if header & 0x80 != 0 {
                let px = data.get(pos..pos + bpp)?;
                pos += bpp;
                for _ in 0..want / bpp {
                    raw.extend_from_slice(px);
                }
            } else {
                raw.extend_from_slice(data.get(pos..pos + want)?);
                pos += count * bpp;
            }
        }
    } else {
        raw.extend_from_slice(data.get(pos..pos.checked_add(raw_len)?)?);
    }

    // bits de atributo (alpha) declarados no descritor: 0 = sem alpha nos 16 bits
    let attr_bits = descriptor & 0x0f;
    let has_alpha = depth == 32 || (depth == 16 && attr_bits > 0) || (paletted && table.iter().any(|c| c[3] != 255));
    let mut rgba = vec![0u8; total.checked_mul(4)?];
    for (i, out) in rgba.chunks_exact_mut(4).enumerate() {
        let src = i * bpp;
        let px: [u8; 4] = if paletted {
            let idx = (raw[src] as usize).checked_sub(cmap_first);
            idx.and_then(|k| table.get(k)).copied().unwrap_or([0, 0, 0, 255])
        } else if gray {
            [raw[src], raw[src], raw[src], 255]
        } else {
            match bpp {
                2 => {
                    let v = u16::from_le_bytes([raw[src], raw[src + 1]]);
                    let a = if attr_bits > 0 && v & 0x8000 == 0 { 0 } else { 255 };
                    [five_to_eight(v >> 10), five_to_eight(v >> 5), five_to_eight(v), a]
                }
                3 => [raw[src + 2], raw[src + 1], raw[src], 255],
                _ => [raw[src + 2], raw[src + 1], raw[src], raw[src + 3]],
            }
        };
        out.copy_from_slice(&px);
    }
    let rgba = if top_down { rgba } else { flip_rows(rgba, width, height) };
    Some(TgaImage { width, height, depth: depth as u8, rle, paletted, has_alpha, rgba })
}

/// 5 bits (os 5 menos significativos de `v`) -> 8 bits.
fn five_to_eight(v: u16) -> u8 {
    let v = (v & 0x1f) as u8;
    (v << 3) | (v >> 2)
}

fn flip_rows(img: Vec<u8>, width: usize, height: usize) -> Vec<u8> {
    let stride = width * 4;
    let mut out = vec![0u8; img.len()];
    for row in 0..height {
        let from = (height - 1 - row) * stride;
        let to = row * stride;
        out[to..to + stride].copy_from_slice(&img[from..from + stride]);
    }
    out
}

/// BMP 24/32-bit despistado (alguns jogos guardam o céu em `.bmp`).
pub fn bmp_png(data: &[u8]) -> Option<String> {
    if data.len() < 54 || &data[0..2] != b"BM" {
        return None;
    }
    let offset = u32le(data, 10)? as usize;
    let width = i32le(data, 18)?;
    let height = i32le(data, 22)?;
    let bpp = u16le(data, 28)? as usize;
    if width <= 0 {
        return None;
    }
    let w = width as usize;
    let h = height.unsigned_abs() as usize;
    let top_down = height < 0;
    let channels = bpp / 8;
    if !(channels == 3 || channels == 4) {
        return None;
    }
    // Céu de mapa GoldSrc não passa de poucos milhares de pixels por lado; dimensões
    // maiores só existem num BMP corrompido/malicioso e não devem virar alocação gigante.
    const MAX_SIDE: usize = 8192;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let out_len = w.checked_mul(h)?.checked_mul(4)?;

    let stride = (w * channels).div_ceil(4) * 4;
    let mut out = vec![0u8; out_len];
    for r in 0..h {
        let src_row = if top_down { r } else { h - 1 - r };
        let base = offset + src_row * stride;
        for c in 0..w {
            let p = base + c * channels;
            let b = *data.get(p)?;
            let g = *data.get(p + 1)?;
            let rr = *data.get(p + 2)?;
            let a = if channels == 4 { *data.get(p + 3)? } else { 255 };
            let d = (r * w + c) * 4;
            out[d] = rr;
            out[d + 1] = g;
            out[d + 2] = b;
            out[d + 3] = a;
        }
    }
    rgba_png(w, h, &out)
}

/// Tenta `.tga` primeiro e depois `.bmp`; devolve o PNG (data URL).
pub fn load_sky_image(path: &std::path::Path) -> Option<String> {
    let tga = path.with_extension("tga");
    if let Ok(bytes) = std::fs::read(&tga) {
        if let Some(candidate) = tga_png(&bytes) {
            if candidate.starts_with("data:image/png") {
                return Some(candidate);
            }
        }
    }
    let bmp = path.with_extension("bmp");
    if let Ok(bytes) = std::fs::read(&bmp) {
        if let Some(png) = bmp_png(&bytes) {
            return Some(png);
        }
    }
    None
}

/// Carrega os 6 lados do céu do mapa, na ordem `[up, down, left, right, front,
/// back]`. Procura no `gfx/env` do próprio mod e no do mod irmão (`valve`).
///
/// Convenções do GoldSrc: às vezes `<skyname>up.tga`, às vezes `<skyname>_up.tga`
/// — tentamos as duas, e `.tga` e `.bmp` (case-insensitive na maioria dos FS).
pub fn load_sky(map_path: &std::path::Path, skyname: &str) -> Option<[Option<String>; 6]> {
    let name = skyname.trim().trim_end_matches('_');
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let mod_dir = map_path.parent().and_then(|p| p.parent())?;
    let game = mod_dir.parent()?;
    let dirs = [
        mod_dir.join("gfx/env"),
        game.join("cstrike/gfx/env"),
        game.join("valve/gfx/env"),
    ];
    let suffixes = ["up", "dn", "lf", "rt", "ft", "bk"];
    let mut out: [Option<String>; 6] = [None, None, None, None, None, None];
    for (i, suffix) in suffixes.iter().enumerate() {
        'dir: for dir in &dirs {
            for pattern in [format!("{name}{suffix}"), format!("{name}_{suffix}")] {
                if let Some(png) = load_sky_image(&dir.join(&pattern)) {
                    out[i] = Some(png);
                    break 'dir;
                }
            }
        }
    }
    if out.iter().all(|f| f.is_none()) {
        None
    } else {
        Some(out)
    }
}

