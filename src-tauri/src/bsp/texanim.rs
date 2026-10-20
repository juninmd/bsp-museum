//! Texturas animadas: `+0nome`…`+9nome` (quadros) e `+Anome`…`+Jnome` (alternativa).
//!
//! Convenção herdada do Quake (`Mod_LoadTextures`): o nome começa com `+`, o segundo
//! caractere é o número do quadro (`0`–`9`) ou a letra do quadro alternativo (`a`–`j`,
//! sem diferenciar maiúscula) e o resto é o nome-base. As faces usam o quadro `+0`; o
//! motor troca pelos outros ciclicamente a 10 quadros por segundo
//! (`(int)(cl.time*10) % total`) e, quando a entidade está "ativa" (botão apertado,
//! `frame = 1`), usa a sequência alternativa `+A…`. Conferido de memória do código do
//! Quake/SDK, não contra um GoldSrc rodando.
//!
//! Os quadros não precisam estar todos no BSP: no GoldSrc só a textura usada pela face
//! entra na tabela e os demais quadros vêm do WAD pelo nome.

use serde::Serialize;
use std::collections::BTreeMap;

/// Segundos por quadro.
pub const FRAME_INTERVAL: f32 = 0.1;

#[derive(Debug, Clone, Serialize)]
pub struct TextureAnim {
    /// nome-base, sem o `+N` (minúsculo)
    pub name: String,
    /// nome da textura que as faces referenciam (`+0nome`, ou `+Anome` se só há alternativa)
    pub texture: String,
    /// quadros `+0`…`+9`, em ordem; vai até o primeiro buraco
    pub frames: Vec<String>,
    /// quadros `+A`…`+J` (sequência do botão ligado)
    pub alt_frames: Vec<String>,
    /// segundos por quadro
    pub interval: f32,
    /// `false` quando há um buraco na numeração (o motor reclama de "missing frame")
    pub complete: bool,
    /// índice em `MeshDetail.textures` de cada quadro (`-1` = sem imagem); preenchido pelo catálogo
    pub frame_slots: Vec<i32>,
    pub alt_slots: Vec<i32>,
}

/// `(base minúscula, eh_alternativa, índice)` de um nome `+Xnome`.
pub fn parse_name(name: &str) -> Option<(String, bool, usize)> {
    let mut chars = name.chars();
    if chars.next()? != '+' {
        return None;
    }
    let c = chars.next()?.to_ascii_lowercase();
    let rest: String = chars.collect();
    if rest.is_empty() {
        return None;
    }
    let base = rest.to_ascii_lowercase();
    match c {
        '0'..='9' => Some((base, false, (c as u8 - b'0') as usize)),
        'a'..='j' => Some((base, true, (c as u8 - b'a') as usize)),
        _ => None,
    }
}

/// Nome do quadro `index` de um grupo (`alt` escolhe a sequência `A–J`).
pub fn frame_name(base: &str, alt: bool, index: usize) -> String {
    let c = if alt { (b'A' + index.min(9) as u8) as char } else { (b'0' + index.min(9) as u8) as char };
    format!("+{c}{base}")
}

/// Agrupa os nomes de textura. `extra` são nomes que existem fora do BSP (índice dos
/// WADs, minúsculos): completam quadros que o mapa não embutiu, mas só para grupos que o
/// mapa já usa. Grupo com um único quadro e nenhuma alternativa não é animação.
pub fn group<'a>(names: impl Iterator<Item = &'a str>, extra: &dyn Fn(&str) -> bool) -> Vec<TextureAnim> {
    // base -> (quadros[10], alternativos[10]) com o nome original de cada um
    type Slots = [Option<String>; 10];
    let mut map: BTreeMap<String, (Slots, Slots)> = BTreeMap::new();
    for name in names {
        if let Some((base, alt, i)) = parse_name(name) {
            let entry = map.entry(base).or_default();
            let slot = if alt { &mut entry.1[i] } else { &mut entry.0[i] };
            if slot.is_none() {
                *slot = Some(name.to_string());
            }
        }
    }

    let mut out = Vec::new();
    for (base, (mut frames, mut alts)) in map {
        for (alt, slots) in [(false, &mut frames), (true, &mut alts)] {
            for (i, slot) in slots.iter_mut().enumerate() {
                if slot.is_none() {
                    let candidate = frame_name(&base, alt, i);
                    if extra(&candidate.to_ascii_lowercase()) {
                        *slot = Some(candidate);
                    }
                }
            }
        }
        let run = |slots: &Slots| -> (Vec<String>, bool) {
            let list: Vec<String> = slots.iter().map_while(|s| s.clone()).collect();
            let total = slots.iter().filter(|s| s.is_some()).count();
            let contiguous = list.len() == total;
            (list, contiguous)
        };
        let (f, f_ok) = run(&frames);
        let (a, a_ok) = run(&alts);
        if f.len() < 2 && a.is_empty() {
            continue;
        }
        let texture = f.first().or(a.first()).cloned().unwrap_or_default();
        out.push(TextureAnim {
            name: base,
            texture,
            frame_slots: Vec::new(),
            alt_slots: Vec::new(),
            frames: f,
            alt_frames: a,
            interval: FRAME_INTERVAL,
            complete: f_ok && a_ok,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> bool {
        false
    }

    #[test]
    fn le_nome_de_quadro_e_de_alternativa() {
        assert_eq!(parse_name("+0AGUA"), Some(("agua".into(), false, 0)));
        assert_eq!(parse_name("+3luz"), Some(("luz".into(), false, 3)));
        assert_eq!(parse_name("+Bbotao"), Some(("botao".into(), true, 1)));
        assert_eq!(parse_name("+jbotao"), Some(("botao".into(), true, 9)));
        assert_eq!(parse_name("+kbotao"), None);
        assert_eq!(parse_name("agua"), None);
        assert_eq!(parse_name("+0"), None);
        assert_eq!(parse_name(""), None);
    }

    #[test]
    fn agrupa_quadros_em_ordem_e_acha_alternativa() {
        let names = ["+2tela", "+0tela", "+1tela", "concrete", "+0botao", "+Abotao", "+1botao", "+0sozinha"];
        let g = group(names.iter().copied(), &none);
        let tela = g.iter().find(|a| a.name == "tela").unwrap();
        assert_eq!(tela.frames, vec!["+0tela", "+1tela", "+2tela"]);
        assert!(tela.alt_frames.is_empty());
        assert!(tela.complete);
        assert!((tela.interval - 0.1).abs() < 1e-6);
        let botao = g.iter().find(|a| a.name == "botao").unwrap();
        assert_eq!(botao.frames.len(), 2);
        assert_eq!(botao.alt_frames, vec!["+Abotao"]);
        assert_eq!(botao.texture, "+0botao");
        // um quadro e nada mais: não é animação
        assert!(g.iter().all(|a| a.name != "sozinha"));
    }

    #[test]
    fn buraco_na_numeracao_marca_incompleto() {
        let g = group(["+0x", "+1x", "+3x"].into_iter(), &none);
        assert_eq!(g[0].frames, vec!["+0x", "+1x"]);
        assert!(!g[0].complete);
    }

    #[test]
    fn quadros_ausentes_vem_do_indice_do_wad() {
        let wad = |n: &str| matches!(n, "+1fogo" | "+2fogo");
        let g = group(["+0fogo"].into_iter(), &wad);
        assert_eq!(g[0].frames, vec!["+0fogo", "+1fogo", "+2fogo"]);
    }

    #[test]
    fn so_alternativa_usa_ela_como_textura() {
        let g = group(["+Atv", "+Btv"].into_iter(), &none);
        assert_eq!(g[0].texture, "+Atv");
        assert!(g[0].frames.is_empty());
    }

    #[test]
    fn lixo_de_nome_nao_derruba() {
        let g = group(["+", "++", "+0", "+€x", "é"].into_iter(), &none);
        assert!(g.is_empty());
    }
}
