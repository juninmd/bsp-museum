//! Estilos de luz (*lightstyles*): luz que pisca, pulsa ou liga/desliga.
//!
//! Cada face do BSP tem até 4 estilos (`styles[0..4]`, `255` = vazio) e, no lump de
//! iluminação, **um bloco de lightmap por estilo, um depois do outro**: o bloco do estilo
//! `k` começa em `lightofs + k * largura * altura * bytes_por_texel`. O motor soma os
//! blocos, cada um multiplicado pelo valor atual do padrão do estilo.
//!
//! Padrão = string de letras `a..z`; a letra `c` vale `(c - 'a') * 22` (de 0 a 550) e `m`
//! (264) é o brilho normal. O motor avança uma letra a cada 0,1 s (`cl.time * 10`, do
//! Quake). Aqui o valor é normalizado para que `m` = 1.0.
//!
//! Padrões de fábrica: os de `world.cpp` do SDK do Half-Life (mesmos de `world.qc` do
//! Quake), de memória — não conferidos contra um motor rodando. O estilo `11` é "pulso
//! lento sem chegar ao preto" (não "sem luz"); `12` é a "mutação de luz submarina". Estilos
//! `32..=62` são atribuídos pelo compilador a luzes ligáveis (`light` com `targetname`).

use serde::Serialize;

/// Quadros por segundo do padrão (uma letra por 0,1 s).
pub const STYLE_FPS: f32 = 10.0;

/// Valor da letra `m`: o "brilho normal".
const NORMAL: f32 = 12.0 * 22.0;

/// Estilos 0..=12 do motor.
pub const DEFAULT_PATTERNS: [(&str, &str); 13] = [
    ("normal", "m"),
    ("flicker 1", "mmnmmommommnonmmonqnmmo"),
    ("pulso lento forte", "abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcba"),
    ("vela 1", "mmmmmaaaaammmmmaaaaaabcdefgabcdefg"),
    ("strobe rápido", "mamamamamama"),
    ("pulso suave", "jklmnopqrstuvwxyzyxwvutsrqponmlkj"),
    ("flicker 2", "nmonqnmomnmomomno"),
    ("vela 2", "mmmaaaabcdefgmmmmaaaammmaamm"),
    ("vela 3", "mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa"),
    ("strobe lento", "aaaaaaaazzzzzzzz"),
    ("fluorescente", "mmamammmmammamamaaamammma"),
    ("pulso lento sem preto", "abcdefghijklmnopqrrqponmlkjihgfedcba"),
    ("mutação submarina", "mmnnmmnnnmmnn"),
];

/// Padrão de fábrica do estilo (`63` é o estilo de teste: `"a"`, escuro).
pub fn default_pattern(id: u8) -> Option<&'static str> {
    match id {
        0..=12 => Some(DEFAULT_PATTERNS[id as usize].1),
        63 => Some("a"),
        _ => None,
    }
}

/// Multiplicador de cada letra do padrão (`m` = 1.0). Letra fora de `a..z` vira `m`.
pub fn pattern_values(pattern: &str) -> Vec<f32> {
    pattern
        .bytes()
        .take(256)
        .map(|c| if c.is_ascii_lowercase() { f32::from(c - b'a') * 22.0 / NORMAL } else { 1.0 })
        .collect()
}

/// Multiplicador do padrão no instante `time` (segundos). Padrão vazio vale 1.0.
pub fn value_at(pattern: &str, time: f32) -> f32 {
    let values = pattern_values(pattern);
    if values.is_empty() || !time.is_finite() {
        return 1.0;
    }
    let frame = (time.max(0.0) * STYLE_FPS) as usize;
    values[frame % values.len()]
}

/// De onde veio o padrão do estilo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleSource {
    /// tabela de fábrica do motor (0..=12, 63)
    Engine,
    /// uma entidade `light` do mapa declara o estilo (ligável: 32+)
    LightEntity,
    /// nenhum padrão conhecido: usa `m` (luz normal)
    Unknown,
}

/// Definição de um estilo para o shader do viewer.
#[derive(Debug, Clone, Serialize)]
pub struct LightStyleDef {
    pub id: u8,
    pub name: String,
    pub pattern: String,
    /// multiplicador por letra (`m` = 1.0), pronto para amostrar a `STYLE_FPS`
    pub values: Vec<f32>,
    pub fps: f32,
    pub source: StyleSource,
    /// luz que um gatilho liga/desliga (estilo >= 32 de `light` com `targetname`)
    pub switchable: bool,
    pub targetname: Option<String>,
    /// começa apagada (`spawnflags & 1` da `light`)
    pub start_off: bool,
    /// faces do mapa que usam o estilo em algum dos 4 slots
    pub faces: usize,
}

fn def(id: u8, name: &str, pattern: &str, source: StyleSource) -> LightStyleDef {
    LightStyleDef {
        id,
        name: name.to_string(),
        pattern: pattern.to_string(),
        values: pattern_values(pattern),
        fps: STYLE_FPS,
        source,
        switchable: false,
        targetname: None,
        start_off: false,
        faces: 0,
    }
}

/// Estilos que o mapa usa: os das faces (`styles[]` != 255) e os declarados por `light`.
/// Estilo 0 sempre entra (é o `m` estático).
pub fn collect(bsp: &super::Bsp, parsed: &[super::entities::Entity]) -> Vec<LightStyleDef> {
    use std::collections::BTreeMap;
    let mut used: BTreeMap<u8, usize> = BTreeMap::new();
    for face in &bsp.faces {
        for &s in &face.styles {
            if s == 255 {
                break;
            }
            *used.entry(s).or_insert(0) += 1;
        }
    }

    // luzes que declaram estilo (>= 32 vira ligável; < 32 só confirma o padrão de fábrica)
    struct Declared {
        pattern: Option<String>,
        targetname: Option<String>,
        start_off: bool,
    }
    let mut declared: BTreeMap<u8, Declared> = BTreeMap::new();
    for e in parsed {
        let class = e.get("classname").map(String::as_str).unwrap_or("");
        if !class.starts_with("light") {
            continue;
        }
        let Some(style) = e.get("style").and_then(|s| s.trim().parse::<i32>().ok()) else { continue };
        let Ok(style) = u8::try_from(style) else { continue };
        let flags = e.get("spawnflags").and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0);
        declared.entry(style).or_insert(Declared {
            pattern: e.get("pattern").map(|p| p.trim().to_ascii_lowercase()).filter(|p| !p.is_empty()),
            targetname: e.get("targetname").filter(|t| !t.is_empty()).cloned(),
            start_off: flags & 1 != 0,
        });
    }

    used.entry(0).or_insert(0);
    for &id in declared.keys() {
        used.entry(id).or_insert(0);
    }

    used.into_iter()
        .map(|(id, faces)| {
            let mut d = if let Some(p) = default_pattern(id) {
                let name = DEFAULT_PATTERNS.get(id as usize).map(|x| x.0).unwrap_or("teste");
                def(id, name, p, StyleSource::Engine)
            } else {
                def(id, &format!("estilo {id}"), "m", StyleSource::Unknown)
            };
            if let Some(decl) = declared.get(&id) {
                if id >= 32 {
                    // CLight::Spawn: >= 32 usa "pattern" (ou "m"); desligada de início vira "a"
                    let pattern = if decl.start_off { "a".to_string() } else { decl.pattern.clone().unwrap_or_else(|| "m".into()) };
                    d = def(id, &format!("luz ligável {id}"), &pattern, StyleSource::LightEntity);
                    d.switchable = decl.targetname.is_some();
                    d.targetname = decl.targetname.clone();
                    d.start_off = decl.start_off;
                }
            }
            d.faces = faces;
            d
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsp::entities;
    use crate::bsp::{Bsp, Face};

    #[test]
    fn tabela_de_fabrica_tem_os_padroes_conhecidos() {
        assert_eq!(default_pattern(0), Some("m"));
        assert_eq!(default_pattern(1), Some("mmnmmommommnonmmonqnmmo"));
        assert_eq!(default_pattern(4), Some("mamamamamama"));
        assert_eq!(default_pattern(11), Some("abcdefghijklmnopqrrqponmlkjihgfedcba"));
        assert_eq!(default_pattern(12), Some("mmnnmmnnnmmnn"));
        assert_eq!(default_pattern(63), Some("a"));
        assert_eq!(default_pattern(13), None);
        assert_eq!(default_pattern(40), None);
    }

    #[test]
    fn letra_m_e_o_brilho_normal() {
        let v = pattern_values("amz");
        assert_eq!(v[0], 0.0);
        assert!((v[1] - 1.0).abs() < 1e-6);
        assert!((v[2] - 25.0 * 22.0 / 264.0).abs() < 1e-5);
    }

    #[test]
    fn padrao_avanca_dez_letras_por_segundo() {
        // "az": 0.0-0.1 s = a (0), 0.1-0.2 s = z
        assert_eq!(value_at("az", 0.05), 0.0);
        assert!(value_at("az", 0.15) > 2.0);
        assert_eq!(value_at("az", 0.25), 0.0); // volta ao começo
        assert_eq!(value_at("", 1.0), 1.0);
        assert_eq!(value_at("az", f32::NAN), 1.0);
    }

    #[test]
    fn letras_invalidas_viram_normal() {
        assert_eq!(pattern_values("A!"), vec![1.0, 1.0]);
    }

    #[test]
    fn coleta_estilos_das_faces_e_das_luzes() {
        let bsp = Bsp {
            faces: vec![
                Face { first_edge: 0, num_edges: 0, texinfo: 0, lightofs: 0, styles: [0, 1, 255, 255] },
                Face { first_edge: 0, num_edges: 0, texinfo: 0, lightofs: 0, styles: [0, 33, 255, 255] },
            ],
            ..Default::default()
        };
        let ents = entities::parse(
            "{\"classname\" \"light\" \"style\" \"33\" \"targetname\" \"lampada\" \"pattern\" \"az\"}\
             {\"classname\" \"light\" \"style\" \"34\" \"spawnflags\" \"1\" \"targetname\" \"l2\"}",
        );
        let defs = collect(&bsp, &ents);
        let ids: Vec<u8> = defs.iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![0, 1, 33, 34]);
        assert_eq!(defs[0].faces, 2);
        assert_eq!(defs[1].source, StyleSource::Engine);
        let l33 = &defs[2];
        assert_eq!((l33.pattern.as_str(), l33.switchable, l33.faces), ("az", true, 1));
        assert_eq!(l33.source, StyleSource::LightEntity);
        // desligada de início
        assert_eq!(defs[3].pattern, "a");
        assert!(defs[3].start_off);
    }

    #[test]
    fn estilo_desconhecido_cai_em_luz_normal() {
        let bsp = Bsp {
            faces: vec![Face { first_edge: 0, num_edges: 0, texinfo: 0, lightofs: 0, styles: [20, 255, 255, 255] }],
            ..Default::default()
        };
        let defs = collect(&bsp, &[]);
        let d = defs.iter().find(|d| d.id == 20).unwrap();
        assert_eq!(d.source, StyleSource::Unknown);
        assert_eq!(d.pattern, "m");
    }
}
