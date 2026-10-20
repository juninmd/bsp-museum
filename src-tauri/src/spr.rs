//! Sprites `.spr` do GoldSrc (versão 2, `IDSP`).
//!
//! Layout (little-endian), como em `spritegen`/`sprite.h` do SDK e no loader do Xash3D:
//!
//! ```text
//! cabeçalho (40 bytes)
//!   i32 ident = "IDSP"      i32 version = 2
//!   i32 type                (0 parallel_upright, 1 facing_upright, 2 parallel,
//!                            3 oriented, 4 parallel_oriented)
//!   i32 texFormat           (0 normal, 1 additive, 2 index-alpha, 3 alpha-test)
//!   f32 boundingradius      i32 width, height (maior frame)
//!   i32 numframes           f32 beamlength     i32 synchtype (0 sync, 1 random)
//! i16 colors (256)  +  paleta RGB de `colors * 3` bytes
//! numframes x:
//!   i32 frametype           0 = frame simples, 1 = grupo
//!   grupo: i32 n, f32 intervalo[n] (tempo ACUMULADO até o fim de cada frame)
//!   cada frame: i32 origem_x, origem_y, largura, altura, depois largura*altura índices
//! ```
//!
//! Transparência (decidida aqui, igual ao motor): `alpha-test` => índice 255 transparente;
//! `index-alpha` => alpha = o próprio índice e cor = última cor da paleta; `additive` => o
//! PNG sai opaco e o shader deve tratar preto como transparente (a flag `additive` avisa);
//! `normal` => opaco. Sprite do Quake (versão 1, sem paleta) não é lido: o erro diz isso.
//!
//! Como o resto do núcleo, o arquivo é tratado como hostil: leitura checada via
//! `bsp::reader`, dimensões e quantidades com teto, nenhum panic.

use crate::bsp::palette::decode_indexed;
use crate::bsp::reader::{BspError, Cursor};
use crate::bsp::rgba_png;
use serde::Serialize;
use std::fmt;
use std::path::Path;

pub const SPR_MAGIC: i32 = i32::from_le_bytes(*b"IDSP");
pub const SPR_VERSION: i32 = 2;
/// Sprite maior que isto não é sprite de mod (os do jogo base têm no máximo algumas centenas de KB).
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
/// Teto de frames no cabeçalho, de frames por grupo e de imagens no arquivo todo.
pub const MAX_FRAMES: usize = 1024;
pub const MAX_IMAGES: usize = 4096;
/// Lado máximo de um frame (os sprites do jogo têm no máximo 512).
pub const MAX_SIDE: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpriteType {
    ParallelUpright,
    FacingUpright,
    Parallel,
    Oriented,
    ParallelOriented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpriteTexFormat {
    Normal,
    Additive,
    IndexAlpha,
    AlphaTest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncType {
    Sync,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    Single,
    Group,
}

/// Uma imagem do sprite (um quadro de animação).
#[derive(Debug, Clone, Serialize)]
pub struct SpriteImage {
    /// deslocamento do canto superior esquerdo em relação ao ponto de origem do sprite
    /// (normalmente negativo: `-largura/2`)
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    /// `data:image/png;base64,...` já com a transparência da `tex_format` aplicada
    pub png: String,
}

/// Entrada da lista de frames do arquivo: um frame simples (1 imagem) ou um grupo animado.
#[derive(Debug, Clone, Serialize)]
pub struct SpriteFrame {
    pub kind: FrameKind,
    /// segundos que cada imagem do grupo fica na tela (vazio num frame simples). O arquivo
    /// guarda o tempo acumulado; aqui já vem convertido em duração por imagem.
    pub durations: Vec<f32>,
    pub images: Vec<SpriteImage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpriteInfo {
    pub version: i32,
    pub sprite_type: SpriteType,
    pub tex_format: SpriteTexFormat,
    pub bounding_radius: f32,
    /// maior largura/altura entre os frames, como o cabeçalho declara
    pub max_width: u32,
    pub max_height: u32,
    /// `numframes` do cabeçalho (== `frames.len()`)
    pub frame_count: u32,
    pub beam_length: f32,
    pub sync_type: SyncType,
    /// `tex_format == additive`: o shader deve tratar preto como transparente
    pub additive: bool,
    /// entradas de paleta gravadas no arquivo (o motor espera 256)
    pub palette_colors: u16,
    /// soma das durações dos grupos, em segundos (0 se não há grupo)
    pub total_duration: f32,
    pub frames: Vec<SpriteFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SprError {
    /// leitura além do fim do arquivo
    Truncated(BspError),
    BadMagic,
    BadVersion(i32),
    /// campo fora do que o formato permite (mensagem legível)
    Invalid(String),
}

impl fmt::Display for SprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SprError::Truncated(e) => write!(f, "sprite truncado: {e}"),
            SprError::BadMagic => write!(f, "não é um sprite do GoldSrc (assinatura IDSP ausente)"),
            SprError::BadVersion(1) => {
                write!(f, "sprite do Quake (versão 1, sem paleta embutida) não é suportado — só o do GoldSrc (versão 2)")
            }
            SprError::BadVersion(v) => write!(f, "versão {v} de sprite não suportada (esperado 2)"),
            SprError::Invalid(msg) => write!(f, "sprite inválido: {msg}"),
        }
    }
}

impl std::error::Error for SprError {}

impl From<BspError> for SprError {
    fn from(e: BspError) -> Self {
        SprError::Truncated(e)
    }
}

fn invalid<T>(msg: impl Into<String>) -> Result<T, SprError> {
    Err(SprError::Invalid(msg.into()))
}

/// Float de arquivo hostil: NaN/infinito não serializa em JSON (vira `null`) e quebraria a UI.
fn finite(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

fn sprite_type_of(v: i32) -> Result<SpriteType, SprError> {
    Ok(match v {
        0 => SpriteType::ParallelUpright,
        1 => SpriteType::FacingUpright,
        2 => SpriteType::Parallel,
        3 => SpriteType::Oriented,
        4 => SpriteType::ParallelOriented,
        _ => return invalid(format!("tipo de sprite {v} desconhecido")),
    })
}

fn tex_format_of(v: i32) -> Result<SpriteTexFormat, SprError> {
    Ok(match v {
        0 => SpriteTexFormat::Normal,
        1 => SpriteTexFormat::Additive,
        2 => SpriteTexFormat::IndexAlpha,
        3 => SpriteTexFormat::AlphaTest,
        _ => return invalid(format!("texFormat {v} desconhecido")),
    })
}

/// Pixels indexados -> RGBA segundo o `texFormat`. `palette` tem sempre 768 bytes.
fn colorize(pixels: &[u8], palette: &[u8], format: SpriteTexFormat) -> Option<Vec<u8>> {
    match format {
        SpriteTexFormat::Normal | SpriteTexFormat::Additive => decode_indexed(pixels, palette, false),
        SpriteTexFormat::AlphaTest => decode_indexed(pixels, palette, true),
        SpriteTexFormat::IndexAlpha => {
            // cor única (a última da paleta); o índice vira a opacidade
            let color = palette.get(765..768)?;
            let mut rgba = Vec::with_capacity(pixels.len() * 4);
            for &idx in pixels {
                rgba.extend_from_slice(&[color[0], color[1], color[2], idx]);
            }
            Some(rgba)
        }
    }
}

/// Lê um `dspriteframe_t` + pixels e já converte em imagem.
fn read_image(cur: &mut Cursor, palette: &[u8], format: SpriteTexFormat) -> Result<SpriteImage, SprError> {
    let origin_x = cur.i32()?;
    let origin_y = cur.i32()?;
    let w = cur.i32()?;
    let h = cur.i32()?;
    if w <= 0 || h <= 0 || w as usize > MAX_SIDE || h as usize > MAX_SIDE {
        return invalid(format!("frame de {w}x{h} (máximo {MAX_SIDE} por lado)"));
    }
    let (w, h) = (w as usize, h as usize);
    let pixels = cur.bytes(w * h)?;
    let rgba = colorize(pixels, palette, format).ok_or_else(|| SprError::Invalid("paleta ilegível".into()))?;
    let png = rgba_png(w, h, &rgba).ok_or_else(|| SprError::Invalid("não montou o PNG".into()))?;
    Ok(SpriteImage { origin_x, origin_y, width: w as u32, height: h as u32, png })
}

/// Decodifica um `.spr` do GoldSrc já em memória.
pub fn parse_sprite(data: &[u8]) -> Result<SpriteInfo, SprError> {
    let mut cur = Cursor::new(data);
    if cur.i32()? != SPR_MAGIC {
        return Err(SprError::BadMagic);
    }
    let version = cur.i32()?;
    if version != SPR_VERSION {
        return Err(SprError::BadVersion(version));
    }
    let sprite_type = sprite_type_of(cur.i32()?)?;
    let tex_format = tex_format_of(cur.i32()?)?;
    let bounding_radius = finite(cur.f32()?);
    let max_width = cur.i32()?;
    let max_height = cur.i32()?;
    let num_frames = cur.i32()?;
    let beam_length = finite(cur.f32()?);
    // synchtype fora de 0/1 não impede de abrir: o motor trata qualquer outro valor como "sync"
    let sync_type = if cur.i32()? == 1 { SyncType::Random } else { SyncType::Sync };

    if num_frames < 0 || num_frames as usize > MAX_FRAMES {
        return invalid(format!("{num_frames} frames no cabeçalho (máximo {MAX_FRAMES})"));
    }

    let colors = cur.i16()? as u16;
    if colors == 0 || colors > 256 {
        return invalid(format!("paleta com {colors} cores (esperado 256)"));
    }
    let mut palette = cur.bytes(colors as usize * 3)?.to_vec();
    palette.resize(768, 0);

    let mut frames = Vec::new();
    let mut images = 0usize;
    let mut total_duration = 0.0f32;
    for _ in 0..num_frames {
        let kind = cur.i32()?;
        match kind {
            0 => {
                images += 1;
                if images > MAX_IMAGES {
                    return invalid(format!("mais de {MAX_IMAGES} imagens"));
                }
                let image = read_image(&mut cur, &palette, tex_format)?;
                frames.push(SpriteFrame { kind: FrameKind::Single, durations: Vec::new(), images: vec![image] });
            }
            1 => {
                let n = cur.i32()?;
                if n <= 0 || n as usize > MAX_FRAMES {
                    return invalid(format!("grupo com {n} frames (máximo {MAX_FRAMES})"));
                }
                let n = n as usize;
                images += n;
                if images > MAX_IMAGES {
                    return invalid(format!("mais de {MAX_IMAGES} imagens"));
                }
                // intervalos acumulados -> duração de cada imagem
                let mut durations = Vec::with_capacity(n);
                let mut previous = 0.0f32;
                for _ in 0..n {
                    let end = finite(cur.f32()?);
                    durations.push((end - previous).max(0.0));
                    previous = end.max(previous);
                }
                total_duration += previous;
                let mut group = Vec::with_capacity(n);
                for _ in 0..n {
                    group.push(read_image(&mut cur, &palette, tex_format)?);
                }
                frames.push(SpriteFrame { kind: FrameKind::Group, durations, images: group });
            }
            other => return invalid(format!("tipo de frame {other} desconhecido (esperado 0 ou 1)")),
        }
    }

    Ok(SpriteInfo {
        version,
        sprite_type,
        tex_format,
        bounding_radius,
        max_width: max_width.max(0) as u32,
        max_height: max_height.max(0) as u32,
        frame_count: frames.len() as u32,
        beam_length,
        sync_type,
        additive: tex_format == SpriteTexFormat::Additive,
        palette_colors: colors,
        total_duration: finite(total_duration),
        frames,
    })
}

/// Lê e decodifica um `.spr` do disco (caminho conferido: arquivo comum, extensão `.spr`, tamanho limitado).
pub fn load_sprite(path: &Path) -> Result<SpriteInfo, String> {
    let bytes = crate::util::read_checked(path, &["spr"], MAX_FILE_BYTES)?;
    parse_sprite(&bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::decode_png_rgba;

    /// Paleta de teste: índice i -> (i, 255 - i, 9), exceto o 255 -> (200, 10, 10).
    fn palette() -> Vec<u8> {
        let mut p: Vec<u8> = (0..=255u8).flat_map(|i| [i, 255 - i, 9]).collect();
        p[765..768].copy_from_slice(&[200, 10, 10]);
        p
    }

    struct Img {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        px: Vec<u8>,
    }

    enum Entry {
        Single(Img),
        Group(Vec<f32>, Vec<Img>),
    }

    fn img(w: i32, h: i32, px: &[u8]) -> Img {
        Img { x: -w / 2, y: h / 2, w, h, px: px.to_vec() }
    }

    fn put_img(out: &mut Vec<u8>, i: &Img) {
        for v in [i.x, i.y, i.w, i.h] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&i.px);
    }

    fn header(kind: i32, fmt: i32, nframes: i32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"IDSP");
        out.extend_from_slice(&2i32.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&fmt.to_le_bytes());
        out.extend_from_slice(&16.5f32.to_le_bytes()); // boundingradius
        out.extend_from_slice(&8i32.to_le_bytes()); // width
        out.extend_from_slice(&4i32.to_le_bytes()); // height
        out.extend_from_slice(&nframes.to_le_bytes());
        out.extend_from_slice(&32.0f32.to_le_bytes()); // beamlength
        out.extend_from_slice(&1i32.to_le_bytes()); // synchtype = random
        out
    }

    /// `.spr` byte a byte.
    fn spr(kind: i32, fmt: i32, entries: &[Entry]) -> Vec<u8> {
        let mut out = header(kind, fmt, entries.len() as i32);
        out.extend_from_slice(&256i16.to_le_bytes());
        out.extend_from_slice(&palette());
        for e in entries {
            match e {
                Entry::Single(i) => {
                    out.extend_from_slice(&0i32.to_le_bytes());
                    put_img(&mut out, i);
                }
                Entry::Group(intervals, imgs) => {
                    out.extend_from_slice(&1i32.to_le_bytes());
                    out.extend_from_slice(&(imgs.len() as i32).to_le_bytes());
                    for t in intervals {
                        out.extend_from_slice(&t.to_le_bytes());
                    }
                    for i in imgs {
                        put_img(&mut out, i);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn um_frame_simples_com_metadados_e_pixels() {
        let file = spr(2, 0, &[Entry::Single(img(2, 2, &[0, 1, 2, 3]))]);
        let s = parse_sprite(&file).unwrap();
        assert_eq!(s.version, 2);
        assert_eq!(s.sprite_type, SpriteType::Parallel);
        assert_eq!(s.tex_format, SpriteTexFormat::Normal);
        assert_eq!(s.bounding_radius, 16.5);
        assert_eq!((s.max_width, s.max_height), (8, 4));
        assert_eq!(s.frame_count, 1);
        assert_eq!(s.beam_length, 32.0);
        assert_eq!(s.sync_type, SyncType::Random);
        assert!(!s.additive);
        assert_eq!(s.palette_colors, 256);
        assert_eq!(s.total_duration, 0.0);
        assert_eq!(s.frames.len(), 1);
        let f = &s.frames[0];
        assert_eq!(f.kind, FrameKind::Single);
        assert!(f.durations.is_empty());
        assert_eq!(f.images.len(), 1);
        let i = &f.images[0];
        assert_eq!((i.origin_x, i.origin_y, i.width, i.height), (-1, 1, 2, 2));
        let (w, h, px) = decode_png_rgba(&i.png);
        assert_eq!((w, h), (2, 2));
        assert_eq!(&px[0..4], &[0, 255, 9, 255]);
        assert_eq!(&px[4..8], &[1, 254, 9, 255]);
        assert_eq!(&px[12..16], &[3, 252, 9, 255]);
    }

    #[test]
    fn cada_tex_format_aplica_a_transparencia_certa() {
        let px = [0u8, 128, 255];
        let alpha_of = |fmt: i32| {
            let s = parse_sprite(&spr(2, fmt, &[Entry::Single(img(3, 1, &px))])).unwrap();
            let (_, _, rgba) = decode_png_rgba(&s.frames[0].images[0].png);
            (s, rgba)
        };

        // normal: tudo opaco, 255 é só uma cor
        let (s, rgba) = alpha_of(0);
        assert_eq!(s.tex_format, SpriteTexFormat::Normal);
        assert!(!s.additive);
        assert_eq!([rgba[3], rgba[7], rgba[11]], [255, 255, 255]);
        assert_eq!(&rgba[8..11], &[200, 10, 10]);

        // additive: PNG opaco (preto vira transparente no shader) + a flag
        let (s, rgba) = alpha_of(1);
        assert_eq!(s.tex_format, SpriteTexFormat::Additive);
        assert!(s.additive);
        assert_eq!([rgba[3], rgba[7], rgba[11]], [255, 255, 255]);
        assert_eq!(&rgba[0..3], &[0, 255, 9], "índice 0 continua sendo a cor 0 da paleta");

        // index-alpha: alpha = índice, cor = última da paleta
        let (s, rgba) = alpha_of(2);
        assert_eq!(s.tex_format, SpriteTexFormat::IndexAlpha);
        assert!(!s.additive);
        assert_eq!(rgba, vec![200, 10, 10, 0, 200, 10, 10, 128, 200, 10, 10, 255]);

        // alpha-test: só o 255 fura
        let (s, rgba) = alpha_of(3);
        assert_eq!(s.tex_format, SpriteTexFormat::AlphaTest);
        assert_eq!([rgba[3], rgba[7], rgba[11]], [255, 255, 0]);
    }

    #[test]
    fn grupo_converte_intervalos_acumulados_em_duracoes() {
        let g = Entry::Group(
            vec![0.1, 0.3, 0.6],
            vec![img(1, 1, &[1]), img(1, 1, &[2]), img(2, 1, &[3, 4])],
        );
        let s = parse_sprite(&spr(0, 1, &[Entry::Single(img(1, 1, &[0])), g])).unwrap();
        assert_eq!(s.frame_count, 2);
        assert_eq!(s.frames[0].kind, FrameKind::Single);
        let group = &s.frames[1];
        assert_eq!(group.kind, FrameKind::Group);
        assert_eq!(group.images.len(), 3);
        assert_eq!(group.images[2].width, 2);
        let d = &group.durations;
        assert_eq!(d.len(), 3);
        assert!((d[0] - 0.1).abs() < 1e-6 && (d[1] - 0.2).abs() < 1e-6 && (d[2] - 0.3).abs() < 1e-6);
        assert!((s.total_duration - 0.6).abs() < 1e-6);
        assert_eq!(s.sprite_type, SpriteType::ParallelUpright);
    }

    #[test]
    fn intervalo_que_volta_no_tempo_vira_duracao_zero() {
        let g = Entry::Group(vec![0.5, 0.2, f32::NAN], vec![img(1, 1, &[1]), img(1, 1, &[1]), img(1, 1, &[1])]);
        let s = parse_sprite(&spr(0, 0, &[g])).unwrap();
        assert_eq!(s.frames[0].durations, vec![0.5, 0.0, 0.0]);
        assert!(s.total_duration.is_finite());
    }

    #[test]
    fn todos_os_tipos_de_sprite() {
        let expected = [
            SpriteType::ParallelUpright,
            SpriteType::FacingUpright,
            SpriteType::Parallel,
            SpriteType::Oriented,
            SpriteType::ParallelOriented,
        ];
        for (code, want) in expected.iter().enumerate() {
            let s = parse_sprite(&spr(code as i32, 0, &[Entry::Single(img(1, 1, &[0]))])).unwrap();
            assert_eq!(s.sprite_type, *want);
        }
        assert!(matches!(parse_sprite(&spr(5, 0, &[])), Err(SprError::Invalid(_))));
        assert!(matches!(parse_sprite(&spr(0, 4, &[])), Err(SprError::Invalid(_))));
    }

    #[test]
    fn serializa_com_os_nomes_do_contrato() {
        let g = Entry::Group(vec![0.5], vec![img(1, 1, &[1])]);
        let s = parse_sprite(&spr(3, 2, &[g])).unwrap();
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["sprite_type"], "oriented");
        assert_eq!(json["tex_format"], "index_alpha");
        assert_eq!(json["sync_type"], "random");
        assert_eq!(json["frames"][0]["kind"], "group");
        assert_eq!(json["frames"][0]["images"][0]["origin_x"], 0);
        assert!(json["frames"][0]["images"][0]["png"].as_str().unwrap().starts_with("data:image/png;base64,"));
    }

    #[test]
    fn float_nao_finito_no_cabecalho_vira_zero() {
        let mut file = spr(0, 0, &[Entry::Single(img(1, 1, &[0]))]);
        file[16..20].copy_from_slice(&f32::NAN.to_le_bytes()); // boundingradius
        file[32..36].copy_from_slice(&f32::INFINITY.to_le_bytes()); // beamlength
        let s = parse_sprite(&file).unwrap();
        assert_eq!((s.bounding_radius, s.beam_length), (0.0, 0.0));
        assert!(serde_json::to_string(&s).unwrap().contains("\"bounding_radius\":0.0"));
    }

    #[test]
    fn rejeita_assinatura_e_versao() {
        let good = spr(0, 0, &[Entry::Single(img(1, 1, &[0]))]);
        let mut bad = good.clone();
        bad[0] = b'X';
        assert_eq!(parse_sprite(&bad).unwrap_err(), SprError::BadMagic);

        let mut quake = good.clone();
        quake[4..8].copy_from_slice(&1i32.to_le_bytes());
        let err = parse_sprite(&quake).unwrap_err();
        assert_eq!(err, SprError::BadVersion(1));
        assert!(err.to_string().contains("Quake"));

        let mut v3 = good;
        v3[4..8].copy_from_slice(&3i32.to_le_bytes());
        assert_eq!(parse_sprite(&v3).unwrap_err(), SprError::BadVersion(3));
    }

    #[test]
    fn rejeita_quantidades_e_dimensoes_absurdas() {
        // frames demais no cabeçalho
        let mut file = spr(0, 0, &[Entry::Single(img(1, 1, &[0]))]);
        file[28..32].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(matches!(parse_sprite(&file), Err(SprError::Invalid(_))));
        file[28..32].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(matches!(parse_sprite(&file), Err(SprError::Invalid(_))));

        // paleta com 0 ou 1000 cores
        for colors in [0i16, 1000, -5] {
            let mut f = spr(0, 0, &[Entry::Single(img(1, 1, &[0]))]);
            f[40..42].copy_from_slice(&colors.to_le_bytes());
            assert!(matches!(parse_sprite(&f), Err(SprError::Invalid(_))), "colors={colors}");
        }

        // frame de 100000x100000 (sem alocar) e frame de largura negativa
        for (w, h) in [(100_000, 100_000), (-4, 4), (0, 4)] {
            let f = spr(0, 0, &[Entry::Single(Img { x: 0, y: 0, w, h, px: vec![0; 4] })]);
            assert!(matches!(parse_sprite(&f), Err(SprError::Invalid(_))), "{w}x{h}");
        }

        // grupo com n gigante / negativo e frametype desconhecido
        let mut f = spr(0, 0, &[Entry::Group(vec![1.0], vec![img(1, 1, &[0])])]);
        let at = 42 + 768 + 4;
        f[at..at + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(matches!(parse_sprite(&f), Err(SprError::Invalid(_))));
        f[at..at + 4].copy_from_slice(&(-3i32).to_le_bytes());
        assert!(matches!(parse_sprite(&f), Err(SprError::Invalid(_))));
        let mut f = spr(0, 0, &[Entry::Single(img(1, 1, &[0]))]);
        let at = 42 + 768;
        f[at..at + 4].copy_from_slice(&7i32.to_le_bytes());
        assert!(matches!(parse_sprite(&f), Err(SprError::Invalid(_))));
    }

    #[test]
    fn truncado_em_qualquer_ponto_da_erro_sem_panic() {
        let g = Entry::Group(vec![0.1, 0.2], vec![img(2, 2, &[1, 2, 3, 4]), img(2, 2, &[4, 3, 2, 1])]);
        let file = spr(2, 2, &[Entry::Single(img(3, 1, &[0, 128, 255])), g]);
        assert!(parse_sprite(&file).is_ok());
        for cut in 0..file.len() {
            assert!(parse_sprite(&file[..cut]).is_err(), "corte em {cut} deveria falhar");
        }
    }

    #[test]
    fn lixo_aleatorio_nao_da_panic() {
        let mut s = 0x1234_5678_9ABC_DEF1u64;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for len in [0usize, 3, 4, 8, 40, 44, 100, 900, 3000] {
            for _ in 0..30 {
                let mut buf: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                let _ = parse_sprite(&buf);
                // com cabeçalho válido e o resto aleatório
                if len >= 44 {
                    buf[0..4].copy_from_slice(b"IDSP");
                    buf[4..8].copy_from_slice(&2i32.to_le_bytes());
                    buf[8..12].copy_from_slice(&((next() % 5) as i32).to_le_bytes());
                    buf[12..16].copy_from_slice(&((next() % 4) as i32).to_le_bytes());
                    buf[28..32].copy_from_slice(&((next() % 5) as i32).to_le_bytes());
                    buf[40..42].copy_from_slice(&256i16.to_le_bytes());
                    let _ = parse_sprite(&buf);
                }
            }
        }
    }

    #[test]
    fn load_sprite_confere_caminho_e_extensao() {
        let dir = std::env::temp_dir().join(format!("bspm-spr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("flare.SPR");
        std::fs::write(&ok, spr(2, 1, &[Entry::Single(img(2, 2, &[0, 1, 2, 3]))])).unwrap();
        let s = load_sprite(&ok).unwrap();
        assert!(s.additive);

        let lixo = dir.join("lixo.spr");
        std::fs::write(&lixo, b"nao sou sprite").unwrap();
        assert!(load_sprite(&lixo).unwrap_err().contains("IDSP"));

        let outro = dir.join("flare.png");
        std::fs::write(&outro, b"x").unwrap();
        assert!(load_sprite(&outro).unwrap_err().contains("extensão"));

        assert!(load_sprite(&dir.join("..").join("flare.spr")).is_err());
        assert!(load_sprite(&dir.join("nao-existe.spr")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
