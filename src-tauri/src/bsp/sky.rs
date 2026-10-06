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

/// TGA truecolor/grayscale, com ou sem RLE. Devolve PNG (data URL).
pub fn tga_png(data: &[u8]) -> Option<String> {
    if data.len() < 18 {
        return None;
    }
    let id_length = data[0] as usize;
    let colormap = data[1];
    let image_type = data[2];
    let width = u16le(data, 12)? as usize;
    let height = u16le(data, 14)? as usize;
    let depth = data[16] as usize;
    let descriptor = data[17];
    let top_down = descriptor & 0x20 != 0;

    // mesmo teto do BMP: céu de mapa não passa de poucos milhares de pixels por lado.
    const MAX_SIDE: usize = 8192;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    if colormap == 1 {
        return None; // paletizado — celeiro do céu não usa, é raro
    }
    let gray = matches!(image_type, 3 | 11);
    if !gray && !matches!(image_type, 2 | 10) {
        return None;
    }
    let channels = if gray { 1 } else { depth / 8 }; // 3 ou 4
    if channels == 0 || channels > 4 {
        return None;
    }

    let mut pos = 18 + id_length;
    let total = width.checked_mul(height)?;
    let mut raw = vec![0u8; total * channels];
    match image_type {
        2 | 3 => {
            let bytes = data.get(pos..pos + total * channels)?;
            raw.copy_from_slice(bytes);
        }
        10 | 11 => {
            let mut oi = 0usize;
            while oi < raw.len() {
                let header = *data.get(pos)?;
                pos += 1;
                let count = ((header & 0x7f) as usize) + 1;
                if header & 0x80 != 0 {
                    let px = data.get(pos..pos + channels)?;
                    pos += channels;
                    for _ in 0..count {
                        if oi >= raw.len() {
                            break;
                        }
                        raw[oi..oi + channels].copy_from_slice(px);
                        oi += channels;
                    }
                } else {
                    for _ in 0..count {
                        if oi >= raw.len() {
                            break;
                        }
                        let px = data.get(pos..pos + channels)?;
                        pos += channels;
                        raw[oi..oi + channels].copy_from_slice(px);
                        oi += channels;
                    }
                }
            }
        }
        _ => return None,
    }

    let img = unfold_rgba(&raw, width, height, channels, gray);
    let out = if top_down { img } else { flip_rows(img, width, height) };
    rgba_png(width, height, &out)
}

fn unfold_rgba(raw: &[u8], width: usize, height: usize, channels: usize, gray: bool) -> Vec<u8> {
    let mut img = vec![0u8; width * height * 4];
    for (i, px) in img.chunks_exact_mut(4).enumerate() {
        let src = i * channels;
        let (r, g, b, a) = if gray {
            let v = *raw.get(src).unwrap_or(&0);
            (v, v, v, 255)
        } else if channels == 4 {
            (*raw.get(src + 2).unwrap_or(&0), *raw.get(src + 1).unwrap_or(&0), *raw.get(src).unwrap_or(&0), *raw.get(src + 3).unwrap_or(&255))
        } else {
            (*raw.get(src + 2).unwrap_or(&0), *raw.get(src + 1).unwrap_or(&0), *raw.get(src).unwrap_or(&0), 255)
        };
        px.copy_from_slice(&[r, g, b, a]);
    }
    img
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

