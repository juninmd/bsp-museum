//! `.dem` do GoldSrc (`HLDEMO`): só metadados e duração — os frames de rede não são
//! decodificados.
//!
//! Layout conferido contra a especificação pública `hl1-demofile-spec.txt`
//! (cgit.sukimashita.com/hl-demofile-spec.git); campos little-endian:
//!
//! ```text
//! cabeçalho (544 bytes):
//!   magic[8] = "HLDEMO\0\0" | demo_protocol u32 (5) | network_protocol u32
//!   map_name[260] | game_dir[260] ("game_dll" na spec) | map_crc u32 | directory_offset u32
//! diretório (em directory_offset):
//!   count u32 (1..=1024), depois count x entrada de 92 bytes:
//!     tipo u32 ("number": 0 = LOADING, 1 = Playback) | título[64]
//!     flags u32 | cd_track i32 ("play") | tempo f32 ("time", segundos) | frames u32
//!     offset u32 | length u32
//! ```
//!
//! O diretório fica no fim do arquivo; offset errado é a causa clássica de demo
//! "corrompida" (a própria spec avisa). Aqui isso vira aviso, não erro: o cabeçalho
//! continua útil.
//!
//! Não conferido contra arquivo real: os testes usam demos montadas byte a byte. A
//! tabela `network_protocol -> versão do HL` é a da spec (40..47), e o `48` do
//! CS 1.6/Steam vem de memória.

use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const DEMO_MAGIC: &[u8; 6] = b"HLDEMO";
pub const HEADER_LEN: usize = 544;
const ENTRY_LEN: usize = 92;
const MAX_ENTRIES: u32 = 1024;
const MAX_WARNINGS: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DemoEntry {
    pub index: u32,
    /// valor bruto do campo de tipo
    pub kind: u32,
    /// `"loading"` (0), `"playback"` (1) ou `"desconhecido"`
    pub kind_name: String,
    pub title: String,
    pub flags: u32,
    pub cd_track: i32,
    pub duration_secs: f32,
    pub frames: u32,
    pub offset: u32,
    pub length: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DemoInfo {
    pub demo_protocol: u32,
    pub network_protocol: u32,
    /// versão do HL/CS que a spec associa ao protocolo de rede, quando conhecida
    pub network_version_hint: Option<String>,
    pub map_name: String,
    pub game_dir: String,
    pub map_crc: u32,
    pub directory_offset: u32,
    pub entries: Vec<DemoEntry>,
    /// soma dos segmentos que não são `loading` (a gravação em si)
    pub duration_secs: f64,
    pub frames: u64,
    pub file_size: u64,
    pub warnings: Vec<String>,
}

fn warn(info: &mut DemoInfo, msg: String) {
    if info.warnings.len() < MAX_WARNINGS {
        info.warnings.push(msg);
    }
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

fn network_hint(n: u32) -> Option<String> {
    let s = match n {
        40 => "Half-Life 1.0.1.6",
        42 => "Half-Life 1.1.0.1",
        43 => "Half-Life 1.1.0.4/6",
        45 => "Half-Life 1.1.0.8",
        46 | 47 => "Half-Life 1.1.1.x",
        48 => "Half-Life 1.1.2.x / CS 1.6 (Steam)",
        _ => return None,
    };
    Some(s.to_string())
}

/// Lê só o cabeçalho (os primeiros [`HEADER_LEN`] bytes).
fn parse_header(head: &[u8], file_size: u64) -> Result<DemoInfo, String> {
    if head.len() < HEADER_LEN {
        return Err(format!("demo pequena demais: {} bytes (cabeçalho tem {HEADER_LEN})", head.len()));
    }
    if &head[0..6] != DEMO_MAGIC {
        return Err("não é uma demo do GoldSrc: assinatura \"HLDEMO\" ausente".to_string());
    }
    let network = u32_at(head, 12).unwrap_or(0);
    Ok(DemoInfo {
        demo_protocol: u32_at(head, 8).unwrap_or(0),
        network_protocol: network,
        network_version_hint: network_hint(network),
        map_name: cstr(&head[16..276]),
        game_dir: cstr(&head[276..536]),
        map_crc: u32_at(head, 536).unwrap_or(0),
        directory_offset: u32_at(head, 540).unwrap_or(0),
        file_size,
        ..Default::default()
    })
}

/// Interpreta o diretório (`count` + entradas). `dir` começa em `directory_offset`.
fn parse_directory(info: &mut DemoInfo, dir: &[u8]) {
    let Some(count) = u32_at(dir, 0) else {
        warn(info, "diretório da demo truncado (sem contagem)".to_string());
        return;
    };
    if count == 0 || count > MAX_ENTRIES {
        warn(info, format!("diretório com {count} entradas (esperado 1 a {MAX_ENTRIES}); offset provavelmente errado"));
        return;
    }
    for i in 0..count as usize {
        let at = 4 + i * ENTRY_LEN;
        let Some(rec) = dir.get(at..at + ENTRY_LEN) else {
            warn(info, format!("diretório truncado na entrada {i} de {count}"));
            break;
        };
        let kind = u32_at(rec, 0).unwrap_or(0);
        let secs = f32::from_bits(u32_at(rec, 76).unwrap_or(0));
        let secs = if secs.is_finite() && secs >= 0.0 {
            secs
        } else {
            warn(info, format!("entrada {i} com duração inválida ({secs}); contada como 0"));
            0.0
        };
        let e = DemoEntry {
            index: i as u32,
            kind,
            kind_name: match kind {
                0 => "loading",
                1 => "playback",
                _ => "desconhecido",
            }
            .to_string(),
            title: cstr(&rec[4..68]),
            flags: u32_at(rec, 68).unwrap_or(0),
            cd_track: u32_at(rec, 72).unwrap_or(0) as i32,
            duration_secs: secs,
            frames: u32_at(rec, 80).unwrap_or(0),
            offset: u32_at(rec, 84).unwrap_or(0),
            length: u32_at(rec, 88).unwrap_or(0),
        };
        if e.offset as u64 + e.length as u64 > info.file_size {
            warn(info, format!("entrada {i} (\"{}\") aponta para fora do arquivo", e.title));
        }
        if e.kind != 0 {
            info.duration_secs += e.duration_secs as f64;
            info.frames += e.frames as u64;
        }
        info.entries.push(e);
    }
}

/// Lê metadados de uma demo já em memória.
pub fn parse_demo(bytes: &[u8]) -> Result<DemoInfo, String> {
    let mut info = parse_header(bytes, bytes.len() as u64)?;
    check_protocol(&mut info);
    let off = info.directory_offset as usize;
    if off < HEADER_LEN || off >= bytes.len() {
        warn(&mut info, format!("offset do diretório ({off}) fora do arquivo de {} bytes; demo possivelmente corrompida", bytes.len()));
    } else {
        parse_directory(&mut info, &bytes[off..]);
    }
    Ok(info)
}

/// Lê metadados direto do disco: cabeçalho + diretório por `seek` (demos passam de 100 MB).
pub fn parse_demo_file(path: &Path) -> Result<DemoInfo, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("não abriu {}: {e}", path.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let mut head = vec![0u8; HEADER_LEN.min(size as usize)];
    f.read_exact(&mut head).map_err(|e| e.to_string())?;
    let mut info = parse_header(&head, size)?;
    check_protocol(&mut info);
    let off = info.directory_offset as u64;
    if off < HEADER_LEN as u64 || off >= size {
        warn(&mut info, format!("offset do diretório ({off}) fora do arquivo de {size} bytes; demo possivelmente corrompida"));
        return Ok(info);
    }
    // 4 + 1024 entradas é o máximo; lê só o que cabe
    let want = ((4 + MAX_ENTRIES as usize * ENTRY_LEN) as u64).min(size - off) as usize;
    f.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
    let mut dir = vec![0u8; want];
    f.read_exact(&mut dir).map_err(|e| e.to_string())?;
    parse_directory(&mut info, &dir);
    Ok(info)
}

fn check_protocol(info: &mut DemoInfo) {
    if info.demo_protocol != 5 {
        warn(info, format!("demo_protocol {} (o GoldSrc grava 5); a leitura pode estar errada", info.demo_protocol));
    }
}

#[cfg(test)]
pub(crate) fn build_demo(map: &str, game: &str, entries: &[(u32, &str, f32, u32)]) -> Vec<u8> {
    let mut v = vec![0u8; HEADER_LEN];
    v[0..6].copy_from_slice(DEMO_MAGIC);
    v[8..12].copy_from_slice(&5u32.to_le_bytes());
    v[12..16].copy_from_slice(&48u32.to_le_bytes());
    v[16..16 + map.len()].copy_from_slice(map.as_bytes());
    v[276..276 + game.len()].copy_from_slice(game.as_bytes());
    v[536..540].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    // "dados" falsos entre o cabeçalho e o diretório
    v.extend_from_slice(&[0xAA; 100]);
    let dir_off = v.len() as u32;
    v[540..544].copy_from_slice(&dir_off.to_le_bytes());
    v.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (kind, title, secs, frames) in entries {
        let mut rec = [0u8; ENTRY_LEN];
        rec[0..4].copy_from_slice(&kind.to_le_bytes());
        rec[4..4 + title.len()].copy_from_slice(title.as_bytes());
        rec[72..76].copy_from_slice(&(-1i32).to_le_bytes());
        rec[76..80].copy_from_slice(&secs.to_le_bytes());
        rec[80..84].copy_from_slice(&frames.to_le_bytes());
        rec[84..88].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        rec[88..92].copy_from_slice(&100u32.to_le_bytes());
        v.extend_from_slice(&rec);
    }
    v.extend_from_slice(&dir_off.to_le_bytes()); // dir_end (sufixo da spec)
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        build_demo("de_dust2", "cstrike", &[(0, "LOADING", 4.5, 100), (1, "Playback", 125.25, 9000)])
    }

    #[test]
    fn cabecalho_e_diretorio() {
        let d = parse_demo(&sample()).unwrap();
        assert_eq!((d.demo_protocol, d.network_protocol), (5, 48));
        assert_eq!(d.map_name, "de_dust2");
        assert_eq!(d.game_dir, "cstrike");
        assert_eq!(d.map_crc, 0xDEAD_BEEF);
        assert_eq!(d.entries.len(), 2);
        assert_eq!(d.entries[0].kind_name, "loading");
        assert_eq!(d.entries[1].kind_name, "playback");
        assert_eq!(d.entries[1].title, "Playback");
        assert_eq!(d.entries[1].frames, 9000);
        assert_eq!(d.entries[1].cd_track, -1);
        // loading não conta na duração da gravação
        assert!((d.duration_secs - 125.25).abs() < 1e-6);
        assert_eq!(d.frames, 9000);
        assert!(d.network_version_hint.is_some());
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    }

    #[test]
    fn assinatura_e_tamanho_invalidos() {
        assert!(parse_demo(b"").is_err());
        assert!(parse_demo(&[0u8; 543]).is_err());
        assert!(parse_demo(&[0u8; 600]).unwrap_err().contains("HLDEMO"));
        let mut b = sample();
        b[0] = b'X';
        assert!(parse_demo(&b).is_err());
    }

    #[test]
    fn offset_do_diretorio_errado_vira_aviso() {
        let mut b = sample();
        b[540..544].copy_from_slice(&u32::MAX.to_le_bytes());
        let d = parse_demo(&b).unwrap();
        assert!(d.entries.is_empty());
        assert_eq!(d.map_name, "de_dust2");
        assert!(d.warnings.iter().any(|w| w.contains("offset do diretório")));
        b[540..544].copy_from_slice(&10u32.to_le_bytes());
        assert!(!parse_demo(&b).unwrap().warnings.is_empty());
    }

    #[test]
    fn contagem_absurda_e_entradas_truncadas() {
        let mut b = sample();
        let off = u32::from_le_bytes(b[540..544].try_into().unwrap()) as usize;
        b[off..off + 4].copy_from_slice(&5000u32.to_le_bytes());
        assert!(parse_demo(&b).unwrap().entries.is_empty());
        b[off..off + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_demo(&b).unwrap().entries.is_empty());
        b[off..off + 4].copy_from_slice(&1000u32.to_le_bytes());
        let d = parse_demo(&b).unwrap();
        assert_eq!(d.entries.len(), 2); // só as 2 que cabem
        assert!(d.warnings.iter().any(|w| w.contains("truncado")));
    }

    #[test]
    fn duracao_invalida_conta_zero() {
        let d = parse_demo(&build_demo("m", "cstrike", &[(1, "P", f32::NAN, 10), (1, "Q", -3.0, 5), (1, "R", 2.0, 1)])).unwrap();
        assert!((d.duration_secs - 2.0).abs() < 1e-9);
        assert!(d.warnings.iter().any(|w| w.contains("duração inválida")));
    }

    #[test]
    fn protocolo_estranho_avisa() {
        let mut b = sample();
        b[8..12].copy_from_slice(&3u32.to_le_bytes());
        assert!(parse_demo(&b).unwrap().warnings.iter().any(|w| w.contains("demo_protocol")));
    }

    #[test]
    fn lixo_e_truncamento_nao_dao_panic() {
        let full = sample();
        for cut in 0..full.len() {
            let _ = parse_demo(&full[..cut]);
        }
        for at in 0..full.len() {
            let mut c = full.clone();
            c[at] ^= 0xFF;
            let _ = parse_demo(&c);
        }
        let mut seed = 3u32;
        for round in 0..200 {
            let mut buf: Vec<u8> = (0..(500 + round * 3))
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 24) as u8
                })
                .collect();
            buf[0..6].copy_from_slice(DEMO_MAGIC);
            let _ = parse_demo(&buf);
        }
    }

    #[test]
    fn leitura_por_seek_bate_com_a_de_memoria() {
        let dir = std::env::temp_dir().join(format!("bsp-museum-demo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.dem");
        std::fs::write(&p, sample()).unwrap();
        let a = parse_demo_file(&p).unwrap();
        let b = parse_demo(&sample()).unwrap();
        assert_eq!(a.entries, b.entries);
        assert_eq!(a.map_name, b.map_name);
        assert_eq!(a.file_size, b.file_size);
        std::fs::write(&p, &sample()[..100]).unwrap();
        assert!(parse_demo_file(&p).is_err());
        std::fs::write(&p, b"").unwrap();
        assert!(parse_demo_file(&p).is_err());
        assert!(parse_demo_file(&dir.join("nao.dem")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
