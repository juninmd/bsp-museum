//! Áudio do mod: metadados de WAV e MP3, envelope de forma de onda para o canvas e
//! leitura do arquivo como `data:` URL para tocar no `<audio>` do WebView.
//!
//! Nada é decodificado além do necessário: o WAV PCM é varrido uma vez para montar o
//! envelope (pares min/max por trecho); ADPCM e afins só são **identificados**; o MP3
//! tem apenas o primeiro frame, as tags e o tamanho lidos.
//!
//! Conferido contra código real: a regra de loop do GoldSrc/Quake — o primeiro ponto do
//! chunk `cue ` é o início do loop, e um `LIST` logo depois com `ltxt` de propósito
//! `mark` dá o comprimento — vem de `GetWavinfo` em `WinQuake/snd_mem.c` (id Software).
//! O GoldSrc lê o mesmo (de memória; o código do engine HL não é público).
//!
//! Não conferido contra arquivo real: nenhum WAV/MP3 da Valve está no repositório. O
//! layout do `smpl`, do `fact` e das tags ID3 vem do formato público (RIFF/MS, ID3.org)
//! e de memória, sem teste contra amostra de jogo.

use base64::Engine as _;
use serde::Serialize;

/// Limite padrão para ler um áudio inteiro (análise e `data:` URL).
pub const MAX_AUDIO_BYTES: u64 = 32 * 1024 * 1024;
/// Número de trechos do envelope quando o chamador não escolhe.
pub const DEFAULT_ENVELOPE: usize = 512;
/// Teto de trechos do envelope (o canvas não precisa de mais).
pub const MAX_ENVELOPE: usize = 4096;
const MAX_WARNINGS: usize = 20;
const MAX_CUES: usize = 4096;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TextTag {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CuePoint {
    pub id: u32,
    /// posição em amostras (frames) a partir do início do `data`
    pub sample_offset: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LoopPoint {
    pub start_sample: u32,
    /// `None` = até o fim do arquivo (caso do `cue` sem `ltxt`)
    pub end_sample: Option<u32>,
    /// `"cue"` (regra do GoldSrc/ambient_generic) ou `"smpl"`
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AudioInfo {
    /// `"wav"` ou `"mp3"`
    pub container: String,
    /// `"pcm"`, `"float"`, `"ms-adpcm"`, `"ima-adpcm"`, `"alaw"`, `"mulaw"`, `"mp3"`, `"desconhecido"`…
    pub codec: String,
    /// `wFormatTag` do WAV (já resolvido quando é `WAVE_FORMAT_EXTENSIBLE`)
    pub format_tag: Option<u16>,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub block_align: u16,
    pub bitrate_kbps: Option<u32>,
    /// total de frames de amostra (uma amostra de cada canal), quando se sabe
    pub sample_frames: Option<u64>,
    pub duration_secs: f64,
    /// `true` quando a duração é estimada (ADPCM pelo byte-rate, MP3 CBR pelo tamanho)
    pub duration_approx: bool,
    pub size_bytes: u64,
    /// o `<audio>` do WebView toca (PCM/float e MP3); ADPCM/A-law não
    pub playable: bool,
    pub cue_points: Vec<CuePoint>,
    pub loops: Vec<LoopPoint>,
    /// `LIST/INFO` do WAV (INAM, IART…) ou ID3 do MP3 (title, artist, album, year, genre, track)
    pub tags: Vec<TextTag>,
    /// maior valor absoluto da onda (0..1); `None` sem envelope
    pub peak: Option<f32>,
    /// pares `[min, max]` em -1..1 (escala cheia), um por trecho; `None` se não for PCM
    pub envelope: Option<Vec<[f32; 2]>>,
    pub warnings: Vec<String>,
}

/// Áudio pronto para o `<audio src=...>`.
#[derive(Debug, Clone, Serialize)]
pub struct AudioData {
    pub mime: String,
    pub size_bytes: u64,
    pub data_url: String,
}

fn warn(info: &mut AudioInfo, msg: String) {
    if info.warnings.len() < MAX_WARNINGS {
        info.warnings.push(msg);
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

fn cstr_latin1(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    latin1(&b[..end]).trim().to_string()
}

fn fourcc(id: &[u8]) -> String {
    latin1(id)
}

// ---------------------------------------------------------------------------------
// WAV
// ---------------------------------------------------------------------------------

struct Fmt {
    tag: u16,
    channels: u16,
    rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits: u16,
}

fn codec_name(tag: u16) -> String {
    match tag {
        0x0001 => "pcm".into(),
        0x0002 => "ms-adpcm".into(),
        0x0003 => "float".into(),
        0x0006 => "alaw".into(),
        0x0007 => "mulaw".into(),
        0x0011 => "ima-adpcm".into(),
        0x0055 => "mp3".into(),
        other => format!("desconhecido(0x{other:04x})"),
    }
}

/// Subchunks `id(4) size(4) dados` de dentro de um `LIST`, com limite de quantidade.
fn sub_chunks(mut b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut v = Vec::new();
    while b.len() >= 8 && v.len() < MAX_CUES {
        let id = [b[0], b[1], b[2], b[3]];
        let size = u32_at(b, 4).unwrap_or(0) as usize;
        let end = 8usize.saturating_add(size).min(b.len());
        v.push((id, &b[8..end]));
        let next = 8usize.saturating_add(size).saturating_add(size & 1);
        if next >= b.len() {
            break;
        }
        b = &b[next..];
    }
    v
}

fn sample_f32(tag: u16, bits: u16, c: &[u8]) -> f32 {
    let v = match (tag, bits) {
        (1, 8) => (c[0] as f32 - 128.0) / 128.0,
        (1, 16) => i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0,
        (1, 24) => (((c[2] as i8 as i32) << 16) | ((c[1] as i32) << 8) | c[0] as i32) as f32 / 8_388_608.0,
        (1, 32) => i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2_147_483_648.0,
        (3, 32) => f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
        (3, 64) => f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]) as f32,
        _ => 0.0,
    };
    if v.is_finite() {
        v.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn round3(x: f32) -> f32 {
    (x * 1000.0).round() / 1000.0
}

/// Lê o WAV e devolve metadados + envelope de `buckets` trechos (limitado a
/// [`MAX_ENVELOPE`] e ao número de frames). Chunks fora do arquivo são aparados; só
/// vira erro o que impede qualquer leitura (sem RIFF/WAVE, sem `fmt `, `fmt ` zerado).
pub fn parse_wav(bytes: &[u8], buckets: usize) -> Result<AudioInfo, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("não é um WAV: RIFF/WAVE ausente".to_string());
    }
    let mut info = AudioInfo { container: "wav".into(), size_bytes: bytes.len() as u64, ..Default::default() };
    let mut fmt: Option<Fmt> = None;
    let mut real_tag: Option<u16> = None;
    let mut data: Option<(usize, usize)> = None;
    let mut fact: Option<u32> = None;
    let mut mark_len: Option<u32> = None;

    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(bytes, pos + 4).unwrap_or(0) as usize;
        let start = pos + 8;
        let avail = bytes.len() - start;
        let clamped = size > avail;
        let end = start + size.min(avail);
        let body = &bytes[start..end];
        if clamped && id != b"data" {
            warn(&mut info, format!("chunk \"{}\" declara {size} bytes mas só restam {avail}", fourcc(id)));
        }
        match id {
            b"fmt " if fmt.is_none() => {
                if body.len() < 16 {
                    return Err(format!("chunk fmt com {} bytes (mínimo 16)", body.len()));
                }
                let f = Fmt {
                    tag: u16_at(body, 0).unwrap_or(0),
                    channels: u16_at(body, 2).unwrap_or(0),
                    rate: u32_at(body, 4).unwrap_or(0),
                    byte_rate: u32_at(body, 8).unwrap_or(0),
                    block_align: u16_at(body, 12).unwrap_or(0),
                    bits: u16_at(body, 14).unwrap_or(0),
                };
                if f.tag == 0xFFFE {
                    // WAVE_FORMAT_EXTENSIBLE: o formato de verdade são os 2 primeiros bytes do GUID
                    real_tag = if body.len() >= 26 { u16_at(body, 24) } else { None };
                    if real_tag.is_none() {
                        warn(&mut info, "fmt EXTENSIBLE sem o GUID do subformato".to_string());
                    }
                }
                fmt = Some(f);
            }
            b"data" if data.is_none() => {
                if clamped {
                    warn(
                        &mut info,
                        format!("data declara {size} bytes mas o arquivo só tem {avail}; usando o que existe"),
                    );
                }
                data = Some((start, end - start));
            }
            b"fact" if fact.is_none() => fact = u32_at(body, 0),
            b"cue " => {
                let count = u32_at(body, 0).unwrap_or(0) as usize;
                for i in 0..count.min(MAX_CUES) {
                    let at = 4 + i * 24;
                    let (Some(cid), Some(off)) = (u32_at(body, at), u32_at(body, at + 20)) else {
                        warn(&mut info, "chunk cue truncado".to_string());
                        break;
                    };
                    info.cue_points.push(CuePoint { id: cid, sample_offset: off });
                }
            }
            b"smpl" => {
                let n = u32_at(body, 28).unwrap_or(0) as usize;
                for i in 0..n.min(MAX_CUES) {
                    let at = 36 + i * 24;
                    let (Some(s), Some(e)) = (u32_at(body, at + 8), u32_at(body, at + 12)) else {
                        warn(&mut info, "chunk smpl truncado".to_string());
                        break;
                    };
                    info.loops.push(LoopPoint { start_sample: s, end_sample: Some(e), source: "smpl".into() });
                }
            }
            b"LIST" if body.len() >= 4 => match &body[0..4] {
                b"INFO" => {
                    for (sid, sdata) in sub_chunks(&body[4..]) {
                        let value = cstr_latin1(sdata);
                        if !value.is_empty() {
                            info.tags.push(TextTag { key: fourcc(&sid), value });
                        }
                    }
                }
                b"adtl" => {
                    for (sid, sdata) in sub_chunks(&body[4..]) {
                        if &sid == b"ltxt" && mark_len.is_none() && sdata.get(8..12) == Some(b"mark") {
                            mark_len = u32_at(sdata, 4);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
        pos = match end.checked_add(size & 1) {
            Some(p) => p,
            None => break,
        };
    }

    let f = fmt.ok_or_else(|| "WAV sem chunk fmt".to_string())?;
    if f.channels == 0 || f.rate == 0 {
        return Err(format!("fmt inválido: {} canais, {} Hz", f.channels, f.rate));
    }
    let tag = real_tag.unwrap_or(f.tag);
    info.format_tag = Some(tag);
    info.codec = codec_name(tag);
    info.channels = f.channels;
    info.sample_rate = f.rate;
    info.bits_per_sample = f.bits;
    info.block_align = f.block_align;
    if f.byte_rate > 0 {
        info.bitrate_kbps = Some((f.byte_rate as u64 * 8 / 1000) as u32);
    }

    let bytes_per_sample = (f.bits as usize).div_ceil(8);
    let frame_size = bytes_per_sample * f.channels as usize;
    let pcm_like = matches!((tag, f.bits), (1, 8 | 16 | 24 | 32) | (3, 32 | 64));
    info.playable = pcm_like;
    if !matches!(tag, 1 | 3) {
        let msg = format!("WAV {}: o motor do Quake só aceita PCM (GoldSrc idem, de memória); converta para PCM 16 bits", info.codec);
        warn(&mut info, msg);
    } else if tag == 1 && !pcm_like {
        warn(&mut info, format!("PCM de {} bits não é suportado para o envelope", f.bits));
    }
    if pcm_like && f.block_align as usize != frame_size {
        warn(&mut info, format!("block_align {} não bate com canais x bytes por amostra ({frame_size})", f.block_align));
    }

    let data_len = data.map_or(0, |d| d.1);
    if data.is_none() {
        warn(&mut info, "WAV sem chunk data".to_string());
    }
    if pcm_like && frame_size > 0 {
        let frames = (data_len / frame_size) as u64;
        info.sample_frames = Some(frames);
        info.duration_secs = frames as f64 / f.rate as f64;
        if data_len % frame_size != 0 {
            warn(&mut info, format!("data termina no meio de um frame ({} bytes sobrando)", data_len % frame_size));
        }
    } else if let Some(fr) = fact {
        info.sample_frames = Some(fr as u64);
        info.duration_secs = fr as f64 / f.rate as f64;
    } else if f.byte_rate > 0 {
        info.duration_secs = data_len as f64 / f.byte_rate as f64;
        info.duration_approx = true;
    }

    // loop do GoldSrc: primeiro cue = início; `ltxt mark` logo depois = comprimento
    if let Some(first) = info.cue_points.first() {
        let start = first.sample_offset;
        let end = mark_len.map(|l| start.saturating_add(l));
        info.loops.insert(0, LoopPoint { start_sample: start, end_sample: end, source: "cue".into() });
    }
    if let Some(total) = info.sample_frames {
        let bad = info.loops.iter().any(|l| l.start_sample as u64 > total || l.end_sample.is_some_and(|e| e as u64 > total));
        if bad {
            warn(&mut info, "ponto de loop depois do fim do áudio".to_string());
        }
    }

    // envelope
    if let (true, Some((dstart, dlen)), Some(frames)) = (pcm_like, data, info.sample_frames) {
        if frames > 0 && buckets > 0 {
            let body = &bytes[dstart..dstart + dlen];
            let n = buckets.clamp(1, MAX_ENVELOPE).min(frames as usize);
            let mut env = Vec::with_capacity(n);
            let mut peak = 0f32;
            for i in 0..n as u64 {
                let a = i * frames / n as u64;
                let b = ((i + 1) * frames / n as u64).max(a + 1).min(frames);
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for fr in a..b {
                    let base = fr as usize * frame_size;
                    for ch in 0..f.channels as usize {
                        let at = base + ch * bytes_per_sample;
                        if let Some(c) = body.get(at..at + bytes_per_sample) {
                            let v = sample_f32(tag, f.bits, c);
                            lo = lo.min(v);
                            hi = hi.max(v);
                        }
                    }
                }
                if lo > hi {
                    lo = 0.0;
                    hi = 0.0;
                }
                peak = peak.max(lo.abs()).max(hi.abs());
                env.push([round3(lo), round3(hi)]);
            }
            info.peak = Some(round3(peak));
            info.envelope = Some(env);
        }
    }
    Ok(info)
}

// ---------------------------------------------------------------------------------
// MP3 (só cabeçalho, tags e tamanho)
// ---------------------------------------------------------------------------------

struct Mp3Frame {
    /// 1 = MPEG-1, 2 = MPEG-2, 25 = MPEG-2.5
    version: u8,
    layer: u8,
    bitrate_kbps: u32,
    sample_rate: u32,
    channels: u16,
    samples_per_frame: u32,
    frame_len: usize,
}

const BR_V1_L1: [u32; 16] = [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0];
const BR_V1_L2: [u32; 16] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0];
const BR_V1_L3: [u32; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
const BR_V2_L1: [u32; 16] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0];
const BR_V2_L23: [u32; 16] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0];

fn parse_frame_header(b: &[u8]) -> Option<Mp3Frame> {
    if b.len() < 4 || b[0] != 0xFF || b[1] & 0xE0 != 0xE0 {
        return None;
    }
    let version = match (b[1] >> 3) & 3 {
        0 => 25,
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let layer = match (b[1] >> 1) & 3 {
        3 => 1,
        2 => 2,
        1 => 3,
        _ => return None,
    };
    let br_idx = ((b[2] >> 4) & 0xF) as usize;
    let sr_idx = ((b[2] >> 2) & 3) as usize;
    let pad = ((b[2] >> 1) & 1) as usize;
    if sr_idx == 3 {
        return None;
    }
    let table = match (version, layer) {
        (1, 1) => &BR_V1_L1,
        (1, 2) => &BR_V1_L2,
        (1, _) => &BR_V1_L3,
        (_, 1) => &BR_V2_L1,
        _ => &BR_V2_L23,
    };
    let bitrate_kbps = table[br_idx];
    if bitrate_kbps == 0 {
        return None; // 0 = "free" e 15 = inválido: não dá para achar o tamanho do frame
    }
    let sample_rate = match version {
        1 => [44100, 48000, 32000][sr_idx],
        2 => [22050, 24000, 16000][sr_idx],
        _ => [11025, 12000, 8000][sr_idx],
    };
    let channels = if (b[3] >> 6) & 3 == 3 { 1 } else { 2 };
    let (samples_per_frame, frame_len) = match (layer, version) {
        (1, _) => (384, (12 * bitrate_kbps as usize * 1000 / sample_rate as usize + pad) * 4),
        (2, _) => (1152, 144 * bitrate_kbps as usize * 1000 / sample_rate as usize + pad),
        (_, 1) => (1152, 144 * bitrate_kbps as usize * 1000 / sample_rate as usize + pad),
        _ => (576, 72 * bitrate_kbps as usize * 1000 / sample_rate as usize + pad),
    };
    if frame_len < 4 {
        return None;
    }
    Some(Mp3Frame { version, layer, bitrate_kbps, sample_rate, channels, samples_per_frame, frame_len })
}

fn synchsafe(b: &[u8]) -> Option<usize> {
    if b.len() < 4 || b.iter().take(4).any(|&x| x & 0x80 != 0) {
        return None;
    }
    Some(((b[0] as usize) << 21) | ((b[1] as usize) << 14) | ((b[2] as usize) << 7) | b[3] as usize)
}

fn decode_id3_text(enc: u8, b: &[u8]) -> String {
    let s = match enc {
        1 | 2 => {
            let (be, rest) = match b {
                [0xFF, 0xFE, r @ ..] => (false, r),
                [0xFE, 0xFF, r @ ..] => (true, r),
                r => (enc == 2, r),
            };
            let units: Vec<u16> = rest
                .chunks_exact(2)
                .map(|p| if be { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) })
                .collect();
            char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')).collect::<String>()
        }
        3 => String::from_utf8_lossy(b).to_string(),
        _ => latin1(b),
    };
    s.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string()
}

fn id3_key(id: &str) -> Option<&'static str> {
    match id {
        "TIT2" | "TT2" => Some("title"),
        "TPE1" | "TP1" => Some("artist"),
        "TALB" | "TAL" => Some("album"),
        "TYER" | "TYE" | "TDRC" => Some("year"),
        "TCON" | "TCO" => Some("genre"),
        "TRCK" | "TRK" => Some("track"),
        _ => None,
    }
}

/// Frames de texto do ID3v2 (2.2, 2.3 e 2.4). `body` é o corpo da tag, sem os 10 bytes.
fn parse_id3v2(major: u8, body: &[u8]) -> Vec<TextTag> {
    let mut tags = Vec::new();
    let hdr = if major == 2 { 6 } else { 10 };
    let mut pos = 0usize;
    while pos + hdr <= body.len() && tags.len() < 64 {
        let id_len = if major == 2 { 3 } else { 4 };
        let id = &body[pos..pos + id_len];
        if id[0] == 0 {
            break; // padding
        }
        let size = match major {
            2 => ((body[pos + 3] as usize) << 16) | ((body[pos + 4] as usize) << 8) | body[pos + 5] as usize,
            4 => synchsafe(&body[pos + 4..]).unwrap_or(0),
            _ => u32::from_be_bytes([body[pos + 4], body[pos + 5], body[pos + 6], body[pos + 7]]) as usize,
        };
        let start = pos + hdr;
        let end = start.saturating_add(size).min(body.len());
        if let Some(key) = id3_key(&latin1(id)) {
            if end > start {
                let value = decode_id3_text(body[start], &body[start + 1..end]);
                if !value.is_empty() {
                    tags.push(TextTag { key: key.to_string(), value });
                }
            }
        }
        pos = start.saturating_add(size);
    }
    tags
}

fn parse_id3v1(b: &[u8]) -> Vec<TextTag> {
    let mut tags = Vec::new();
    let mut put = |key: &str, raw: &[u8]| {
        let v = cstr_latin1(raw);
        if !v.is_empty() {
            tags.push(TextTag { key: key.to_string(), value: v });
        }
    };
    put("title", &b[3..33]);
    put("artist", &b[33..63]);
    put("album", &b[63..93]);
    put("year", &b[93..97]);
    if b[125] == 0 && b[126] != 0 {
        tags.push(TextTag { key: "track".into(), value: b[126].to_string() });
    }
    tags
}

/// Lê tags, o primeiro frame e (se houver) o cabeçalho Xing/Info/VBRI. Duração: com
/// contagem de frames do Xing/VBRI ela é exata; senão é o tamanho do áudio dividido
/// pelo bitrate do primeiro frame (aproximada — VBR sem Xing erra).
pub fn parse_mp3(bytes: &[u8]) -> Result<AudioInfo, String> {
    let mut info = AudioInfo { container: "mp3".into(), size_bytes: bytes.len() as u64, ..Default::default() };
    let mut audio_start = 0usize;
    if bytes.len() >= 10 && &bytes[0..3] == b"ID3" {
        let size = synchsafe(&bytes[6..10]).ok_or_else(|| "ID3v2 com tamanho inválido".to_string())?;
        let footer = if bytes[5] & 0x10 != 0 { 10 } else { 0 };
        let body_end = 10usize.saturating_add(size).min(bytes.len());
        info.tags.extend(parse_id3v2(bytes[3], &bytes[10..body_end]));
        audio_start = (10usize.saturating_add(size).saturating_add(footer)).min(bytes.len());
    }
    let mut audio_end = bytes.len();
    if bytes.len() >= audio_start + 128 && &bytes[bytes.len() - 128..bytes.len() - 125] == b"TAG" {
        let v1 = parse_id3v1(&bytes[bytes.len() - 128..]);
        for t in v1 {
            if !info.tags.iter().any(|x| x.key == t.key) {
                info.tags.push(t);
            }
        }
        audio_end = bytes.len() - 128;
    }

    // primeiro frame válido: confirma olhando se o seguinte também começa com sync
    let limit = audio_end.min(audio_start.saturating_add(256 * 1024));
    let mut found: Option<(usize, Mp3Frame)> = None;
    let mut p = audio_start;
    while p + 4 <= limit {
        if bytes[p] == 0xFF {
            if let Some(fr) = parse_frame_header(&bytes[p..]) {
                let next = p + fr.frame_len;
                let ok = next + 4 > audio_end || parse_frame_header(&bytes[next..]).is_some();
                if ok {
                    found = Some((p, fr));
                    break;
                }
            }
        }
        p += 1;
    }
    let (fpos, fr) = found.ok_or_else(|| "nenhum frame MPEG válido encontrado".to_string())?;
    if fpos > audio_start {
        warn(&mut info, format!("{} bytes de lixo antes do primeiro frame", fpos - audio_start));
    }
    info.codec = "mp3".into();
    info.format_tag = Some(0x0055);
    info.channels = fr.channels;
    info.sample_rate = fr.sample_rate;
    info.bitrate_kbps = Some(fr.bitrate_kbps);
    info.playable = fr.layer == 3;
    if fr.layer != 3 {
        info.codec = format!("mpeg-layer{}", fr.layer);
        warn(&mut info, format!("MPEG layer {}: só o layer 3 (MP3) é esperado no jogo", fr.layer));
    }

    // Xing/Info/VBRI: contagem exata de frames
    let side = match (fr.version, fr.channels) {
        (1, 1) => 17,
        (1, _) => 32,
        (_, 1) => 9,
        _ => 17,
    };
    let frame_bytes = &bytes[fpos..(fpos + fr.frame_len).min(audio_end)];
    let mut vbr_frames: Option<u32> = None;
    let xing = 4 + side;
    if matches!(frame_bytes.get(xing..xing + 4), Some(b"Xing" | b"Info")) {
        let flags = frame_bytes.get(xing + 4..xing + 8).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
        if flags.is_some_and(|f| f & 1 != 0) {
            vbr_frames = frame_bytes
                .get(xing + 8..xing + 12)
                .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
        }
    } else if frame_bytes.get(36..40) == Some(b"VBRI") {
        vbr_frames = frame_bytes.get(50..54).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    }
    match vbr_frames.filter(|&n| n > 0) {
        Some(n) => {
            let total = n as u64 * fr.samples_per_frame as u64;
            info.sample_frames = Some(total);
            info.duration_secs = total as f64 / fr.sample_rate as f64;
        }
        None => {
            let audio_bytes = audio_end.saturating_sub(fpos) as f64;
            info.duration_secs = audio_bytes * 8.0 / (fr.bitrate_kbps as f64 * 1000.0);
            info.duration_approx = true;
        }
    }
    Ok(info)
}

// ---------------------------------------------------------------------------------
// Entrada geral
// ---------------------------------------------------------------------------------

fn looks_like_wav(b: &[u8]) -> bool {
    b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WAVE"
}

fn looks_like_mp3(b: &[u8]) -> bool {
    b.len() >= 4 && (&b[0..3] == b"ID3" || (b[0] == 0xFF && b[1] & 0xE0 == 0xE0))
}

/// Detecta pelo conteúdo (não pela extensão) e analisa WAV ou MP3.
pub fn analyze_audio_bytes(bytes: &[u8], buckets: usize) -> Result<AudioInfo, String> {
    if looks_like_wav(bytes) {
        parse_wav(bytes, buckets)
    } else if looks_like_mp3(bytes) {
        parse_mp3(bytes)
    } else {
        Err("formato de áudio não reconhecido (esperado WAV ou MP3)".to_string())
    }
}

fn read_limited(path: &std::path::Path, max: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("não leu {}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} não é um arquivo", path.display()));
    }
    if meta.len() > max {
        return Err(format!(
            "áudio de {:.1} MB passa do limite de {:.0} MB",
            meta.len() as f64 / 1_048_576.0,
            max as f64 / 1_048_576.0
        ));
    }
    std::fs::read(path).map_err(|e| format!("não leu {}: {e}", path.display()))
}

/// Analisa um arquivo de áudio do disco (até [`MAX_AUDIO_BYTES`]).
pub fn analyze_audio_file(path: &std::path::Path, buckets: usize) -> Result<AudioInfo, String> {
    analyze_audio_bytes(&read_limited(path, MAX_AUDIO_BYTES)?, buckets)
}

/// Monta o `data:` URL (`audio/wav` ou `audio/mpeg`, decidido pelo conteúdo).
pub fn audio_data_url(bytes: &[u8]) -> Result<AudioData, String> {
    let mime = if looks_like_wav(bytes) {
        "audio/wav"
    } else if looks_like_mp3(bytes) {
        "audio/mpeg"
    } else {
        return Err("formato de áudio não reconhecido (esperado WAV ou MP3)".to_string());
    };
    if bytes.len() as u64 > MAX_AUDIO_BYTES {
        return Err(format!("áudio de {} bytes passa do limite de {MAX_AUDIO_BYTES}", bytes.len()));
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(AudioData { mime: mime.to_string(), size_bytes: bytes.len() as u64, data_url: format!("data:{mime};base64,{b64}") })
}

/// Lê o arquivo (até [`MAX_AUDIO_BYTES`]) e devolve o `data:` URL.
pub fn read_audio_data_url(path: &std::path::Path) -> Result<AudioData, String> {
    audio_data_url(&read_limited(path, MAX_AUDIO_BYTES)?)
}

#[cfg(test)]
pub(crate) mod testutil {
    pub fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.extend_from_slice(body);
        if body.len() % 2 == 1 {
            v.push(0);
        }
        v
    }

    pub fn fmt_body(tag: u16, ch: u16, rate: u32, bits: u16) -> Vec<u8> {
        let ba = ch * bits.div_ceil(8);
        let mut v = Vec::new();
        v.extend_from_slice(&tag.to_le_bytes());
        v.extend_from_slice(&ch.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        v.extend_from_slice(&(rate * ba as u32).to_le_bytes());
        v.extend_from_slice(&ba.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());
        v
    }

    pub fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = chunks.concat();
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(&body);
        v
    }

    /// WAV PCM 16 bits com as amostras dadas (intercaladas se estéreo).
    pub fn wav16(ch: u16, rate: u32, samples: &[i16], extra: &[Vec<u8>]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut chunks = vec![chunk(b"fmt ", &fmt_body(1, ch, rate, 16))];
        chunks.extend(extra.iter().cloned());
        chunks.push(chunk(b"data", &data));
        riff(&chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn cue_chunk(offsets: &[u32]) -> Vec<u8> {
        let mut b = (offsets.len() as u32).to_le_bytes().to_vec();
        for (i, o) in offsets.iter().enumerate() {
            b.extend_from_slice(&(i as u32 + 1).to_le_bytes()); // id
            b.extend_from_slice(&0u32.to_le_bytes()); // position
            b.extend_from_slice(b"data");
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&o.to_le_bytes());
        }
        chunk(b"cue ", &b)
    }

    fn list_adtl_mark(len: u32) -> Vec<u8> {
        let mut ltxt = Vec::new();
        ltxt.extend_from_slice(&1u32.to_le_bytes()); // name
        ltxt.extend_from_slice(&len.to_le_bytes()); // sample length
        ltxt.extend_from_slice(b"mark");
        ltxt.extend_from_slice(&[0u8; 8]); // country, language, dialect, codepage
        let mut body = b"adtl".to_vec();
        body.extend_from_slice(&chunk(b"ltxt", &ltxt));
        chunk(b"LIST", &body)
    }

    #[test]
    fn wav_mono_16_metadados_e_envelope() {
        // 1000 amostras: metade positiva a 16384, metade negativa a -8192
        let mut s = vec![16384i16; 500];
        s.extend(vec![-8192i16; 500]);
        let w = wav16(1, 11025, &s, &[]);
        let i = parse_wav(&w, 4).unwrap();
        assert_eq!((i.container.as_str(), i.codec.as_str()), ("wav", "pcm"));
        assert_eq!((i.channels, i.sample_rate, i.bits_per_sample), (1, 11025, 16));
        assert_eq!(i.sample_frames, Some(1000));
        assert!((i.duration_secs - 1000.0 / 11025.0).abs() < 1e-9);
        assert!(i.playable && !i.duration_approx);
        let env = i.envelope.unwrap();
        assert_eq!(env.len(), 4);
        assert_eq!(env[0], [0.5, 0.5]);
        assert_eq!(env[3], [-0.25, -0.25]);
        assert_eq!(i.peak, Some(0.5));
    }

    #[test]
    fn wav_8_bits_sem_sinal() {
        let data = [128u8, 255, 0, 128];
        let w = riff(&[chunk(b"fmt ", &fmt_body(1, 1, 8000, 8)), chunk(b"data", &data)]);
        let i = parse_wav(&w, 4).unwrap();
        let env = i.envelope.unwrap();
        assert_eq!(env[0], [0.0, 0.0]);
        assert_eq!(env[1], [0.992, 0.992]);
        assert_eq!(env[2], [-1.0, -1.0]);
    }

    #[test]
    fn wav_estereo_mistura_os_canais_no_envelope() {
        // L=+0.5, R=-0.5 em todos os frames
        let s: Vec<i16> = (0..200).flat_map(|_| [16384i16, -16384]).collect();
        let i = parse_wav(&wav16(2, 22050, &s, &[]), 2).unwrap();
        assert_eq!(i.channels, 2);
        assert_eq!(i.sample_frames, Some(200));
        assert_eq!(i.envelope.unwrap()[0], [-0.5, 0.5]);
    }

    #[test]
    fn envelope_nao_passa_do_numero_de_frames_nem_do_teto() {
        let i = parse_wav(&wav16(1, 8000, &[1, 2, 3], &[]), 512).unwrap();
        assert_eq!(i.envelope.unwrap().len(), 3);
        let big: Vec<i16> = (0..20_000).map(|n| (n % 100) as i16).collect();
        let i = parse_wav(&wav16(1, 8000, &big, &[]), 1_000_000).unwrap();
        assert_eq!(i.envelope.unwrap().len(), MAX_ENVELOPE);
        let i = parse_wav(&wav16(1, 8000, &big, &[]), 0).unwrap();
        assert!(i.envelope.is_none());
    }

    #[test]
    fn loop_do_goldsrc_cue_mais_ltxt_mark() {
        let w = wav16(1, 11025, &vec![0i16; 2000], &[cue_chunk(&[100, 900]), list_adtl_mark(500)]);
        let i = parse_wav(&w, 8).unwrap();
        assert_eq!(i.cue_points.len(), 2);
        assert_eq!(i.cue_points[0], CuePoint { id: 1, sample_offset: 100 });
        assert_eq!(i.loops.len(), 1);
        assert_eq!(i.loops[0], LoopPoint { start_sample: 100, end_sample: Some(600), source: "cue".into() });
    }

    #[test]
    fn cue_sem_ltxt_loopa_ate_o_fim() {
        let w = wav16(1, 11025, &vec![0i16; 2000], &[cue_chunk(&[42])]);
        let i = parse_wav(&w, 8).unwrap();
        assert_eq!(i.loops[0].end_sample, None);
        assert_eq!(i.loops[0].start_sample, 42);
    }

    #[test]
    fn loop_alem_do_fim_gera_aviso() {
        let w = wav16(1, 11025, &[0i16; 100], &[cue_chunk(&[5000])]);
        let i = parse_wav(&w, 8).unwrap();
        assert!(i.warnings.iter().any(|w| w.contains("loop")));
    }

    #[test]
    fn smpl_loops() {
        let mut b = vec![0u8; 36];
        b[28..32].copy_from_slice(&1u32.to_le_bytes());
        for v in [7u32, 0, 10, 90, 0, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let w = wav16(1, 11025, &vec![0i16; 200], &[chunk(b"smpl", &b)]);
        let i = parse_wav(&w, 8).unwrap();
        assert_eq!(i.loops, vec![LoopPoint { start_sample: 10, end_sample: Some(90), source: "smpl".into() }]);
    }

    #[test]
    fn list_info_vira_tags() {
        let mut body = b"INFO".to_vec();
        body.extend_from_slice(&chunk(b"INAM", b"Vento\0"));
        body.extend_from_slice(&chunk(b"ISFT", b"Cool Edit"));
        let w = wav16(1, 8000, &[0; 10], &[chunk(b"LIST", &body)]);
        let i = parse_wav(&w, 4).unwrap();
        assert_eq!(i.tags[0], TextTag { key: "INAM".into(), value: "Vento".into() });
        assert_eq!(i.tags[1].value, "Cool Edit");
    }

    #[test]
    fn adpcm_so_e_identificado() {
        let mut fmt = fmt_body(2, 1, 22050, 4);
        fmt[8..12].copy_from_slice(&11000u32.to_le_bytes()); // byte rate
        let w = riff(&[chunk(b"fmt ", &fmt), chunk(b"fact", &44100u32.to_le_bytes()), chunk(b"data", &[0u8; 64])]);
        let i = parse_wav(&w, 16).unwrap();
        assert_eq!(i.codec, "ms-adpcm");
        assert_eq!(i.format_tag, Some(2));
        assert!(i.envelope.is_none() && !i.playable);
        assert_eq!(i.sample_frames, Some(44100));
        assert!((i.duration_secs - 2.0).abs() < 1e-9);
        assert!(i.warnings.iter().any(|w| w.contains("PCM")));
        // sem fact: estima pelo byte-rate
        let w = riff(&[chunk(b"fmt ", &fmt), chunk(b"data", &[0u8; 22000])]);
        let i = parse_wav(&w, 16).unwrap();
        assert!(i.duration_approx);
        assert!((i.duration_secs - 2.0).abs() < 1e-9);
    }

    #[test]
    fn extensible_resolve_o_formato_real() {
        let mut fmt = fmt_body(0xFFFE, 2, 44100, 16);
        fmt.extend_from_slice(&22u16.to_le_bytes()); // cbSize
        fmt.extend_from_slice(&16u16.to_le_bytes()); // valid bits
        fmt.extend_from_slice(&3u32.to_le_bytes()); // mask
        fmt.extend_from_slice(&1u16.to_le_bytes()); // subformat PCM
        fmt.extend_from_slice(&[0u8; 14]);
        let w = riff(&[chunk(b"fmt ", &fmt), chunk(b"data", &[0u8; 40])]);
        let i = parse_wav(&w, 4).unwrap();
        assert_eq!(i.codec, "pcm");
        assert_eq!(i.sample_frames, Some(10));
    }

    #[test]
    fn pcm_24_e_float() {
        let d24 = [0x00u8, 0x00, 0x40, 0x00, 0x00, 0xC0]; // +0.5, -0.5
        let w = riff(&[chunk(b"fmt ", &fmt_body(1, 1, 8000, 24)), chunk(b"data", &d24)]);
        let e = parse_wav(&w, 2).unwrap().envelope.unwrap();
        assert_eq!((e[0], e[1]), ([0.5, 0.5], [-0.5, -0.5]));
        let df: Vec<u8> = [0.25f32, f32::NAN, 7.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let w = riff(&[chunk(b"fmt ", &fmt_body(3, 1, 8000, 32)), chunk(b"data", &df)]);
        let e = parse_wav(&w, 3).unwrap().envelope.unwrap();
        assert_eq!(e, vec![[0.25, 0.25], [0.0, 0.0], [1.0, 1.0]]);
    }

    #[test]
    fn data_maior_que_o_arquivo_e_aparado() {
        let mut w = wav16(1, 8000, &[100; 50], &[]);
        let at = w.windows(4).position(|x| x == b"data").unwrap() + 4;
        w[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let i = parse_wav(&w, 4).unwrap();
        assert_eq!(i.sample_frames, Some(50));
        assert!(i.warnings.iter().any(|w| w.contains("data declara")));
    }

    #[test]
    fn erros_claros() {
        assert!(parse_wav(b"", 4).is_err());
        assert!(parse_wav(b"RIFF\0\0\0\0WAVX", 4).is_err());
        assert!(parse_wav(&riff(&[chunk(b"data", &[0; 4])]), 4).unwrap_err().contains("fmt"));
        assert!(parse_wav(&riff(&[chunk(b"fmt ", &fmt_body(1, 0, 8000, 16))]), 4).is_err());
        assert!(parse_wav(&riff(&[chunk(b"fmt ", &[0u8; 8])]), 4).is_err());
    }

    #[test]
    fn wav_truncado_em_todo_tamanho_e_lixo_nao_da_panic() {
        let w = wav16(2, 11025, &vec![1000i16; 300], &[cue_chunk(&[10]), list_adtl_mark(5)]);
        for cut in 0..w.len() {
            let _ = parse_wav(&w[..cut], 16);
            let _ = analyze_audio_bytes(&w[..cut], 16);
        }
        let mut seed = 99u32;
        for round in 0..400 {
            let mut buf: Vec<u8> = (0..(round % 300))
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 24) as u8
                })
                .collect();
            if round % 3 == 0 && buf.len() > 12 {
                buf[0..4].copy_from_slice(b"RIFF");
                buf[8..12].copy_from_slice(b"WAVE");
            }
            if round % 3 == 1 && buf.len() > 4 {
                buf[0] = 0xFF;
                buf[1] = 0xFB;
            }
            let _ = parse_wav(&buf, 32);
            let _ = parse_mp3(&buf);
            let _ = analyze_audio_bytes(&buf, 32);
        }
        // corrompe um byte por vez de um WAV válido
        for at in 0..w.len().min(120) {
            let mut c = w.clone();
            c[at] ^= 0xFF;
            let _ = parse_wav(&c, 16);
        }
    }

    #[test]
    fn chunks_com_tamanho_absurdo_nao_estouram() {
        let mut w = b"RIFF\0\0\0\0WAVE".to_vec();
        w.extend_from_slice(b"cue ");
        w.extend_from_slice(&u32::MAX.to_le_bytes());
        w.extend_from_slice(&u32::MAX.to_le_bytes()); // contagem gigante, corpo curto
        let _ = parse_wav(&w, 4);
        let mut w = riff(&[chunk(b"fmt ", &fmt_body(1, 1, 8000, 16))]);
        w.extend_from_slice(b"LIST");
        w.extend_from_slice(&u32::MAX.to_le_bytes());
        w.extend_from_slice(b"adtl");
        let _ = parse_wav(&w, 4);
    }

    // ---- MP3 ----

    fn mp3_frames(n: usize) -> Vec<u8> {
        let mut v = Vec::new();
        for _ in 0..n {
            let mut f = vec![0u8; 417];
            f[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            v.extend_from_slice(&f);
        }
        v
    }

    fn id3v2(frames: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, data) in frames {
            body.extend_from_slice(id.as_bytes());
            body.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(&[0, 0]);
            body.extend_from_slice(data);
        }
        let n = body.len();
        let mut v = b"ID3\x03\x00\x00".to_vec();
        v.extend_from_slice(&[(n >> 21) as u8 & 0x7F, (n >> 14) as u8 & 0x7F, (n >> 7) as u8 & 0x7F, n as u8 & 0x7F]);
        v.extend_from_slice(&body);
        v
    }

    #[test]
    fn mp3_cbr_cabecalho_e_duracao_aproximada() {
        let b = mp3_frames(100);
        let i = parse_mp3(&b).unwrap();
        assert_eq!((i.container.as_str(), i.codec.as_str()), ("mp3", "mp3"));
        assert_eq!((i.channels, i.sample_rate, i.bitrate_kbps), (2, 44100, Some(128)));
        assert!(i.duration_approx && i.playable);
        assert!((i.duration_secs - 41700.0 * 8.0 / 128000.0).abs() < 1e-9);
    }

    #[test]
    fn mp3_com_id3v2_e_id3v1() {
        let mut b = id3v2(&[("TIT2", [&[0u8][..], b"Titulo"].concat()), ("TPE1", [&[3u8][..], "Ação".as_bytes()].concat())]);
        let hdr = b.len();
        b.extend_from_slice(&mp3_frames(10));
        let mut v1 = vec![0u8; 128];
        v1[..3].copy_from_slice(b"TAG");
        v1[3..8].copy_from_slice(b"Outro");
        v1[63..68].copy_from_slice(b"Album");
        b.extend_from_slice(&v1);
        let i = parse_mp3(&b).unwrap();
        let get = |k: &str| i.tags.iter().find(|t| t.key == k).map(|t| t.value.clone());
        assert_eq!(get("title").as_deref(), Some("Titulo")); // v2 vence o v1
        assert_eq!(get("artist").as_deref(), Some("Ação"));
        assert_eq!(get("album").as_deref(), Some("Album"));
        assert!((i.duration_secs - 4170.0 * 8.0 / 128000.0).abs() < 1e-9, "{hdr}");
    }

    #[test]
    fn mp3_com_xing_tem_duracao_exata() {
        let mut b = mp3_frames(5);
        let at = 4 + 32; // MPEG-1 estéreo
        b[at..at + 4].copy_from_slice(b"Xing");
        b[at + 4..at + 8].copy_from_slice(&1u32.to_be_bytes());
        b[at + 8..at + 12].copy_from_slice(&1000u32.to_be_bytes());
        let i = parse_mp3(&b).unwrap();
        assert!(!i.duration_approx);
        assert_eq!(i.sample_frames, Some(1_152_000));
        assert!((i.duration_secs - 1_152_000.0 / 44100.0).abs() < 1e-9);
    }

    #[test]
    fn mp3_invalido() {
        assert!(parse_mp3(b"").is_err());
        assert!(parse_mp3(&[0u8; 1000]).is_err());
        assert!(parse_mp3(b"ID3\x03\x00\x00\xFF\xFF\xFF\xFF").is_err());
        // lixo antes do primeiro frame é tolerado com aviso
        let mut b = vec![0u8; 50];
        b.extend_from_slice(&mp3_frames(3));
        let i = parse_mp3(&b).unwrap();
        assert!(i.warnings.iter().any(|w| w.contains("lixo")));
    }

    #[test]
    fn data_url_por_conteudo() {
        let w = wav16(1, 8000, &[1, 2, 3], &[]);
        let d = audio_data_url(&w).unwrap();
        assert_eq!(d.mime, "audio/wav");
        assert!(d.data_url.starts_with("data:audio/wav;base64,UklGR"));
        assert_eq!(audio_data_url(&mp3_frames(2)).unwrap().mime, "audio/mpeg");
        assert!(audio_data_url(b"MZ\x90\0 exe disfarcado").is_err());
        assert!(audio_data_url(b"").is_err());
    }

    #[test]
    fn arquivo_em_disco_respeita_o_limite() {
        let dir = std::env::temp_dir().join(format!("bsp-museum-audio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.wav");
        std::fs::write(&p, wav16(1, 8000, &[5; 100], &[])).unwrap();
        assert_eq!(analyze_audio_file(&p, 8).unwrap().sample_frames, Some(100));
        assert!(read_audio_data_url(&p).is_ok());
        assert!(read_limited(&p, 10).unwrap_err().contains("limite"));
        assert!(analyze_audio_file(&dir, 8).is_err());
        assert!(analyze_audio_file(&dir.join("nao.wav"), 8).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
