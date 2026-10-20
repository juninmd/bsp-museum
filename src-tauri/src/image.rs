//! Visualizador genérico de imagens do mod: TGA, BMP (8/24/32 bits) e `.lmp`/`qpic`.
//!
//! O mod tem mais imagem solta do que o resto do app enxerga: sprays (`tga`/`bmp`),
//! overviews de radar (`bmp` 8 bits), céu, `gfx/*.lmp`. Aqui tudo vira PNG (data URL) com
//! largura, altura e o formato original — o frontend só mostra.
//!
//! Como no resto do núcleo, o arquivo é tratado como hostil: toda leitura é checada, as
//! dimensões têm teto (`MAX_SIDE`) e dado curto demais vira erro, nunca panic.

use crate::bsp::reader::{BspError, Cursor};
use crate::bsp::rgba_png;
use crate::bsp::sky::tga_decode;
use serde::Serialize;
use std::fmt;
use std::path::Path;

/// Lado máximo (largura e altura) aceito: 4096² RGBA são 64 MB, o teto do que o WebView
/// engole sem engasgar. Imagem de mod GoldSrc raramente passa de 512.
pub const MAX_SIDE: usize = 4096;
/// Arquivo maior que isto não é imagem de mod: recusa antes de ler.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Tga,
    Bmp,
    /// `.lmp` do Quake / `qpic` do Half-Life: largura, altura, pixels indexados
    Lmp,
}

/// De onde vieram as cores de uma imagem indexada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaletteKind {
    /// imagem truecolor/escala de cinza: não há paleta
    None,
    /// paleta de 256 cores dentro do próprio arquivo
    Embedded,
    /// `.lmp` do Quake sem paleta: a global do jogo não vem com o arquivo, então
    /// os índices viram tons de cinza (reconhece-se o desenho, não a cor)
    Grayscale,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// bits por pixel do arquivo (8, 16, 24 ou 32)
    pub bits_per_pixel: u16,
    pub has_alpha: bool,
    /// BMP RLE8 / TGA com RLE
    pub rle: bool,
    pub palette: PaletteKind,
    /// `data:image/png;base64,...`
    pub png: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// leitura além do fim do arquivo
    Truncated(BspError),
    /// formato que o visualizador não reconhece (assinatura/extensão)
    Unknown,
    /// cabeçalho inconsistente ou variante não suportada (mensagem legível)
    Invalid(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::Truncated(e) => write!(f, "imagem truncada: {e}"),
            ImageError::Unknown => write!(f, "formato de imagem não reconhecido (esperado TGA, BMP ou LMP)"),
            ImageError::Invalid(msg) => write!(f, "imagem inválida: {msg}"),
        }
    }
}

impl std::error::Error for ImageError {}

impl From<BspError> for ImageError {
    fn from(e: BspError) -> Self {
        ImageError::Truncated(e)
    }
}

fn invalid<T>(msg: impl Into<String>) -> Result<T, ImageError> {
    Err(ImageError::Invalid(msg.into()))
}

/// Confere largura/altura (positivas e dentro do teto).
fn check_dims(w: usize, h: usize) -> Result<(), ImageError> {
    if w == 0 || h == 0 {
        return invalid("largura ou altura zero");
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return invalid(format!("{w}x{h} passa do máximo de {MAX_SIDE} pixels por lado"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn finish(
    format: ImageFormat,
    w: usize,
    h: usize,
    bpp: u16,
    has_alpha: bool,
    rle: bool,
    palette: PaletteKind,
    rgba: &[u8],
) -> Result<ImageInfo, ImageError> {
    let png = rgba_png(w, h, rgba).ok_or_else(|| ImageError::Invalid("não montou o PNG".into()))?;
    Ok(ImageInfo { format, width: w as u32, height: h as u32, bits_per_pixel: bpp, has_alpha, rle, palette, png })
}

/// Decodifica uma imagem já em memória. A assinatura `BM` decide o BMP; TGA e LMP não têm
/// assinatura, então dependem de `hint` (a extensão do arquivo).
pub fn decode_image(data: &[u8], hint: Option<ImageFormat>) -> Result<ImageInfo, ImageError> {
    // `BM` não colide com os outros: num TGA seria id_length=66 e colormap=77 (inválido), num
    // lmp seria largura 19778 (> MAX_SIDE)
    if data.starts_with(b"BM") {
        return decode_bmp(data);
    }
    match hint {
        Some(ImageFormat::Tga) => decode_tga(data),
        Some(ImageFormat::Lmp) => decode_lmp(data),
        Some(ImageFormat::Bmp) => invalid("BMP sem a assinatura `BM`"),
        None => Err(ImageError::Unknown),
    }
}

/// Lê e decodifica uma imagem do disco (`.tga`, `.bmp` ou `.lmp`).
pub fn load_image(path: &Path) -> Result<ImageInfo, String> {
    let bytes = crate::util::read_checked(path, &["tga", "bmp", "lmp"], MAX_FILE_BYTES)?;
    let hint = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("tga") => Some(ImageFormat::Tga),
        Some("bmp") => Some(ImageFormat::Bmp),
        Some("lmp") => Some(ImageFormat::Lmp),
        _ => None,
    };
    decode_image(&bytes, hint).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------- TGA

/// TGA (tipos 1/2/3/9/10/11; 8/16/24/32 bits). O decodificador é o mesmo do céu
/// (`bsp::sky::tga_decode`), só com o teto de dimensões deste módulo.
pub fn decode_tga(data: &[u8]) -> Result<ImageInfo, ImageError> {
    let img = tga_decode(data, MAX_SIDE)
        .ok_or_else(|| ImageError::Invalid("TGA ilegível (tipo, profundidade ou dados não suportados/truncados)".into()))?;
    let palette = if img.paletted { PaletteKind::Embedded } else { PaletteKind::None };
    finish(ImageFormat::Tga, img.width, img.height, img.depth as u16, img.has_alpha, img.rle, palette, &img.rgba)
}

// ---------------------------------------------------------------------------- BMP

fn le16(data: &[u8], at: usize) -> Result<u16, ImageError> {
    let b = data.get(at..at + 2).ok_or(BspError::TooSmall { need: at + 2, have: data.len() })?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn le32(data: &[u8], at: usize) -> Result<u32, ImageError> {
    let b = data.get(at..at + 4).ok_or(BspError::TooSmall { need: at + 4, have: data.len() })?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Extrai o canal de `v` pela máscara, escalado para 0..=255 (máscara 0 = canal ausente).
fn masked_channel(v: u32, mask: u32) -> Option<u8> {
    if mask == 0 {
        return None;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    let raw = (v & mask) >> shift;
    Some(((raw as u64 * 255) / max as u64) as u8)
}

/// RLE8 do BMP (`BI_RLE8`): devolve a grade de índices na ordem do arquivo (linha 0 = primeira
/// linha gravada). Pixel nunca coberto fica 0; dado truncado deixa o resto em 0 em vez de falhar.
fn rle8(data: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut grid = vec![0u8; w * h];
    let (mut x, mut y, mut pos) = (0usize, 0usize, 0usize);
    let mut put = |x: usize, y: usize, v: u8| {
        if x < w && y < h {
            grid[y * w + x] = v;
        }
    };
    while y < h {
        let (Some(&n), Some(&c)) = (data.get(pos), data.get(pos + 1)) else { break };
        pos += 2;
        if n > 0 {
            // modo codificado: `n` pixels do índice `c`
            for _ in 0..n {
                put(x, y, c);
                x += 1;
            }
            continue;
        }
        match c {
            0 => {
                x = 0;
                y += 1;
            }
            1 => break,
            2 => {
                let (Some(&dx), Some(&dy)) = (data.get(pos), data.get(pos + 1)) else { break };
                pos += 2;
                x += dx as usize;
                y += dy as usize;
            }
            k => {
                // modo absoluto: `k` índices literais, alinhado a palavra de 16 bits
                let k = k as usize;
                for i in 0..k {
                    let Some(&v) = data.get(pos + i) else { break };
                    put(x, y, v);
                    x += 1;
                }
                pos += k + (k & 1);
            }
        }
    }
    grid
}

/// BMP de 8 bits (paleta, RGB ou RLE8), 24 bits e 32 bits (RGB ou BITFIELDS).
///
/// Sem suporte (erro legível): 1/4/16 bits, RLE4, JPEG/PNG embutido.
pub fn decode_bmp(data: &[u8]) -> Result<ImageInfo, ImageError> {
    if data.len() < 26 || &data[0..2] != b"BM" {
        return invalid("BMP sem a assinatura `BM`");
    }
    let data_off = le32(data, 10)? as usize;
    let dib_size = le32(data, 14)? as usize;

    // OS/2 (cabeçalho de 12 bytes, paleta de 3 bytes) ou BITMAPINFOHEADER e derivados (>= 40)
    let (width, height, bpp, compression, clr_used, pal_entry) = if dib_size == 12 {
        (le16(data, 18)? as i64, (le16(data, 20)? as i16) as i64, le16(data, 24)?, 0u32, 0u32, 3usize)
    } else if dib_size >= 40 {
        (
            (le32(data, 18)? as i32) as i64,
            (le32(data, 22)? as i32) as i64,
            le16(data, 28)?,
            le32(data, 30)?,
            le32(data, 46)?,
            4usize,
        )
    } else {
        return invalid(format!("cabeçalho DIB de {dib_size} bytes não é suportado"));
    };
    if width <= 0 || height == 0 {
        return invalid("largura ou altura inválida");
    }
    let w = width as usize;
    let h = height.unsigned_abs() as usize;
    check_dims(w, h)?;
    let bottom_up = height > 0;
    if !matches!(bpp, 8 | 24 | 32) {
        return invalid(format!("{bpp} bits por pixel não é suportado (só 8, 24 e 32)"));
    }
    match (compression, bpp) {
        (0, _) | (1, 8) | (3, 32) => {}
        (1, _) => return invalid("RLE8 só vale com 8 bits por pixel"),
        (2, _) => return invalid("RLE4 não é suportado"),
        (3, _) => return invalid("BITFIELDS só é suportado com 32 bits"),
        (c, _) => return invalid(format!("compressão {c} do BMP não é suportada")),
    }
    let pixels = data.get(data_off..).ok_or(BspError::TooSmall { need: data_off, have: data.len() })?;

    // paleta (8 bits): logo depois do cabeçalho DIB
    let mut palette: Vec<[u8; 3]> = Vec::new();
    if bpp == 8 {
        let n = if clr_used == 0 || clr_used > 256 { 256 } else { clr_used as usize };
        let base = 14 + dib_size;
        for i in 0..n {
            let at = base + i * pal_entry;
            // a paleta não pode invadir os pixels; entrada que falta fica de fora (preto)
            if at + pal_entry > data_off {
                break;
            }
            let Some(e) = data.get(at..at + pal_entry) else { break };
            palette.push([e[2], e[1], e[0]]);
        }
        if palette.is_empty() {
            return invalid("BMP de 8 bits sem paleta");
        }
    }

    // linha da imagem (0 = topo) a partir da posição gravada no arquivo
    let image_row = |stored: usize| if bottom_up { h - 1 - stored } else { stored };
    let mut rgba = vec![0u8; w * h * 4];
    let mut has_alpha = false;

    if bpp == 8 {
        let owned;
        let grid: &[u8] = if compression == 1 {
            owned = rle8(pixels, w, h);
            &owned
        } else {
            let stride = w.div_ceil(4) * 4;
            let need = stride * h;
            if pixels.len() < need {
                return Err(BspError::TooSmall { need: data_off + need, have: data.len() }.into());
            }
            owned = (0..h).flat_map(|r| pixels[r * stride..r * stride + w].iter().copied()).collect();
            &owned
        };
        for stored in 0..h {
            let dst_row = image_row(stored);
            for c in 0..w {
                let rgb = palette.get(grid[stored * w + c] as usize).copied().unwrap_or([0, 0, 0]);
                let d = (dst_row * w + c) * 4;
                rgba[d..d + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
    } else {
        let channels = (bpp / 8) as usize;
        let stride = (w * channels).div_ceil(4) * 4;
        let need = stride * h;
        if pixels.len() < need {
            return Err(BspError::TooSmall { need: data_off + need, have: data.len() }.into());
        }
        // máscaras do BITFIELDS ficam logo depois dos 40 bytes do BITMAPINFOHEADER
        let masks = if compression == 3 {
            let m = |i: usize| le32(data, 14 + 40 + i * 4).unwrap_or(0);
            Some([m(0), m(1), m(2), if dib_size >= 56 { m(3) } else { 0 }])
        } else {
            None
        };
        let mut any_alpha_byte = false;
        for stored in 0..h {
            let dst_row = image_row(stored);
            for c in 0..w {
                let p = stored * stride + c * channels;
                let d = (dst_row * w + c) * 4;
                let px = &pixels[p..p + channels];
                let out: [u8; 4] = match (channels, masks) {
                    (3, _) => [px[2], px[1], px[0], 255],
                    (_, Some(m)) => {
                        let v = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
                        let a = masked_channel(v, m[3]);
                        has_alpha |= a.is_some();
                        [
                            masked_channel(v, m[0]).unwrap_or(0),
                            masked_channel(v, m[1]).unwrap_or(0),
                            masked_channel(v, m[2]).unwrap_or(0),
                            a.unwrap_or(255),
                        ]
                    }
                    _ => {
                        any_alpha_byte |= px[3] != 0;
                        [px[2], px[1], px[0], px[3]]
                    }
                };
                rgba[d..d + 4].copy_from_slice(&out);
            }
        }
        if channels == 4 && masks.is_none() {
            if any_alpha_byte {
                has_alpha = true;
            } else {
                // BMP de 32 bits costuma gravar o 4º byte como 0 ("sem uso"): tratar como
                // transparente deixaria a imagem invisível
                for px in rgba.chunks_exact_mut(4) {
                    px[3] = 255;
                }
            }
        }
    }

    let palette_kind = if bpp == 8 { PaletteKind::Embedded } else { PaletteKind::None };
    finish(ImageFormat::Bmp, w, h, bpp, has_alpha, compression == 1, palette_kind, &rgba)
}

// ---------------------------------------------------------------------------- LMP / qpic

/// `.lmp` do Quake / `qpic` do Half-Life: `u32 largura, u32 altura, pixels[w*h]` e, no
/// Half-Life, depois dos pixels `u16 = 256` + paleta de 768 bytes. Sem paleta, os índices
/// viram tons de cinza (ver `PaletteKind::Grayscale`). O índice 255 é mantido opaco: só o
/// desenho do HUD o trata como transparente, e isso depende de quem desenha.
pub fn decode_lmp(data: &[u8]) -> Result<ImageInfo, ImageError> {
    let mut cur = Cursor::new(data);
    let w = cur.u32()? as usize;
    let h = cur.u32()? as usize;
    check_dims(w, h)?;
    let n = w * h;
    let pixels = data.get(8..8 + n).ok_or(BspError::TooSmall { need: 8 + n, have: data.len() })?;
    let rest = &data[8 + n..];
    let (pal, kind): (Vec<u8>, PaletteKind) = if rest.len() >= 2 + 768 && u16::from_le_bytes([rest[0], rest[1]]) == 256 {
        (rest[2..2 + 768].to_vec(), PaletteKind::Embedded)
    } else {
        ((0..=255u8).flat_map(|v| [v, v, v]).collect(), PaletteKind::Grayscale)
    };
    let rgba = crate::bsp::palette::decode_indexed(pixels, &pal, false)
        .ok_or_else(|| ImageError::Invalid("paleta ilegível".into()))?;
    finish(ImageFormat::Lmp, w, h, 8, false, false, kind, &rgba)
}

/// PNG de um `qpic` já em memória (lump `'B'` de WAD3), para o navegador de WAD.
pub(crate) fn qpic_png(data: &[u8]) -> Result<(String, u32, u32), ImageError> {
    let img = decode_lmp(data)?;
    Ok((img.png, img.width, img.height))
}

// ---------------------------------------------------------------------------- testes

/// Decodifica o PNG que `rgba_png` gera (RGBA 8 bits, filtro "None"): devolve `(w, h, rgba)`.
/// Só para os testes dos módulos que produzem imagem.
#[cfg(test)]
pub(crate) fn decode_png_rgba(data_url: &str) -> (u32, u32, Vec<u8>) {
    use base64::Engine as _;
    let b64 = data_url.strip_prefix("data:image/png;base64,").expect("data URL de PNG");
    let png = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    assert_eq!(&png[0..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let (mut w, mut h) = (0u32, 0u32);
    let mut pos = 8usize;
    let mut idat = Vec::new();
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &png[pos + 4..pos + 8];
        let body = &png[pos + 8..pos + 8 + len];
        if kind == b"IHDR" {
            w = u32::from_be_bytes(body[0..4].try_into().unwrap());
            h = u32::from_be_bytes(body[4..8].try_into().unwrap());
        } else if kind == b"IDAT" {
            idat.extend_from_slice(body);
        }
        pos += 8 + len + 4;
    }
    let mut scan = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&idat[..]), &mut scan).unwrap();
    let stride = w as usize * 4;
    let mut rgba = Vec::new();
    for row in scan.chunks(stride + 1) {
        assert_eq!(row[0], 0, "o encoder só usa filtro None");
        rgba.extend_from_slice(&row[1..]);
    }
    (w, h, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paleta de teste: índice i -> (i, 255 - i, 7).
    fn pal768() -> Vec<u8> {
        (0..=255u8).flat_map(|i| [i, 255 - i, 7]).collect()
    }

    // ---- BMP

    /// BMP montado byte a byte. `rows` já na ordem gravada no arquivo (primeira = a de baixo
    /// quando `height > 0`), cada uma com o padding de 4 bytes aplicado por quem chama.
    fn bmp(width: i32, height: i32, bpp: u16, compression: u32, palette: &[[u8; 3]], pixels: &[u8]) -> Vec<u8> {
        let pal_bytes = palette.len() * 4;
        let data_off = 14 + 40 + pal_bytes;
        let mut out = Vec::new();
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&((data_off + pixels.len()) as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&bpp.to_le_bytes());
        out.extend_from_slice(&compression.to_le_bytes());
        out.extend_from_slice(&(pixels.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0; 8]); // resolução
        out.extend_from_slice(&(palette.len() as u32).to_le_bytes()); // clrUsed
        out.extend_from_slice(&[0; 4]);
        for c in palette {
            out.extend_from_slice(&[c[2], c[1], c[0], 0]);
        }
        out.extend_from_slice(pixels);
        out
    }

    fn rgb_palette() -> Vec<[u8; 3]> {
        vec![[255, 0, 0], [0, 255, 0], [0, 0, 255], [10, 20, 30]]
    }

    #[test]
    fn bmp_8_bits_de_baixo_para_cima() {
        // 3x2, paleta de 4 cores. Linha gravada primeiro = linha de baixo da imagem.
        // baixo: 0 1 2 (+1 de padding) ; cima: 3 3 0 (+1)
        let file = bmp(3, 2, 8, 0, &rgb_palette(), &[0, 1, 2, 0, 3, 3, 0, 0]);
        let info = decode_image(&file, Some(ImageFormat::Bmp)).unwrap();
        assert_eq!((info.width, info.height, info.bits_per_pixel), (3, 2, 8));
        assert_eq!(info.format, ImageFormat::Bmp);
        assert_eq!(info.palette, PaletteKind::Embedded);
        assert!(!info.has_alpha && !info.rle);
        let (w, h, px) = decode_png_rgba(&info.png);
        assert_eq!((w, h), (3, 2));
        // topo: 3 3 0
        assert_eq!(&px[0..4], &[10, 20, 30, 255]);
        assert_eq!(&px[8..12], &[255, 0, 0, 255]);
        // base: 0 1 2
        assert_eq!(&px[12..16], &[255, 0, 0, 255]);
        assert_eq!(&px[16..20], &[0, 255, 0, 255]);
        assert_eq!(&px[20..24], &[0, 0, 255, 255]);
    }

    #[test]
    fn bmp_8_bits_de_cima_para_baixo() {
        // altura negativa = a primeira linha gravada é o topo
        let file = bmp(2, -2, 8, 0, &rgb_palette(), &[0, 1, 0, 0, 2, 3, 0, 0]);
        let info = decode_bmp(&file).unwrap();
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(&px[0..4], &[255, 0, 0, 255]); // topo-esquerda = índice 0
        assert_eq!(&px[8..12], &[0, 0, 255, 255]); // segunda linha, primeiro pixel = índice 2
    }

    #[test]
    fn bmp_8_bits_rle8() {
        // 4x2 bottom-up. linha de baixo: run de 4 pixels do índice 1; EOL;
        // linha de cima: absoluto de 3 (0,2,3) + padding, 1 pixel codificado do índice 1; EOB
        let rle = [4, 1, 0, 0, 0, 3, 0, 2, 3, 0, 1, 1, 0, 1];
        let file = bmp(4, 2, 8, 1, &rgb_palette(), &rle);
        let info = decode_bmp(&file).unwrap();
        assert!(info.rle);
        let (_, _, px) = decode_png_rgba(&info.png);
        // base (linha 1 da imagem): tudo verde
        for c in 0..4 {
            assert_eq!(&px[(4 + c) * 4..(4 + c) * 4 + 4], &[0, 255, 0, 255]);
        }
        // topo: 0 2 3 1
        assert_eq!(&px[0..4], &[255, 0, 0, 255]);
        assert_eq!(&px[4..8], &[0, 0, 255, 255]);
        assert_eq!(&px[8..12], &[10, 20, 30, 255]);
        assert_eq!(&px[12..16], &[0, 255, 0, 255]);
    }

    #[test]
    fn bmp_rle8_truncado_e_delta_fora_nao_da_panic() {
        // delta gigante e depois dado cortado no meio de um pacote absoluto
        let rle = [0, 2, 200, 200, 4, 1, 0, 5, 1, 2];
        let file = bmp(4, 2, 8, 1, &rgb_palette(), &rle);
        assert!(decode_bmp(&file).is_ok());
    }

    #[test]
    fn bmp_24_bits_bgr_com_padding() {
        // 1x2 bottom-up: linha de baixo = vermelho, de cima = azul; cada linha tem 3 bytes + 1 de padding
        let file = bmp(1, 2, 24, 0, &[], &[0, 0, 255, 0, 255, 0, 0, 0]);
        let info = decode_bmp(&file).unwrap();
        assert_eq!(info.bits_per_pixel, 24);
        assert_eq!(info.palette, PaletteKind::None);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(&px[0..4], &[0, 0, 255, 255]); // topo = 2ª linha gravada, BGR (255,0,0) = azul
        assert_eq!(&px[4..8], &[255, 0, 0, 255]); // base = 1ª linha gravada, BGR (0,0,255) = vermelho
    }

    #[test]
    fn bmp_32_bits_alfa_zerado_vira_opaco() {
        // 1x1, BGRA = (10,20,30,0): o 4º byte "sem uso" não pode deixar o pixel invisível
        let file = bmp(1, 1, 32, 0, &[], &[30, 20, 10, 0]);
        let info = decode_bmp(&file).unwrap();
        assert!(!info.has_alpha);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(px, vec![10, 20, 30, 255]);
    }

    #[test]
    fn bmp_32_bits_com_alfa_real() {
        let file = bmp(2, 1, 32, 0, &[], &[1, 2, 3, 128, 4, 5, 6, 0]);
        let info = decode_bmp(&file).unwrap();
        assert!(info.has_alpha);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(px, vec![3, 2, 1, 128, 6, 5, 4, 0]);
    }

    #[test]
    fn bmp_32_bits_bitfields() {
        // máscaras RGB 565-like num campo de 32 bits: R=0xFF0000, G=0x00FF00, B=0x0000FF
        let mut file = bmp(1, 1, 32, 3, &[], &[]);
        // BITMAPINFOHEADER de 40 bytes + 3 máscaras: reescreve o fim do arquivo
        let masks = [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF];
        let mut tail = Vec::new();
        for m in masks {
            tail.extend_from_slice(&m.to_le_bytes());
        }
        let data_off = 14 + 40 + 12;
        file.truncate(14 + 40);
        file.extend_from_slice(&tail);
        file.extend_from_slice(&0x0011_2233u32.to_le_bytes());
        file[10..14].copy_from_slice(&(data_off as u32).to_le_bytes());
        let info = decode_bmp(&file).unwrap();
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(px, vec![0x11, 0x22, 0x33, 255]);
    }

    #[test]
    fn bmp_recusa_variantes_nao_suportadas() {
        assert!(matches!(decode_bmp(&bmp(2, 2, 4, 0, &rgb_palette(), &[0; 8])), Err(ImageError::Invalid(_))));
        assert!(matches!(decode_bmp(&bmp(2, 2, 8, 2, &rgb_palette(), &[0; 8])), Err(ImageError::Invalid(_))));
        assert!(matches!(decode_bmp(&bmp(0, 2, 24, 0, &[], &[])), Err(ImageError::Invalid(_))));
        // dimensão acima do teto: erro sem alocar
        assert!(matches!(decode_bmp(&bmp(100_000, 100_000, 24, 0, &[], &[0; 16])), Err(ImageError::Invalid(_))));
        // 8 bits sem paleta
        assert!(matches!(decode_bmp(&bmp(2, 2, 8, 0, &[], &[0; 8])), Err(ImageError::Invalid(_))));
    }

    #[test]
    fn bmp_pixels_curtos_demais_dao_erro() {
        let file = bmp(4, 4, 24, 0, &[], &[0; 10]);
        assert!(matches!(decode_bmp(&file), Err(ImageError::Truncated(_))));
    }

    #[test]
    fn bmp_os2_cabecalho_de_12_bytes() {
        // OS/2: 12 bytes de DIB, paleta de 3 bytes/entrada
        let mut f = Vec::new();
        f.extend_from_slice(b"BM");
        f.extend_from_slice(&[0; 8]);
        let data_off = 14 + 12 + 256 * 3;
        f.extend_from_slice(&(data_off as u32).to_le_bytes());
        f.extend_from_slice(&12u32.to_le_bytes());
        f.extend_from_slice(&2u16.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&8u16.to_le_bytes());
        for i in 0..=255u8 {
            f.extend_from_slice(&[i, 0, 0]); // BGR: azul = i
        }
        f.extend_from_slice(&[5, 6, 0, 0]);
        let info = decode_bmp(&f).unwrap();
        let (w, h, px) = decode_png_rgba(&info.png);
        assert_eq!((w, h), (2, 1));
        assert_eq!(&px[0..4], &[0, 0, 5, 255]);
        assert_eq!(&px[4..8], &[0, 0, 6, 255]);
    }

    // ---- TGA

    fn tga_header(image_type: u8, w: u16, h: u16, depth: u8, descriptor: u8) -> Vec<u8> {
        let mut v = vec![0, 0, image_type, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.push(depth);
        v.push(descriptor);
        v
    }

    #[test]
    fn tga_24_bits_de_baixo_para_cima() {
        let mut f = tga_header(2, 2, 2, 24, 0);
        // gravado de baixo p/ cima: base = (B,G,R) vermelho, verde; topo = azul, branco
        f.extend_from_slice(&[0, 0, 255, 0, 255, 0]);
        f.extend_from_slice(&[255, 0, 0, 255, 255, 255]);
        let info = decode_image(&f, Some(ImageFormat::Tga)).unwrap();
        assert_eq!((info.format, info.width, info.height, info.bits_per_pixel), (ImageFormat::Tga, 2, 2, 24));
        assert!(!info.has_alpha && !info.rle);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(&px[0..4], &[0, 0, 255, 255]); // topo-esquerda: azul
        assert_eq!(&px[8..12], &[255, 0, 0, 255]); // base-esquerda: vermelho
    }

    #[test]
    fn tga_32_bits_topo_para_baixo_com_alfa() {
        let mut f = tga_header(2, 1, 2, 32, 0x28);
        f.extend_from_slice(&[1, 2, 3, 100]); // topo (B,G,R,A)
        f.extend_from_slice(&[4, 5, 6, 200]);
        let info = decode_tga(&f).unwrap();
        assert!(info.has_alpha);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(px, vec![3, 2, 1, 100, 6, 5, 4, 200]);
    }

    #[test]
    fn tga_rle_e_cinza() {
        // tipo 11 (cinza RLE): run de 3 pixels 0x40 + 1 literal 0x80
        let mut f = tga_header(11, 2, 2, 8, 0x20);
        f.extend_from_slice(&[0x82, 0x40, 0x00, 0x80]);
        let info = decode_tga(&f).unwrap();
        assert!(info.rle);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(&px[0..4], &[0x40, 0x40, 0x40, 255]);
        assert_eq!(&px[12..16], &[0x80, 0x80, 0x80, 255]);
    }

    #[test]
    fn tga_16_bits_e_paletizado() {
        // 16 bits A1R5G5B5 sem bits de atributo no descritor: opaco; vermelho puro = 0x7C00
        let mut f = tga_header(2, 1, 1, 16, 0x20);
        f.extend_from_slice(&0x7C00u16.to_le_bytes());
        let (_, _, px) = decode_png_rgba(&decode_tga(&f).unwrap().png);
        assert_eq!(px, vec![255, 0, 0, 255]);

        // paletizado (tipo 1): tabela de 2 entradas de 24 bits, 2x1 de índices
        let mut p = vec![0, 1, 1];
        p.extend_from_slice(&0u16.to_le_bytes()); // first
        p.extend_from_slice(&2u16.to_le_bytes()); // len
        p.push(24);
        p.extend_from_slice(&[0; 4]);
        p.extend_from_slice(&2u16.to_le_bytes());
        p.extend_from_slice(&1u16.to_le_bytes());
        p.push(8);
        p.push(0x20);
        p.extend_from_slice(&[0, 0, 255, 255, 0, 0]); // cor 0 = vermelho, cor 1 = azul (BGR)
        p.extend_from_slice(&[1, 0]);
        let info = decode_tga(&p).unwrap();
        assert_eq!(info.palette, PaletteKind::Embedded);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(px, vec![0, 0, 255, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn tga_lixo_e_truncado() {
        assert!(decode_tga(&[]).is_err());
        assert!(decode_tga(&[0; 17]).is_err());
        // dados curtos
        let mut f = tga_header(2, 8, 8, 24, 0);
        f.extend_from_slice(&[0; 20]);
        assert!(decode_tga(&f).is_err());
        // RLE que mente sobre o tamanho (4096x4096 com 5 bytes de dados): recusa sem alocar
        let mut r = tga_header(10, 4096, 4096, 32, 0);
        r.extend_from_slice(&[0x7f, 1, 2, 3, 4]);
        assert!(decode_tga(&r).is_err());
        // acima do teto de dimensões
        let big = tga_header(2, 5000, 5000, 24, 0);
        assert!(decode_tga(&big).is_err());
    }

    // ---- LMP

    #[test]
    fn lmp_com_paleta_embutida() {
        let mut f = Vec::new();
        f.extend_from_slice(&2u32.to_le_bytes());
        f.extend_from_slice(&1u32.to_le_bytes());
        f.extend_from_slice(&[10, 255]);
        f.extend_from_slice(&256u16.to_le_bytes());
        f.extend_from_slice(&pal768());
        let info = decode_image(&f, Some(ImageFormat::Lmp)).unwrap();
        assert_eq!((info.format, info.width, info.height), (ImageFormat::Lmp, 2, 1));
        assert_eq!(info.palette, PaletteKind::Embedded);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(px, vec![10, 245, 7, 255, 255, 0, 7, 255]);
    }

    #[test]
    fn lmp_sem_paleta_vira_cinza() {
        let mut f = Vec::new();
        f.extend_from_slice(&2u32.to_le_bytes());
        f.extend_from_slice(&1u32.to_le_bytes());
        f.extend_from_slice(&[10, 200]);
        let info = decode_lmp(&f).unwrap();
        assert_eq!(info.palette, PaletteKind::Grayscale);
        let (_, _, px) = decode_png_rgba(&info.png);
        assert_eq!(px, vec![10, 10, 10, 255, 200, 200, 200, 255]);
    }

    #[test]
    fn lmp_truncado_e_absurdo() {
        let mut f = Vec::new();
        f.extend_from_slice(&4u32.to_le_bytes());
        f.extend_from_slice(&4u32.to_le_bytes());
        f.extend_from_slice(&[1, 2, 3]);
        assert!(matches!(decode_lmp(&f), Err(ImageError::Truncated(_))));
        assert!(matches!(decode_lmp(&[0xff; 8]), Err(ImageError::Invalid(_))));
        assert!(decode_lmp(&[1, 2]).is_err());
    }

    // ---- roteamento e robustez

    #[test]
    fn sem_dica_e_sem_assinatura_e_desconhecido() {
        assert_eq!(decode_image(&[1, 2, 3, 4], None).unwrap_err(), ImageError::Unknown);
        assert!(decode_image(&[], None).is_err());
        assert!(decode_image(b"BM", Some(ImageFormat::Tga)).is_err());
    }

    #[test]
    fn lixo_aleatorio_nao_da_panic() {
        // sequência pseudoaleatória determinística (xorshift) em vários tamanhos e cabeçalhos
        let mut s = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for len in [0usize, 1, 2, 17, 18, 26, 54, 64, 300, 2000] {
            for _ in 0..40 {
                let mut buf: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                for hint in [None, Some(ImageFormat::Tga), Some(ImageFormat::Bmp), Some(ImageFormat::Lmp)] {
                    let _ = decode_image(&buf, hint);
                }
                // e com assinatura BM / cabeçalho de TGA plausível
                if len >= 2 {
                    buf[0] = b'B';
                    buf[1] = b'M';
                    let _ = decode_image(&buf, None);
                }
                if len >= 18 {
                    buf[0] = 0;
                    buf[1] = (next() % 2) as u8;
                    buf[2] = [1u8, 2, 3, 9, 10, 11][(next() % 6) as usize];
                    buf[16] = [8u8, 16, 24, 32][(next() % 4) as usize];
                    let _ = decode_image(&buf, Some(ImageFormat::Tga));
                }
            }
        }
    }

    #[test]
    fn bmp_truncado_em_todos_os_tamanhos_nao_da_panic() {
        let full = bmp(5, 3, 8, 0, &rgb_palette(), &[0, 1, 2, 3, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 2, 2, 2, 2, 2, 0, 0, 0]);
        assert!(decode_bmp(&full).is_ok());
        for cut in 0..full.len() {
            let _ = decode_bmp(&full[..cut]);
        }
        let rle = bmp(4, 2, 8, 1, &rgb_palette(), &[4, 1, 0, 0, 0, 3, 0, 2, 3, 0, 1, 1, 0, 1]);
        for cut in 0..rle.len() {
            let _ = decode_bmp(&rle[..cut]);
        }
    }

    #[test]
    fn load_image_confere_caminho_e_extensao() {
        let dir = std::env::temp_dir().join(format!("bspm-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("spray.bmp");
        std::fs::write(&ok, bmp(1, 1, 24, 0, &[], &[1, 2, 3, 0])).unwrap();
        let info = load_image(&ok).unwrap();
        assert_eq!((info.width, info.height), (1, 1));
        // extensão fora da lista
        let txt = dir.join("leia.txt");
        std::fs::write(&txt, b"oi").unwrap();
        assert!(load_image(&txt).unwrap_err().contains("extensão"));
        // pasta com nome de imagem
        let pasta = dir.join("falsa.tga");
        std::fs::create_dir_all(&pasta).unwrap();
        assert!(load_image(&pasta).unwrap_err().contains("não é um arquivo"));
        // `..` e inexistente
        assert!(load_image(&dir.join("..").join("x.bmp")).is_err());
        assert!(load_image(&dir.join("nao-existe.tga")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
