//! Arquivos de texto do mod: `liblist.gam`, `sprites/hud.txt` e `sprites/weapon_*.txt`,
//! `maps/<mapa>.txt`, `maps/<mapa>_detail.txt`, `mapcycle.txt` e `motd.txt`.
//!
//! Parsers tolerantes: comentário `//` (fora de aspas), aspas opcionais, linhas
//! tortas viram aviso e são puladas — nunca erro nem panic.
//!
//! Conferido contra código/documentação:
//! - `_detail.txt`: `Mod_ParseDetailTextures` do Xash3D (`engine/common/mod_bmodel.c`) —
//!   `textura detalhe xScale yScale`, com a manha de `{` solto ao ler nomes (`{fence`
//!   pode vir como `{` + `fence`) e escala `<= 0` ignorada; o detalhe é carregado de
//!   `gfx/<detalhe>`.
//! - `hud.txt`/`weapon_*.txt`: o carregador do cliente (`hud.cpp` do hlsdk-portable)
//!   monta `sprites/<arquivo>.spr` e filtra pela resolução 320/640 (a maior que o
//!   `hud.txt` declara entra no teto); o formato `contagem` + linhas
//!   `objeto resolução arquivo x y largura altura` bate com a página do TWHL
//!   (consultada só por resumo de busca — a página bloqueou o fetch direto, HTTP 403).
//!
//! De memória, sem conferir contra arquivo da Valve: as chaves de `liblist.gam`
//! (`game`, `gamedll`, `gamedll_linux`, `startmap`, `trainmap`, `type`, `url_info`…),
//! o formato de `mapcycle.txt` (`mapa [min [max]]`, o CS/AMX Mod X aceitam os limites
//! de jogadores opcionais), `motd.txt` (texto ou HTML) e `maps/<mapa>.txt`
//! (descrição do mapa mostrada pelo jogo).

use serde::Serialize;

const MAX_WARNINGS: usize = 20;
/// Teto de caracteres devolvidos por [`parse_text`].
pub const MAX_TEXT_CHARS: usize = 256 * 1024;

fn warn(w: &mut Vec<String>, msg: String) {
    if w.len() < MAX_WARNINGS {
        w.push(msg);
    }
}

/// Decodifica texto de arquivo do jogo: UTF-8 se válido (aceitando corte no meio de um
/// caractere no fim), senão Latin-1 — os `.txt` antigos são ANSI. Tira BOM e `\r`.
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(e) if e.error_len().is_none() => String::from_utf8_lossy(&bytes[..e.valid_up_to()]).to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Remove o comentário `//` que não esteja dentro de aspas.
fn strip_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let mut in_quote = false;
    for i in 0..b.len() {
        match b[i] {
            b'"' => in_quote = !in_quote,
            b'/' if !in_quote && b.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Separa por espaço respeitando aspas; aspa sem par vai até o fim da linha.
fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut t = String::new();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                t.push(c);
            }
            out.push(t);
        } else {
            let mut t = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '"' {
                    break;
                }
                t.push(c);
                chars.next();
            }
            out.push(t);
        }
    }
    out
}

fn clean_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate().map(|(i, l)| (i + 1, strip_comment(l).trim())).filter(|(_, l)| !l.is_empty())
}

// ---------------------------------------------------------------------------------
// liblist.gam
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Liblist {
    /// todas as chaves, na ordem do arquivo, com a chave em minúsculas
    pub entries: Vec<KeyValue>,
    pub game: Option<String>,
    pub gamedll: Option<String>,
    pub gamedll_linux: Option<String>,
    pub startmap: Option<String>,
    pub trainmap: Option<String>,
    /// valor da chave `type` (`multiplayer_only`, `singleplayer_only`…)
    pub game_type: Option<String>,
    pub version: Option<String>,
    pub url_info: Option<String>,
    pub url_dl: Option<String>,
    pub warnings: Vec<String>,
}

/// `chave "valor"` ou `chave valor`, um par por linha.
pub fn parse_liblist(bytes: &[u8]) -> Liblist {
    let text = decode_text(bytes);
    let mut out = Liblist::default();
    for (n, line) in clean_lines(&text) {
        let (key, rest) = match line.find(char::is_whitespace) {
            Some(i) => (&line[..i], line[i..].trim()),
            None => (line, ""),
        };
        if key.contains('"') {
            warn(&mut out.warnings, format!("linha {n}: chave com aspas, ignorada"));
            continue;
        }
        let value = match rest.strip_prefix('"') {
            Some(r) => r.split('"').next().unwrap_or("").to_string(),
            None => rest.to_string(),
        };
        out.entries.push(KeyValue { key: key.to_ascii_lowercase(), value });
    }
    // chave repetida: o motor fica com a última
    let get = |k: &str| out.entries.iter().rev().find(|e| e.key == k).map(|e| e.value.clone());
    out.game = get("game");
    out.gamedll = get("gamedll");
    out.gamedll_linux = get("gamedll_linux");
    out.startmap = get("startmap");
    out.trainmap = get("trainmap");
    out.game_type = get("type");
    out.version = get("version");
    out.url_info = get("url_info");
    out.url_dl = get("url_dl");
    out
}

// ---------------------------------------------------------------------------------
// hud.txt / weapon_*.txt
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HudSprite {
    /// papel do sprite (`weapon`, `ammo`, `crosshair`, `number_0`…)
    pub name: String,
    /// resolução-base da entrada (320 ou 640)
    pub resolution: u32,
    /// nome do arquivo, sem pasta e sem extensão
    pub file: String,
    /// `sprites/<file>.spr`
    pub sprite_path: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct HudSprites {
    /// número na primeira linha do arquivo
    pub declared_count: Option<u32>,
    pub entries: Vec<HudSprite>,
    pub skipped: u32,
    pub warnings: Vec<String>,
}

/// Tabela de sprites do HUD. Aceita arquivo sem a linha de contagem.
pub fn parse_hud_sprites(bytes: &[u8]) -> HudSprites {
    let text = decode_text(bytes);
    let mut out = HudSprites::default();
    for (n, line) in clean_lines(&text) {
        let t = tokens(line);
        if t.len() == 1 && out.declared_count.is_none() && out.entries.is_empty() {
            if let Ok(c) = t[0].parse::<u32>() {
                out.declared_count = Some(c);
                continue;
            }
        }
        let parsed = (t.len() >= 7)
            .then(|| {
                Some(HudSprite {
                    name: t[0].clone(),
                    resolution: t[1].parse().ok()?,
                    file: t[2].clone(),
                    sprite_path: format!("sprites/{}.spr", t[2]),
                    x: t[3].parse().ok()?,
                    y: t[4].parse().ok()?,
                    width: t[5].parse().ok()?,
                    height: t[6].parse().ok()?,
                })
            })
            .flatten();
        match parsed {
            Some(e) => out.entries.push(e),
            None => {
                out.skipped += 1;
                warn(&mut out.warnings, format!("linha {n}: esperado \"objeto resolução arquivo x y largura altura\""));
            }
        }
    }
    if let Some(c) = out.declared_count {
        if c as usize != out.entries.len() {
            warn(&mut out.warnings, format!("a primeira linha declara {c} entradas, mas foram lidas {}", out.entries.len()));
        }
    }
    out
}

// ---------------------------------------------------------------------------------
// maps/<mapa>_detail.txt
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DetailTexture {
    pub texture: String,
    pub detail: String,
    /// onde o motor procura a textura de detalhe: `gfx/<detail>` (+ extensão, normalmente `.tga`)
    pub detail_path: String,
    pub x_scale: f32,
    pub y_scale: f32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DetailTextures {
    pub entries: Vec<DetailTexture>,
    /// entradas com escala <= 0 ou ilegível (o motor também as ignora)
    pub skipped: u32,
    pub warnings: Vec<String>,
}

/// `textura detalhe xScale yScale`, repetido. Lê por fichas (não por linha), como o motor.
pub fn parse_detail_textures(bytes: &[u8]) -> DetailTextures {
    let text = decode_text(bytes);
    let mut toks: Vec<String> = Vec::new();
    for (_, line) in clean_lines(&text) {
        toks.extend(tokens(line));
    }
    let mut out = DetailTextures::default();
    let mut it = toks.into_iter();
    while let Some(first) = it.next() {
        // `{` solto: o motor lê a ficha seguinte e cola
        let texture = if first == "{" { format!("{{{}", it.next().unwrap_or_default()) } else { first };
        let mut detail = it.next().unwrap_or_default();
        let mut scale = it.next();
        if scale.as_deref() == Some("{") {
            detail.push('{');
            detail.push_str(&it.next().unwrap_or_default());
            scale = it.next();
        }
        let (Some(xs), Some(ys)) = (scale, it.next()) else {
            warn(&mut out.warnings, format!("entrada \"{texture}\" incompleta no fim do arquivo"));
            out.skipped += 1;
            break;
        };
        let (x, y) = (xs.parse::<f32>().unwrap_or(0.0), ys.parse::<f32>().unwrap_or(0.0));
        if !(x > 0.0 && y > 0.0) || !x.is_finite() || !y.is_finite() {
            out.skipped += 1;
            warn(&mut out.warnings, format!("\"{texture}\": escala {xs} {ys} inválida (o motor ignora)"));
            continue;
        }
        out.entries.push(DetailTexture {
            detail_path: format!("gfx/{detail}"),
            texture,
            detail,
            x_scale: x,
            y_scale: y,
        });
    }
    out
}

// ---------------------------------------------------------------------------------
// mapcycle.txt
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MapCycleEntry {
    pub map: String,
    pub min_players: Option<u32>,
    pub max_players: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MapCycle {
    pub entries: Vec<MapCycleEntry>,
    pub warnings: Vec<String>,
}

/// Um mapa por linha, com limites opcionais de jogadores.
pub fn parse_mapcycle(bytes: &[u8]) -> MapCycle {
    let text = decode_text(bytes);
    let mut out = MapCycle::default();
    for (n, line) in clean_lines(&text) {
        let t = tokens(line);
        let Some(first) = t.first() else { continue };
        let map = first.trim_end_matches(".bsp").to_string();
        if map.is_empty() || map.contains('/') || map.contains('\\') || map.contains("..") {
            warn(&mut out.warnings, format!("linha {n}: nome de mapa suspeito ({first})"));
            continue;
        }
        let num = |i: usize| t.get(i).and_then(|s| s.parse::<u32>().ok());
        if t.len() > 3 || (t.len() > 1 && num(1).is_none()) || (t.len() > 2 && num(2).is_none()) {
            warn(&mut out.warnings, format!("linha {n}: campos extras depois do mapa foram ignorados"));
        }
        out.entries.push(MapCycleEntry { map, min_players: num(1), max_players: num(2) });
    }
    out
}

// ---------------------------------------------------------------------------------
// texto livre: briefing de mapa, motd
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
pub struct TextDoc {
    pub text: String,
    pub line_count: u32,
    pub bytes: u64,
    pub truncated: bool,
}

/// Texto de `maps/<mapa>.txt` (briefing/descrição) ou `motd.txt`, cortado em `max_chars`
/// (no máximo [`MAX_TEXT_CHARS`]).
pub fn parse_text(bytes: &[u8], max_chars: usize) -> TextDoc {
    let full = decode_text(bytes);
    let max = max_chars.min(MAX_TEXT_CHARS);
    let truncated = full.chars().count() > max;
    let text: String = if truncated { full.chars().take(max).collect() } else { full };
    TextDoc { line_count: text.lines().count() as u32, bytes: bytes.len() as u64, truncated, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liblist_com_comentarios_e_aspas() {
        let src = "// liblist\r\ngame \"Counter-Strike\"\r\nstartmap \"c0a0\" // comentario\r\ntype \"multiplayer_only\"\r\n\
                   gamedll \"dlls\\mp.dll\"\r\ngamedll_linux \"dlls/cs.so\"\r\nurl_info \"http://www.counter-strike.net\"\r\n\
                   version 1.6\r\nflag_sem_valor\r\ngame \"Segunda\"\r\n";
        let l = parse_liblist(src.as_bytes());
        assert_eq!(l.game.as_deref(), Some("Segunda")); // a última vence
        assert_eq!(l.startmap.as_deref(), Some("c0a0"));
        assert_eq!(l.game_type.as_deref(), Some("multiplayer_only"));
        assert_eq!(l.gamedll.as_deref(), Some("dlls\\mp.dll"));
        assert_eq!(l.url_info.as_deref(), Some("http://www.counter-strike.net")); // // dentro de aspas
        assert_eq!(l.version.as_deref(), Some("1.6"));
        assert!(l.entries.iter().any(|e| e.key == "flag_sem_valor" && e.value.is_empty()));
        assert_eq!(l.entries[0], KeyValue { key: "game".into(), value: "Counter-Strike".into() });
    }

    #[test]
    fn liblist_lixo() {
        let l = parse_liblist(&[0xFF, 0xFE, 0, 0, b'"', b'"', b'x', 0x80]);
        let _ = l.entries.len();
        assert!(parse_liblist(b"").entries.is_empty());
        assert!(parse_liblist(b"   \n\n// so comentario").entries.is_empty());
        assert_eq!(parse_liblist(b"game \"sem fechar").game.as_deref(), Some("sem fechar"));
    }

    const WEAPON: &str = "// ak47\n10\nweapon\t\t320\t320hud1\t0\t0\t80\t20\nweapon_s 320 320hud4 0 0 80 20\n\
                          crosshair 640 crosshairs 24 0 24 24\n\"ammo\" 640 \"640hud7\" 0 180 24 24 // fim\n";

    #[test]
    fn hud_sprites() {
        let h = parse_hud_sprites(WEAPON.as_bytes());
        assert_eq!(h.declared_count, Some(10));
        assert_eq!(h.entries.len(), 4);
        assert_eq!(
            h.entries[2],
            HudSprite {
                name: "crosshair".into(),
                resolution: 640,
                file: "crosshairs".into(),
                sprite_path: "sprites/crosshairs.spr".into(),
                x: 24,
                y: 0,
                width: 24,
                height: 24
            }
        );
        assert_eq!(h.entries[3].name, "ammo");
        assert_eq!(h.entries[3].file, "640hud7");
        // contagem declarada (10) != lidas (4)
        assert!(h.warnings.iter().any(|w| w.contains("declara 10")));
    }

    #[test]
    fn hud_tolerante() {
        let h = parse_hud_sprites(b"2\nnumber_0 640 640hud7 0 0 20 24\nlinha torta\nx 640 f a b c d\nnumber_1 640 640hud7 20 0 -3 24\n");
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.skipped, 2);
        assert_eq!(h.entries[1].width, -3);
        // sem linha de contagem
        let h = parse_hud_sprites(b"a 320 f 0 0 1 1\n");
        assert_eq!(h.declared_count, None);
        assert_eq!(h.entries.len(), 1);
        assert!(h.warnings.is_empty());
        // números gigantes não cabem em i32
        let h = parse_hud_sprites(b"a 320 f 99999999999 0 1 1\n");
        assert_eq!(h.skipped, 1);
    }

    #[test]
    fn detail_textures_como_o_motor() {
        let src = "// base detalhe x y\nc1a0_wall1 detail/det_wall 5.0 5.0\n{ fence detail/det_fence 2 2\n\
                   \"{blue\" detail/a 1 1\nzero detail/z 0 1\nnegativa detail/n -1 -1\nabc detail/q abc 2\n{grade detail/a { b 3 3\n";
        let d = parse_detail_textures(src.as_bytes());
        assert_eq!(d.entries.len(), 4);
        assert_eq!(d.entries[0].texture, "c1a0_wall1");
        assert_eq!(d.entries[0].detail_path, "gfx/detail/det_wall");
        assert_eq!((d.entries[0].x_scale, d.entries[0].y_scale), (5.0, 5.0));
        assert_eq!(d.entries[1].texture, "{fence"); // `{` solto colado
        assert_eq!(d.entries[2].texture, "{blue");
        // nome de detalhe com `{` no meio: detalhe + "{" + próxima ficha
        assert_eq!(d.entries[3].texture, "{grade");
        assert_eq!(d.entries[3].detail, "detail/a{b");
        assert_eq!(d.skipped, 3);
    }

    #[test]
    fn detail_incompleto_e_lixo() {
        let d = parse_detail_textures(b"tex detail/x 2");
        assert!(d.entries.is_empty());
        assert_eq!(d.skipped, 1);
        assert!(parse_detail_textures(b"").entries.is_empty());
        let _ = parse_detail_textures(&[0xFF, 0x00, b'{', b'{', b'"']);
        let d = parse_detail_textures(b"a b inf inf\nc d NaN 1");
        assert!(d.entries.is_empty());
    }

    #[test]
    fn mapcycle() {
        let m = parse_mapcycle(b"// ciclo\nde_dust2\r\nde_inferno 10 20\n\"cs_italy\"\nde_nuke.bsp\n../fuga\n\nde_x 5 oito\n");
        let names: Vec<&str> = m.entries.iter().map(|e| e.map.as_str()).collect();
        assert_eq!(names, ["de_dust2", "de_inferno", "cs_italy", "de_nuke", "de_x"]);
        assert_eq!(m.entries[1].min_players, Some(10));
        assert_eq!(m.entries[1].max_players, Some(20));
        assert_eq!(m.entries[4].min_players, Some(5));
        assert_eq!(m.entries[4].max_players, None);
        assert!(m.warnings.iter().any(|w| w.contains("suspeito")));
        assert!(m.warnings.iter().any(|w| w.contains("extras")));
    }

    #[test]
    fn texto_utf8_latin1_bom_e_corte() {
        assert_eq!(decode_text("\u{FEFF}ação\r\nok".as_bytes()), "ação\nok");
        assert_eq!(decode_text(&[b'a', 0xE7, 0xE3, b'o']), "a\u{e7}\u{e3}o"); // latin-1
        // corte no meio de um caractere UTF-8 no fim
        let mut cut = "açã".as_bytes().to_vec();
        cut.pop();
        assert_eq!(decode_text(&cut), "aç");
        let t = parse_text("linha1\nlinha2\nlinha3".as_bytes(), 8);
        assert!(t.truncated);
        assert_eq!(t.text, "linha1\nl");
        assert_eq!(t.line_count, 2);
        let t = parse_text(b"curto", 100);
        assert!(!t.truncated);
        assert_eq!(parse_text(&vec![b'a'; MAX_TEXT_CHARS + 10], usize::MAX).text.len(), MAX_TEXT_CHARS);
    }

    #[test]
    fn lixo_aleatorio_em_todos_os_parsers() {
        let mut seed = 11u32;
        for round in 0..400 {
            let buf: Vec<u8> = (0..(round % 300))
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    // favorece caracteres que importam para os parsers
                    const ALPHA: &[u8] = b" \n\t\"/{}0.-9a\xff";
                    ALPHA[(seed >> 24) as usize % ALPHA.len()]
                })
                .collect();
            let _ = parse_liblist(&buf);
            let _ = parse_hud_sprites(&buf);
            let _ = parse_detail_textures(&buf);
            let _ = parse_mapcycle(&buf);
            let _ = parse_text(&buf, 50);
        }
    }
}
