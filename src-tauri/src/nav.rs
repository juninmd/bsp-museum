//! `.nav` do Counter-Strike 1.6 / Condition Zero (malha de navegação dos bots).
//!
//! Layout conferido contra o código aberto do ReGameDLL_CS
//! (`regamedll/game_shared/bot/nav_file.cpp`, `nav.h`, `nav_area.cpp`), todos os campos
//! little-endian:
//!
//! ```text
//! magic u32 = 0xFEEDFACE
//! version u32                (1..=5; o ReGameDLL recusa > 5)
//! [v>=4] bsp_size u32        (tamanho do .bsp usado na geração — detecta nav velho)
//! [v>=5] lugares: count u16, depois count x { len u16, nome[len] com NUL }
//! area_count u32
//! area_count x área:
//!   id u32 | atributos u8 (1 crouch, 2 jump, 4 precise, 8 no_jump)
//!   extent: lo.xyz f32 | hi.xyz f32 | ne_z f32 | sw_z f32
//!   4 x { n u32, n x id u32 }         (conexões N, E, S, W)
//!   hiding_count u8, depois:  v1: xyz f32 (12 B)   v>=2: id u32 + xyz f32 + flags u8 (17 B)
//!   approach_count u8, depois x { here u32, prev u32, prev_how u8, next u32, next_how u8 }
//!   encounter_count u32, depois:
//!     v<3: x { from u32, to u32, 6 f32, n u8, n x (xyz f32 + f32) }   (descartado pelo jogo)
//!     v>=3: x { from u32, from_dir u8, to u32, to_dir u8, n u8, n x (id u32 + t u8) }
//!   [v>=5] lugar u16 (índice+1 na tabela de lugares; 0 = sem lugar)
//! ```
//!
//! Não há lista de escadas no arquivo (o jogo as monta a partir das entidades do mapa).
//! Observação: `CNavArea::Save` do ReGameDLL grava a contagem de conexões como `size_t`
//! (8 bytes em build 64 bits), mas o `Load` — e os arquivos do jogo, gerados em 32 bits —
//! usam `u32`; aqui vale o `Load`.
//!
//! Não conferido contra arquivo real: nenhum `.nav` da Valve está no repositório; os
//! testes usam arquivos montados byte a byte a partir do `Save`/`Load` acima. Versão
//! desconhecida (> 5) devolve só o cabeçalho com aviso.

use serde::Serialize;

pub const NAV_MAGIC: u32 = 0xFEED_FACE;
/// Maior versão que o ReGameDLL (e o CS 1.6/CZ) conhece.
pub const NAV_MAX_VERSION: u32 = 5;
/// Teto de áreas devolvidas na lista (as contagens continuam exatas).
pub const MAX_LISTED_AREAS: usize = 50_000;
const MAX_WARNINGS: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NavArea {
    pub id: u32,
    /// bits: 1 crouch, 2 jump, 4 precise, 8 no_jump
    pub attributes: u8,
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// altura dos cantos nordeste e sudoeste (o relevo da área)
    pub ne_z: f32,
    pub sw_z: f32,
    /// conexões de saída por direção: `[norte, leste, sul, oeste]`
    pub connections: [u32; 4],
    pub hiding_spots: u32,
    /// nome do lugar (v5), `None` se a área não tem
    pub place: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct NavInfo {
    pub version: u32,
    /// tamanho do .bsp gravado no nav (v>=4)
    pub bsp_size: Option<u32>,
    /// preenchido por quem conhece o .bsp (`check_bsp_size`)
    pub bsp_size_matches: Option<bool>,
    /// `false` quando a versão é desconhecida e só o cabeçalho foi lido
    pub parsed: bool,
    pub area_count: u32,
    pub connection_count: u64,
    pub hiding_spot_count: u64,
    pub approach_count: u64,
    pub encounter_count: u64,
    pub places: Vec<String>,
    /// caixa que envolve todas as áreas lidas: `[minx,miny,minz,maxx,maxy,maxz]`
    pub bounds: Option<[f32; 6]>,
    pub areas: Vec<NavArea>,
    /// `true` se `areas` foi cortada em [`MAX_LISTED_AREAS`]
    pub areas_truncated: bool,
    pub warnings: Vec<String>,
}

/// Compara o tamanho gravado no nav com o tamanho real do `.bsp`.
pub fn check_bsp_size(info: &mut NavInfo, bsp_len: u64) {
    if let Some(sz) = info.bsp_size {
        let ok = sz as u64 == bsp_len;
        info.bsp_size_matches = Some(ok);
        if !ok && sz != 0 {
            warn(info, format!("nav gerado para um .bsp de {sz} bytes, mas o mapa tem {bsp_len}: os bots andam mal"));
        }
    }
}

fn warn(info: &mut NavInfo, msg: String) {
    if info.warnings.len() < MAX_WARNINGS {
        info.warnings.push(msg);
    }
}

/// Leitor little-endian com limites checados; `None` = acabou o arquivo.
struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn remaining(&self) -> usize {
        self.d.len() - self.p
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.p.checked_add(n)?;
        let s = self.d.get(self.p..end)?;
        self.p = end;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|s| u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }
    /// pula `n` registros de `size` bytes, recusando contagem que passa do arquivo
    fn skip_records(&mut self, n: usize, size: usize) -> Option<()> {
        self.skip(n.checked_mul(size)?)
    }
}

/// Lê a malha de navegação. Só é erro o que impede ler o cabeçalho (arquivo curto,
/// magic errado, versão 0); dado torto depois disso devolve o que deu para ler + aviso.
pub fn parse_nav(bytes: &[u8]) -> Result<NavInfo, String> {
    let mut r = Rd { d: bytes, p: 0 };
    let magic = r.u32().ok_or("arquivo vazio ou menor que o cabeçalho (.nav precisa de 8 bytes)")?;
    if magic != NAV_MAGIC {
        return Err(format!("não é um .nav: magic 0x{magic:08X} (esperado 0x{NAV_MAGIC:08X})"));
    }
    let version = r.u32().ok_or("cabeçalho do .nav truncado (sem versão)")?;
    if version == 0 {
        return Err("versão 0 não existe no formato .nav".to_string());
    }
    let mut info = NavInfo { version, ..Default::default() };
    if version > NAV_MAX_VERSION {
        // só o que é estável entre versões: o tamanho do bsp vem logo depois da versão desde a v4
        info.bsp_size = r.u32();
        warn(
            &mut info,
            format!("versão {version} do .nav desconhecida (o CS 1.6/CZ usa até {NAV_MAX_VERSION}); só o cabeçalho foi lido"),
        );
        return Ok(info);
    }
    info.parsed = true;
    if version >= 4 {
        match r.u32() {
            Some(s) => info.bsp_size = Some(s),
            None => {
                warn(&mut info, "arquivo termina antes do tamanho do bsp".to_string());
                return Ok(info);
            }
        }
    }
    if version >= 5 {
        let Some(count) = r.u16() else {
            warn(&mut info, "arquivo termina antes da tabela de lugares".to_string());
            return Ok(info);
        };
        for i in 0..count {
            let name = r.u16().and_then(|len| r.take(len as usize));
            match name {
                Some(raw) => {
                    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                    info.places.push(String::from_utf8_lossy(&raw[..end]).trim().to_string());
                }
                None => {
                    warn(&mut info, format!("tabela de lugares truncada no lugar {i} de {count}"));
                    return Ok(info);
                }
            }
        }
    }
    let Some(area_count) = r.u32() else {
        warn(&mut info, "arquivo termina antes da contagem de áreas".to_string());
        return Ok(info);
    };
    info.area_count = area_count;
    if area_count == 0 {
        warn(&mut info, "nav sem nenhuma área (o jogo trata como arquivo inválido)".to_string());
    }
    // cada área ocupa no mínimo 4+1+24+8+16+1+1+4 = 59 bytes: contagem maior que o
    // arquivo comporta é lixo, mas lemos até onde der em vez de alocar pela contagem
    let mut bounds: Option<[f32; 6]> = None;
    for n in 0..area_count {
        match read_area(&mut r, version, &info.places) {
            Some((area, stats)) => {
                info.connection_count += area.connections.iter().map(|&c| c as u64).sum::<u64>();
                info.hiding_spot_count += area.hiding_spots as u64;
                info.approach_count += stats.approach as u64;
                info.encounter_count += stats.encounters as u64;
                let b = bounds.get_or_insert([f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN]);
                for k in 0..3 {
                    if area.min[k].is_finite() {
                        b[k] = b[k].min(area.min[k]);
                    }
                    if area.max[k].is_finite() {
                        b[k + 3] = b[k + 3].max(area.max[k]);
                    }
                }
                if area.min[0] >= area.max[0] || area.min[1] >= area.max[1] {
                    warn(&mut info, format!("área {} degenerada (extensão zero ou invertida em X/Y)", area.id));
                }
                if info.areas.len() < MAX_LISTED_AREAS {
                    info.areas.push(area);
                } else {
                    info.areas_truncated = true;
                }
            }
            None => {
                warn(&mut info, format!("arquivo truncado ou corrompido na área {n} de {area_count}; resultado parcial"));
                break;
            }
        }
    }
    info.bounds = bounds.filter(|b| b.iter().all(|v| v.is_finite()));
    Ok(info)
}

struct AreaStats {
    approach: u32,
    encounters: u32,
}

fn read_area(r: &mut Rd, version: u32, places: &[String]) -> Option<(NavArea, AreaStats)> {
    let id = r.u32()?;
    let attributes = r.u8()?;
    let min = [r.f32()?, r.f32()?, r.f32()?];
    let max = [r.f32()?, r.f32()?, r.f32()?];
    let ne_z = r.f32()?;
    let sw_z = r.f32()?;
    let mut connections = [0u32; 4];
    for c in connections.iter_mut() {
        let n = r.u32()?;
        r.skip_records(n as usize, 4)?;
        *c = n;
    }
    let hiding = r.u8()? as usize;
    r.skip_records(hiding, if version == 1 { 12 } else { 17 })?;
    let approach = r.u8()? as usize;
    r.skip_records(approach, 14)?;
    let encounters = r.u32()?;
    if version < 3 {
        for _ in 0..encounters {
            r.skip(8 + 24)?;
            let n = r.u8()? as usize;
            r.skip_records(n, 16)?;
        }
    } else {
        // cada encontro tem no mínimo 4+1+4+1+1 = 11 bytes
        if (encounters as usize).checked_mul(11)? > r.remaining() {
            return None;
        }
        for _ in 0..encounters {
            r.skip(4 + 1 + 4 + 1)?;
            let n = r.u8()? as usize;
            r.skip_records(n, 5)?;
        }
    }
    let place = if version >= 5 {
        let entry = r.u16()? as usize;
        if entry == 0 {
            None
        } else {
            places.get(entry - 1).cloned()
        }
    } else {
        None
    };
    Some((
        NavArea { id, attributes, min, max, ne_z, sw_z, connections, hiding_spots: hiding as u32, place },
        AreaStats { approach: approach as u32, encounters },
    ))
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    pub struct AreaSpec {
        pub id: u32,
        pub attrs: u8,
        pub min: [f32; 3],
        pub max: [f32; 3],
        pub conn: [Vec<u32>; 4],
        pub hiding: u8,
        pub approach: u8,
        pub encounters: Vec<Vec<(u32, u8)>>,
        pub place: u16,
    }

    pub fn area(id: u32, min: [f32; 3], max: [f32; 3]) -> AreaSpec {
        AreaSpec {
            id,
            attrs: 0,
            min,
            max,
            conn: Default::default(),
            hiding: 0,
            approach: 0,
            encounters: vec![],
            place: 0,
        }
    }

    pub fn build_nav(version: u32, bsp_size: u32, places: &[&str], areas: &[AreaSpec]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&NAV_MAGIC.to_le_bytes());
        v.extend_from_slice(&version.to_le_bytes());
        if version >= 4 {
            v.extend_from_slice(&bsp_size.to_le_bytes());
        }
        if version >= 5 {
            v.extend_from_slice(&(places.len() as u16).to_le_bytes());
            for p in places {
                v.extend_from_slice(&(p.len() as u16 + 1).to_le_bytes());
                v.extend_from_slice(p.as_bytes());
                v.push(0);
            }
        }
        v.extend_from_slice(&(areas.len() as u32).to_le_bytes());
        for a in areas {
            v.extend_from_slice(&a.id.to_le_bytes());
            v.push(a.attrs);
            for f in a.min.iter().chain(a.max.iter()) {
                v.extend_from_slice(&f.to_le_bytes());
            }
            v.extend_from_slice(&a.max[2].to_le_bytes()); // ne_z
            v.extend_from_slice(&a.min[2].to_le_bytes()); // sw_z
            for d in &a.conn {
                v.extend_from_slice(&(d.len() as u32).to_le_bytes());
                for id in d {
                    v.extend_from_slice(&id.to_le_bytes());
                }
            }
            v.push(a.hiding);
            for h in 0..a.hiding {
                if version >= 2 {
                    v.extend_from_slice(&(h as u32 + 1).to_le_bytes());
                }
                for f in [1.0f32, 2.0, 3.0] {
                    v.extend_from_slice(&f.to_le_bytes());
                }
                if version >= 2 {
                    v.push(1);
                }
            }
            v.push(a.approach);
            for _ in 0..a.approach {
                v.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0, 1, 3, 0, 0, 0, 0]);
            }
            v.extend_from_slice(&(a.encounters.len() as u32).to_le_bytes());
            for enc in &a.encounters {
                if version < 3 {
                    v.extend_from_slice(&[0u8; 8 + 24]);
                    v.push(enc.len() as u8);
                    for _ in enc {
                        v.extend_from_slice(&[0u8; 16]);
                    }
                } else {
                    v.extend_from_slice(&1u32.to_le_bytes());
                    v.push(0);
                    v.extend_from_slice(&2u32.to_le_bytes());
                    v.push(2);
                    v.push(enc.len() as u8);
                    for (id, t) in enc {
                        v.extend_from_slice(&id.to_le_bytes());
                        v.push(*t);
                    }
                }
            }
            if version >= 5 {
                v.extend_from_slice(&a.place.to_le_bytes());
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn two_areas() -> Vec<AreaSpec> {
        let mut a = area(1, [0.0, 0.0, 0.0], [100.0, 50.0, 8.0]);
        a.conn[1] = vec![2];
        a.hiding = 2;
        a.approach = 1;
        a.encounters = vec![vec![(1, 10), (2, 200)]];
        a.attrs = 1;
        a.place = 2;
        let mut b = area(2, [100.0, -20.0, 4.0], [180.0, 60.0, 12.0]);
        b.conn[3] = vec![1];
        (vec![a, b]).into_iter().collect()
    }

    #[test]
    fn v5_completo() {
        let bytes = build_nav(5, 123_456, &["BombsiteA", "Spawn"], &two_areas());
        let n = parse_nav(&bytes).unwrap();
        assert!(n.parsed);
        assert_eq!((n.version, n.bsp_size, n.area_count), (5, Some(123_456), 2));
        assert_eq!(n.places, vec!["BombsiteA", "Spawn"]);
        assert_eq!(n.areas.len(), 2);
        assert_eq!(n.areas[0].connections, [0, 1, 0, 0]);
        assert_eq!(n.areas[0].place.as_deref(), Some("Spawn"));
        assert_eq!(n.areas[1].place, None);
        assert_eq!(n.areas[0].attributes, 1);
        assert_eq!((n.connection_count, n.hiding_spot_count, n.approach_count, n.encounter_count), (2, 2, 1, 1));
        assert_eq!(n.bounds, Some([0.0, -20.0, 0.0, 180.0, 60.0, 12.0]));
        assert!(n.warnings.is_empty(), "{:?}", n.warnings);
    }

    #[test]
    fn versoes_1_a_4_tambem_leem() {
        for v in 1..=4u32 {
            let n = parse_nav(&build_nav(v, 777, &[], &two_areas())).unwrap();
            assert!(n.parsed, "v{v}");
            assert_eq!(n.areas.len(), 2, "v{v}");
            assert_eq!(n.hiding_spot_count, 2, "v{v}");
            assert_eq!(n.areas[1].connections, [0, 0, 0, 1], "v{v}");
            assert_eq!(n.bsp_size, if v >= 4 { Some(777) } else { None });
            assert!(n.areas[0].place.is_none());
            assert!(n.warnings.is_empty(), "v{v}: {:?}", n.warnings);
        }
    }

    #[test]
    fn versao_desconhecida_devolve_so_o_cabecalho() {
        let mut b = build_nav(5, 99, &[], &[]);
        b[4..8].copy_from_slice(&9u32.to_le_bytes());
        let n = parse_nav(&b).unwrap();
        assert!(!n.parsed);
        assert_eq!(n.version, 9);
        assert!(n.areas.is_empty());
        assert!(n.warnings[0].contains("desconhecida"));
    }

    #[test]
    fn cabecalho_invalido() {
        assert!(parse_nav(b"").is_err());
        assert!(parse_nav(&NAV_MAGIC.to_le_bytes()).is_err());
        assert!(parse_nav(b"\0\0\0\0\x05\0\0\0").unwrap_err().contains("magic"));
        let mut b = build_nav(5, 1, &[], &[]);
        b[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_nav(&b).is_err());
    }

    #[test]
    fn truncado_devolve_parcial_com_aviso() {
        let bytes = build_nav(5, 1, &["A"], &two_areas());
        let cut = bytes.len() - 20; // corta no meio da 2ª área
        let n = parse_nav(&bytes[..cut]).unwrap();
        assert_eq!(n.areas.len(), 1);
        assert!(n.warnings.iter().any(|w| w.contains("truncado")));
    }

    #[test]
    fn contagens_absurdas_nao_alocam_nem_estouram() {
        // 4 bilhões de áreas com corpo vazio
        let mut b = build_nav(5, 1, &[], &[]);
        let n = b.len();
        b[n - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        let info = parse_nav(&b).unwrap();
        assert_eq!(info.area_count, u32::MAX);
        assert!(info.areas.is_empty());
        // conexões gigantes numa área
        let mut b = build_nav(2, 0, &[], &[area(1, [0.0; 3], [1.0; 3])]);
        let at = 8 + 4 + 4 + 1 + 24 + 8; // início da contagem da direção norte
        b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let info = parse_nav(&b).unwrap();
        assert!(info.areas.is_empty());
        assert!(!info.warnings.is_empty());
        // lugares gigantes
        let mut b = build_nav(5, 0, &[], &[]);
        b[12..14].copy_from_slice(&u16::MAX.to_le_bytes());
        let _ = parse_nav(&b).unwrap();
    }

    #[test]
    fn area_degenerada_gera_aviso() {
        let n = parse_nav(&build_nav(5, 1, &[], &[area(7, [10.0, 10.0, 0.0], [10.0, 20.0, 0.0])])).unwrap();
        assert!(n.warnings.iter().any(|w| w.contains("degenerada")));
    }

    #[test]
    fn nan_nos_extents_nao_quebra_a_caixa() {
        let n = parse_nav(&build_nav(
            5,
            1,
            &[],
            &[area(1, [f32::NAN, 0.0, 0.0], [f32::INFINITY, 5.0, 5.0]), area(2, [0.0; 3], [4.0; 3])],
        ))
        .unwrap();
        assert_eq!(n.bounds, Some([0.0, 0.0, 0.0, 4.0, 5.0, 5.0]));
    }

    #[test]
    fn tamanho_do_bsp() {
        let mut n = parse_nav(&build_nav(5, 1000, &[], &two_areas())).unwrap();
        check_bsp_size(&mut n, 1000);
        assert_eq!(n.bsp_size_matches, Some(true));
        check_bsp_size(&mut n, 999);
        assert_eq!(n.bsp_size_matches, Some(false));
        assert!(n.warnings.iter().any(|w| w.contains("bots")));
        let mut v3 = parse_nav(&build_nav(3, 0, &[], &two_areas())).unwrap();
        check_bsp_size(&mut v3, 5);
        assert_eq!(v3.bsp_size_matches, None);
    }

    #[test]
    fn lixo_e_truncamentos_nao_dao_panic() {
        let full = build_nav(5, 1, &["X"], &two_areas());
        for cut in 0..full.len() {
            let _ = parse_nav(&full[..cut]);
        }
        let mut seed = 7u32;
        for round in 0..500 {
            let mut buf: Vec<u8> = (0..(round % 200))
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 24) as u8
                })
                .collect();
            if round % 2 == 0 && buf.len() >= 8 {
                buf[0..4].copy_from_slice(&NAV_MAGIC.to_le_bytes());
                buf[4..8].copy_from_slice(&((round as u32 % 6) + 1).to_le_bytes());
            }
            let _ = parse_nav(&buf);
        }
        for at in 0..full.len() {
            let mut c = full.clone();
            c[at] ^= 0xA5;
            let _ = parse_nav(&c);
        }
    }
}
