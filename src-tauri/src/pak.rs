//! Leitor de arquivos `.pak` do Quake / Half-Life (formato `PACK`).
//!
//! Layout (conferido contra `common.c` do Quake, `dpackheader_t`/`dpackfile_t`):
//!
//! ```text
//! cabeçalho (12 bytes):  "PACK" | dirofs: i32 | dirlen: i32
//! diretório (em dirofs): dirlen/64 entradas de 64 bytes:
//!                        name[56] (NUL-terminado) | filepos: i32 | filelen: i32
//! ```
//!
//! O Quake limita a 2048 arquivos por pak (`MAX_FILES_IN_PACK`); aqui o teto é maior
//! (ver [`MAX_ENTRIES`]) só para não recusar paks de outros jogos. Tudo é lido como
//! arquivo hostil: offsets são checados contra o tamanho real e nada dá panic.
//! Somente leitura — este módulo nunca extrai para o disco.

use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const PAK_MAGIC: &[u8; 4] = b"PACK";
const HEADER_LEN: usize = 12;
const ENTRY_LEN: usize = 64;
const NAME_LEN: usize = 56;
/// Teto de entradas aceitas (o Quake usa 2048).
pub const MAX_ENTRIES: usize = 65_536;
/// Limite padrão ao extrair um membro para memória.
pub const DEFAULT_MAX_EXTRACT: usize = 64 * 1024 * 1024;
const MAX_WARNINGS: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PakEntry {
    /// caminho dentro do pak, com `/` e como está gravado (a busca ignora maiúsculas)
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PakListing {
    /// tamanho do arquivo `.pak` inteiro
    pub file_size: u64,
    pub entries: Vec<PakEntry>,
    /// entradas descartadas (nome inseguro, offset/tamanho fora do arquivo)
    pub skipped: u32,
    pub warnings: Vec<String>,
}

/// Bytes extraídos de um membro do pak.
#[derive(Debug, Clone, Serialize)]
pub struct PakData {
    pub name: String,
    /// tamanho total do membro (pode ser maior que `bytes.len()` se truncado)
    pub size: u64,
    pub truncated: bool,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

fn warn(w: &mut Vec<String>, msg: String) {
    if w.len() < MAX_WARNINGS {
        w.push(msg);
    } else if w.len() == MAX_WARNINGS {
        w.push("… mais avisos omitidos".to_string());
    }
}

fn le_i32(b: &[u8], at: usize) -> Option<i32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Normaliza o nome de um membro (`\` vira `/`, tira `./` inicial) e recusa o que
/// escaparia de uma raiz: vazio, absoluto, com `..`, com `:` ou NUL.
pub fn safe_entry_name(name: &str) -> Option<String> {
    let n = name.replace('\\', "/");
    let n = n.trim_start_matches("./");
    if n.is_empty() || n.starts_with('/') || n.contains('\0') || n.contains(':') {
        return None;
    }
    if n.split('/').any(|c| c == ".." || c.is_empty()) {
        return None;
    }
    Some(n.to_string())
}

/// Valida o cabeçalho de 12 bytes e devolve `(dirofs, dirlen)` já checados contra o
/// tamanho do arquivo.
fn parse_header(head: &[u8], file_size: u64) -> Result<(u64, u64), String> {
    if head.len() < HEADER_LEN {
        return Err(format!("pak pequeno demais: {} bytes (cabeçalho tem {HEADER_LEN})", head.len()));
    }
    if &head[0..4] != PAK_MAGIC {
        return Err("não é um .pak: assinatura \"PACK\" ausente".to_string());
    }
    let ofs = le_i32(head, 4).unwrap_or(-1);
    let len = le_i32(head, 8).unwrap_or(-1);
    if ofs < 0 || len < 0 {
        return Err(format!("diretório do pak com offset/tamanho negativo ({ofs}/{len})"));
    }
    let (ofs, len) = (ofs as u64, len as u64);
    if ofs < HEADER_LEN as u64 {
        return Err(format!("diretório do pak começa em {ofs}, dentro do cabeçalho"));
    }
    match ofs.checked_add(len) {
        Some(end) if end <= file_size => {}
        _ => return Err(format!("diretório do pak ({ofs}+{len}) passa do fim do arquivo ({file_size} bytes)")),
    }
    if len as usize / ENTRY_LEN > MAX_ENTRIES {
        return Err(format!("pak com {} entradas (máximo aceito: {MAX_ENTRIES})", len as usize / ENTRY_LEN));
    }
    Ok((ofs, len))
}

/// Interpreta os bytes do diretório. Entradas inválidas são puladas e contadas.
fn parse_directory(dir: &[u8], file_size: u64) -> PakListing {
    let mut out = PakListing { file_size, ..Default::default() };
    if dir.len() % ENTRY_LEN != 0 {
        warn(
            &mut out.warnings,
            format!("diretório com {} bytes, que não é múltiplo de {ENTRY_LEN}; sobra ignorada", dir.len()),
        );
    }
    for (i, rec) in dir.chunks_exact(ENTRY_LEN).enumerate() {
        let raw = &rec[..NAME_LEN];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
        let name = String::from_utf8_lossy(&raw[..end]).to_string();
        let pos = le_i32(rec, NAME_LEN).unwrap_or(-1);
        let len = le_i32(rec, NAME_LEN + 4).unwrap_or(-1);
        let Some(name) = safe_entry_name(&name) else {
            out.skipped += 1;
            warn(&mut out.warnings, format!("entrada {i}: nome inseguro ou vazio ({})", String::from_utf8_lossy(&raw[..end])));
            continue;
        };
        if pos < 0 || len < 0 || (pos as u64 + len as u64) > file_size {
            out.skipped += 1;
            warn(&mut out.warnings, format!("entrada {i} ({name}): {pos}+{len} fora do arquivo de {file_size} bytes"));
            continue;
        }
        out.entries.push(PakEntry { name, offset: pos as u32, size: len as u32 });
    }
    out
}

/// Lista os membros de um pak já carregado em memória.
pub fn list_pak_bytes(bytes: &[u8]) -> Result<PakListing, String> {
    let (ofs, len) = parse_header(bytes, bytes.len() as u64)?;
    let dir = bytes
        .get(ofs as usize..(ofs + len) as usize)
        .ok_or_else(|| "diretório do pak fora do arquivo".to_string())?;
    Ok(parse_directory(dir, bytes.len() as u64))
}

/// Lista os membros lendo só o cabeçalho e o diretório (paks passam de 100 MB).
pub fn list_pak_file(path: &Path) -> Result<PakListing, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("não abriu {}: {e}", path.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let mut head = [0u8; HEADER_LEN];
    f.read_exact(&mut head).map_err(|_| format!("pak pequeno demais: {size} bytes"))?;
    let (ofs, len) = parse_header(&head, size)?;
    f.seek(SeekFrom::Start(ofs)).map_err(|e| e.to_string())?;
    let mut dir = vec![0u8; len as usize];
    f.read_exact(&mut dir).map_err(|e| format!("diretório do pak truncado: {e}"))?;
    Ok(parse_directory(&dir, size))
}

/// Procura um membro ignorando maiúsculas e a diferença `/` x `\`. O primeiro vence
/// (mesma regra do Quake ao percorrer `pack->files`).
pub fn find_entry<'a>(listing: &'a PakListing, name: &str) -> Option<&'a PakEntry> {
    let want = safe_entry_name(name)?.to_ascii_lowercase();
    listing.entries.iter().find(|e| e.name.to_ascii_lowercase() == want)
}

/// Extrai um membro de um pak em memória, no máximo `max_bytes`.
pub fn read_pak_bytes_entry(pak: &[u8], name: &str, max_bytes: usize) -> Result<PakData, String> {
    let listing = list_pak_bytes(pak)?;
    let e = find_entry(&listing, name).ok_or_else(|| format!("\"{name}\" não está no pak"))?;
    let take = (e.size as usize).min(max_bytes);
    let start = e.offset as usize;
    let bytes = pak
        .get(start..start.checked_add(take).ok_or("offset do membro estoura")?)
        .ok_or_else(|| format!("membro \"{}\" fora do arquivo", e.name))?
        .to_vec();
    Ok(PakData { name: e.name.clone(), size: e.size as u64, truncated: take < e.size as usize, bytes })
}

/// Extrai um membro direto do arquivo, por `seek`, no máximo `max_bytes`.
pub fn read_pak_file_entry(path: &Path, name: &str, max_bytes: usize) -> Result<PakData, String> {
    let listing = list_pak_file(path)?;
    let e = find_entry(&listing, name).ok_or_else(|| format!("\"{name}\" não está em {}", path.display()))?;
    let take = (e.size as usize).min(max_bytes);
    let mut f = std::fs::File::open(path).map_err(|err| err.to_string())?;
    f.seek(SeekFrom::Start(e.offset as u64)).map_err(|err| err.to_string())?;
    let mut bytes = vec![0u8; take];
    f.read_exact(&mut bytes).map_err(|err| format!("membro \"{}\" truncado: {err}", e.name))?;
    Ok(PakData { name: e.name.clone(), size: e.size as u64, truncated: take < e.size as usize, bytes })
}

#[cfg(test)]
pub(crate) fn build_pak(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut dir = Vec::new();
    for (name, content) in files {
        let mut rec = [0u8; ENTRY_LEN];
        rec[..name.len()].copy_from_slice(name.as_bytes());
        rec[NAME_LEN..NAME_LEN + 4].copy_from_slice(&((HEADER_LEN + data.len()) as i32).to_le_bytes());
        rec[NAME_LEN + 4..].copy_from_slice(&(content.len() as i32).to_le_bytes());
        dir.extend_from_slice(&rec);
        data.extend_from_slice(content);
    }
    let mut out = Vec::new();
    out.extend_from_slice(PAK_MAGIC);
    out.extend_from_slice(&((HEADER_LEN + data.len()) as i32).to_le_bytes());
    out.extend_from_slice(&(dir.len() as i32).to_le_bytes());
    out.extend_from_slice(&data);
    out.extend_from_slice(&dir);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        build_pak(&[("sound/ambience/wind.wav", b"RIFFxxxx"), ("maps/de_x.txt", b"ola mundo"), ("vazio.bin", b"")])
    }

    #[test]
    fn lista_e_extrai() {
        let pak = sample();
        let l = list_pak_bytes(&pak).unwrap();
        assert_eq!(l.entries.len(), 3);
        assert_eq!(l.skipped, 0);
        assert_eq!(l.entries[0].name, "sound/ambience/wind.wav");
        assert_eq!(l.entries[1].size, 9);
        let d = read_pak_bytes_entry(&pak, "MAPS\\de_x.txt", 1024).unwrap();
        assert_eq!(d.bytes, b"ola mundo");
        assert!(!d.truncated);
        assert_eq!(read_pak_bytes_entry(&pak, "vazio.bin", 10).unwrap().bytes.len(), 0);
    }

    #[test]
    fn extracao_respeita_o_limite() {
        let d = read_pak_bytes_entry(&sample(), "maps/de_x.txt", 3).unwrap();
        assert_eq!(d.bytes, b"ola");
        assert_eq!(d.size, 9);
        assert!(d.truncated);
    }

    #[test]
    fn membro_ausente_e_nome_malicioso() {
        let pak = sample();
        assert!(read_pak_bytes_entry(&pak, "nao/existe", 10).is_err());
        assert!(read_pak_bytes_entry(&pak, "../maps/de_x.txt", 10).is_err());
        assert!(read_pak_bytes_entry(&pak, "/maps/de_x.txt", 10).is_err());
        assert!(read_pak_bytes_entry(&pak, "", 10).is_err());
    }

    #[test]
    fn cabecalho_invalido() {
        assert!(list_pak_bytes(b"").is_err());
        assert!(list_pak_bytes(b"PACK").is_err());
        assert!(list_pak_bytes(b"WAD3\0\0\0\0\0\0\0\0").is_err());
        let mut pak = sample();
        pak[4..8].copy_from_slice(&(-5i32).to_le_bytes());
        assert!(list_pak_bytes(&pak).unwrap_err().contains("negativo"));
        let mut pak = sample();
        pak[4..8].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(list_pak_bytes(&pak).unwrap_err().contains("passa do fim"));
        let mut pak = sample();
        pak[4..8].copy_from_slice(&4i32.to_le_bytes());
        assert!(list_pak_bytes(&pak).is_err());
        // dirlen gigante: não deve alocar nem estourar
        let mut pak = sample();
        pak[8..12].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(list_pak_bytes(&pak).is_err());
    }

    #[test]
    fn entradas_ruins_sao_puladas_sem_derrubar_o_resto() {
        let mut pak = build_pak(&[("ok.txt", b"abc"), ("../fuga.txt", b"x"), ("longe.txt", b"y"), ("/abs.txt", b"z")]);
        // aponta "longe.txt" para fora do arquivo
        let dir_ofs = i32::from_le_bytes(pak[4..8].try_into().unwrap()) as usize;
        let rec = dir_ofs + 2 * ENTRY_LEN + NAME_LEN;
        pak[rec..rec + 4].copy_from_slice(&1_000_000i32.to_le_bytes());
        let l = list_pak_bytes(&pak).unwrap();
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.entries[0].name, "ok.txt");
        assert_eq!(l.skipped, 3);
        assert!(!l.warnings.is_empty());
    }

    #[test]
    fn diretorio_nao_multiplo_de_64() {
        let mut pak = sample();
        let n = i32::from_le_bytes(pak[8..12].try_into().unwrap());
        pak.extend_from_slice(&[0u8; 10]);
        pak[8..12].copy_from_slice(&(n + 10).to_le_bytes());
        let l = list_pak_bytes(&pak).unwrap();
        assert_eq!(l.entries.len(), 3);
        assert!(l.warnings.iter().any(|w| w.contains("múltiplo")));
    }

    #[test]
    fn nome_sem_nul_usa_os_56_bytes() {
        let name = "a".repeat(56);
        let mut pak = build_pak(&[("x", b"1")]);
        let dir_ofs = i32::from_le_bytes(pak[4..8].try_into().unwrap()) as usize;
        pak[dir_ofs..dir_ofs + 56].copy_from_slice(name.as_bytes());
        let l = list_pak_bytes(&pak).unwrap();
        assert_eq!(l.entries[0].name, name);
    }

    #[test]
    fn lixo_aleatorio_nao_da_panic() {
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8
        };
        for round in 0..300 {
            let n = (round * 7) % 400;
            let mut buf: Vec<u8> = (0..n).map(|_| next()).collect();
            if round % 2 == 0 && buf.len() >= 12 {
                buf[0..4].copy_from_slice(PAK_MAGIC);
            }
            let _ = list_pak_bytes(&buf);
            let _ = read_pak_bytes_entry(&buf, "a.txt", 16);
        }
        // truncar um pak válido em cada tamanho possível
        let pak = sample();
        for cut in 0..pak.len() {
            let _ = list_pak_bytes(&pak[..cut]);
        }
    }

    #[test]
    fn arquivo_em_disco_por_seek() {
        let dir = std::env::temp_dir().join(format!("bsp-museum-pak-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("pak0.pak");
        std::fs::write(&p, sample()).unwrap();
        let l = list_pak_file(&p).unwrap();
        assert_eq!(l.entries.len(), 3);
        let d = read_pak_file_entry(&p, "Maps/DE_X.txt", 100).unwrap();
        assert_eq!(d.bytes, b"ola mundo");
        assert!(read_pak_file_entry(&p, "nada", 100).is_err());
        std::fs::write(&p, &sample()[..20]).unwrap();
        assert!(list_pak_file(&p).is_err());
        assert!(list_pak_file(&dir.join("nao-existe.pak")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
