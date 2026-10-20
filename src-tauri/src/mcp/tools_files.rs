//! Ferramentas sobre os demais arquivos do acervo: WAD, sprite, modelo, arquivos do mod,
//! áudio, nav e demo. Também só chamam o núcleo e enxugam o resultado (sem PNG embutido,
//! sem envelopes de áudio, sem vértices).

use super::output::{page, strip_key, to_value, ToolError, ToolOutput, UNTRUSTED_NOTE};
use super::schema::{boolean, integer, object, string, string_enum};
use super::tools::{dir_arg, file_arg, Args, Config, Handler, Tool, MAX_DEMO_BYTES, MAX_MDL_BYTES, MAX_NAV_BYTES};
use super::tools_map::{notice, p_limit, p_offset, p_path};
use crate::{audio, demo, mdl, modfiles, nav, spr, wadlist};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

type R = Result<ToolOutput, ToolError>;

fn tool(name: &'static str, title: &'static str, desc: &str, schema: Value, handler: Handler) -> Tool {
    Tool { name, title, description: desc.to_string(), schema, read_only: true, handler }
}

const KINDS: [&str; 13] =
    ["map", "wad", "model", "sprite", "sound", "image", "nav", "demo", "text", "pak", "dll", "cfg", "other"];

pub(super) fn tools(_cfg: &Config) -> Vec<Tool> {
    vec![
        tool(
            "wad_list",
            "Listar texturas de um WAD",
            "Lista as entradas do diretório de um WAD2/WAD3: nome, tipo (miptex/decal/qpic/font…), dimensões, transparência, tamanho e se dá para decodificar. Paginado.",
            object(
                vec![
                    ("path", p_path("Caminho do .wad (absoluto ou relativo a uma raiz permitida).")),
                    ("query", string("Filtra por trecho do nome.")),
                    ("kind", string_enum("Filtra pelo tipo.", &["miptex", "decal", "qpic", "font", "console", "palette", "other"])),
                    ("offset", p_offset()),
                    ("limit", p_limit(200, 2000)),
                ],
                &["path"],
            ),
            wad_list,
        ),
        tool(
            "wad_texture",
            "Textura de um WAD (PNG)",
            "Decodifica uma textura do WAD pelo nome e devolve o mip 0 como imagem PNG, mais nome, tipo e dimensões.",
            object(
                vec![
                    ("path", p_path("Caminho do .wad.")),
                    ("name", string("Nome da textura (sem diferenciar maiúsculas).")),
                ],
                &["path", "name"],
            ),
            wad_texture,
        ),
        tool(
            "wad_usage",
            "Uso de um WAD pelos mapas",
            "Quais mapas de uma pasta usam cada textura de um WAD (pelo índice de nomes do BSP; não abre o mapa inteiro) e quantas texturas ninguém usa. Paginado.",
            object(
                vec![
                    ("wad_path", p_path("Caminho do .wad.")),
                    ("maps_dir", p_path("Pasta de mapas (recursivo).")),
                    ("only_unused", boolean("Só as texturas que nenhum mapa usa.", false)),
                    ("only_used", boolean("Só as texturas usadas por pelo menos um mapa.", false)),
                    ("offset", p_offset()),
                    ("limit", p_limit(200, 2000)),
                ],
                &["wad_path", "maps_dir"],
            ),
            wad_usage,
        ),
        tool(
            "sprite_info",
            "Informações de sprite",
            "Cabeçalho e quadros de um .spr: tipo, formato de textura, raio, tamanho máximo, sincronia, duração e, por quadro, origem e dimensões. Com `frame` devolve também a imagem PNG do quadro.",
            object(
                vec![
                    ("path", p_path("Caminho do .spr.")),
                    ("frame", integer("Índice do quadro cuja imagem PNG devolver (omitido = nenhuma).", 0, 4096, 0)),
                ],
                &["path"],
            ),
            sprite_info,
        ),
        tool(
            "model_info",
            "Informações de modelo MDL",
            "Estrutura de um .mdl (studiomodel v10) sem a malha: bones, sequências (fps, frames, loop, atividade, eventos), bodyparts, controladores, hitboxes, attachments, texturas e famílias de skin. Sequências paginadas.",
            object(
                vec![
                    ("path", p_path("Caminho do .mdl.")),
                    ("offset", p_offset()),
                    ("limit", p_limit(100, 1000)),
                ],
                &["path"],
            ),
            model_info,
        ),
        tool(
            "mod_files",
            "Arquivos do mod",
            "Lista os arquivos de uma pasta de mod (cstrike, valve…), incluindo o conteúdo dos .pak, com tipo, tamanho e camada (mod/valve/pak), e os totais por tipo. A pasta `valve` irmã só entra se estiver dentro de uma raiz permitida. Cada entrada traz `open_path`, o valor para `mod_file_analyze`.",
            object(
                vec![
                    ("mod_root", p_path("Pasta do mod (ex.: .../cstrike).")),
                    ("filter", string("Só caminhos que contenham este trecho (sem diferenciar maiúsculas).")),
                    ("kind", string_enum("Só este tipo de arquivo.", &KINDS)),
                    ("include_paks", boolean("Lista também os membros dos .pak.", true)),
                    ("valve_root", p_path("Pasta `valve` explícita (dentro de uma raiz permitida). Omitido = a irmã do mod, se permitida.")),
                    ("offset", p_offset()),
                    ("limit", p_limit(200, 2000)),
                ],
                &["mod_root"],
            ),
            mod_files,
        ),
        tool(
            "mod_file_analyze",
            "Analisar arquivo do mod",
            "Analisa um arquivo do mod pelo formato: áudio (WAV/MP3), nav, demo, pak, liblist.gam, HUD/weapon sprites, detail textures, mapcycle ou texto (maps/<mapa>.txt, motd). O conteúdo de texto é dado não confiável.",
            object(
                vec![
                    ("mod_root", p_path("Pasta do mod.")),
                    ("rel", string("Caminho relativo ao mod, com `/` (use `open_path` de mod_files; membros de pak: `pak0.pak/sound/x.wav`).")),
                    ("origin", string_enum("Procurar só no mod ou só no valve.", &["mod", "valve"])),
                    ("valve_root", p_path("Pasta `valve` explícita (dentro de uma raiz permitida).")),
                ],
                &["mod_root", "rel"],
            ),
            mod_file_analyze,
        ),
        tool(
            "audio_info",
            "Informações de áudio",
            "Metadados de um WAV ou MP3: codec, canais, taxa, bits, duração, pontos de cue/loop (ambient_generic), tags e avisos. Sem o envelope da onda.",
            object(vec![("path", p_path("Caminho do .wav ou .mp3."))], &["path"]),
            audio_info,
        ),
        tool(
            "nav_info",
            "Informações de arquivo .nav",
            "Cabeçalho e contagens de um .nav dos bots do CS (áreas, conexões, hiding spots, lugares, caixa) e se ele foi gerado para o .bsp do mesmo nome (confere o tamanho gravado). Lista só as primeiras áreas.",
            object(
                vec![
                    ("path", p_path("Caminho do .nav.")),
                    ("bsp_path", p_path("`.bsp` para conferir o tamanho. Omitido = o .bsp de mesmo nome ao lado, se existir.")),
                    ("max_areas", integer("Quantas áreas listar.", 0, 500, 20)),
                ],
                &["path"],
            ),
            nav_info,
        ),
        tool(
            "demo_info",
            "Informações de demo",
            "Cabeçalho e diretório de uma demo .dem do GoldSrc: protocolos, mapa, pasta do jogo, CRC do mapa, segmentos, duração e quadros.",
            object(vec![("path", p_path("Caminho do .dem."))], &["path"]),
            demo_info,
        ),
    ]
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// `data:image/png;base64,...` → bytes do PNG.
fn png_of(data_url: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(data_url.strip_prefix("data:image/png;base64,")?).ok()
}

fn wad_list(cfg: &Config, a: &Args) -> R {
    let p = file_arg(cfg, a, "path", &["wad"], wadlist::MAX_WAD_BYTES)?;
    let l = wadlist::list_wad(&p)?;
    let query = a.str("query").map(str::to_lowercase);
    let kind = a.str("kind");
    let rows: Vec<Value> = l
        .entries
        .iter()
        .filter(|e| query.as_ref().map_or(true, |q| e.name.to_lowercase().contains(q)))
        .filter(|e| kind.map_or(true, |k| serde_json::to_value(e.kind).ok().and_then(|v| v.as_str().map(|s| s == k)).unwrap_or(false)))
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    let (win, mut meta) = page(&rows, a.usize_or("offset", 0), a.usize_or("limit", 200));
    meta.insert("name".into(), json!(l.name));
    meta.insert("file_size".into(), json!(l.file_size));
    meta.insert("magic".into(), json!(l.magic));
    meta.insert("lump_count".into(), json!(l.lump_count));
    meta.insert("texture_count".into(), json!(l.texture_count));
    meta.insert("entries".into(), Value::Array(win));
    Ok(ToolOutput::json(Value::Object(meta)))
}

fn wad_texture(cfg: &Config, a: &Args) -> R {
    let p = file_arg(cfg, a, "path", &["wad"], wadlist::MAX_WAD_BYTES)?;
    let name = a.req_str("name")?;
    let t = wadlist::wad_texture(&p, name, false)?;
    let png = png_of(&t.png).ok_or("a textura não gerou um PNG válido")?;
    Ok(ToolOutput::json(json!({
        "wad": name_of(&p),
        "name": t.name,
        "kind": t.kind,
        "width": t.width,
        "height": t.height,
        "transparent": t.transparent,
    }))
    .with_image(&png))
}

fn wad_usage(cfg: &Config, a: &Args) -> R {
    let wad = file_arg(cfg, a, "wad_path", &["wad"], wadlist::MAX_WAD_BYTES)?;
    let dir = dir_arg(cfg, a, "maps_dir")?;
    let u = wadlist::wad_usage(&wad, &dir)?;
    let (only_unused, only_used) = (a.bool_or("only_unused", false), a.bool_or("only_used", false));
    let rows: Vec<Value> = u
        .textures
        .iter()
        .filter(|t| !(only_unused && t.map_count > 0) && !(only_used && t.map_count == 0))
        .filter_map(|t| serde_json::to_value(t).ok())
        .collect();
    let (win, mut meta) = page(&rows, a.usize_or("offset", 0), a.usize_or("limit", 200));
    meta.insert("wad".into(), json!(name_of(&wad)));
    meta.insert("maps_dir".into(), json!(cfg.roots.show(&dir)));
    meta.insert("maps_scanned".into(), json!(u.maps_scanned));
    meta.insert("maps_unreadable".into(), json!(u.maps_unreadable));
    meta.insert("maps_truncated".into(), json!(u.truncated));
    meta.insert("unused_count".into(), json!(u.unused_count));
    meta.insert("texture_total".into(), json!(u.textures.len()));
    meta.insert("textures".into(), Value::Array(win));
    Ok(ToolOutput::json(Value::Object(meta)))
}

fn sprite_info(cfg: &Config, a: &Args) -> R {
    let p = file_arg(cfg, a, "path", &["spr"], spr::MAX_FILE_BYTES)?;
    let info = spr::load_sprite(&p)?;
    let image = a
        .opt_usize("frame")
        .map(|i| {
            info.frames
                .get(i)
                .and_then(|f| f.images.first())
                .and_then(|im| png_of(&im.png))
                .ok_or_else(|| ToolError::Failed(format!("o sprite não tem o quadro {i} (quadros: {})", info.frames.len())))
        })
        .transpose()?;
    let mut v = to_value(&info)?;
    strip_key(&mut v, "png");
    if let Value::Object(m) = &mut v {
        m.insert("name".into(), json!(name_of(&p)));
    }
    let out = ToolOutput::json(v);
    Ok(match image {
        Some(png) => out.with_image(&png),
        None => out,
    })
}

fn merge(into: &mut Value, from: Value) {
    if let (Value::Object(a), Value::Object(b)) = (into, from) {
        for (k, v) in b {
            a.entry(k).or_insert(v);
        }
    }
}

fn model_info(cfg: &Config, a: &Args) -> R {
    let p = file_arg(cfg, a, "path", &["mdl"], MAX_MDL_BYTES)?;
    let m = mdl::files::parse_file(&p, &mdl::ParseOptions::default())?;
    let seqs: Vec<Value> = m
        .seq_info
        .iter()
        .enumerate()
        .map(|(i, info)| {
            let mut v = serde_json::to_value(info).unwrap_or(Value::Null);
            if let Value::Object(o) = &mut v {
                o.insert("index".into(), json!(i));
            }
            if let Some(d) = m.extra.seq_details.get(i) {
                merge(&mut v, serde_json::to_value(d).unwrap_or(Value::Null));
            }
            v
        })
        .collect();
    let (win, mut meta) = page(&seqs, a.usize_or("offset", 0), a.usize_or("limit", 100));
    let x = &m.extra;
    meta.insert("name".into(), json!(name_of(&p)));
    meta.insert("file_size".into(), json!(std::fs::metadata(&p).map(|md| md.len()).unwrap_or(0)));
    meta.insert("bones".into(), json!(m.num_bones));
    meta.insert("triangles".into(), json!(m.positions.len() / 9));
    meta.insert("textures".into(), json!(m.textures.iter().map(|t| json!({"name": t.name, "width": t.width, "height": t.height})).collect::<Vec<_>>()));
    meta.insert("skin_families".into(), json!(m.skin_families.len()));
    meta.insert("external_textures".into(), json!(x.external_textures));
    meta.insert("bone_info".into(), to_value(&x.bone_info)?);
    meta.insert("bodyparts".into(), to_value(&x.bodyparts)?);
    meta.insert("controllers".into(), to_value(&x.controllers)?);
    meta.insert("texture_info".into(), to_value(&x.texture_info)?);
    meta.insert("hitboxes".into(), to_value(&x.hitboxes)?);
    meta.insert("attachments".into(), to_value(&x.attachments)?);
    meta.insert("warnings".into(), to_value(&x.warnings)?);
    meta.insert("sequences".into(), Value::Array(win));
    // `total/offset/limit/...` acima descrevem as sequências
    meta.insert("sequences_total".into(), meta.get("total").cloned().unwrap_or(Value::Null));
    Ok(ToolOutput::json(Value::Object(meta)))
}

/// Pasta `valve`: a pedida, a irmã do mod se estiver nas raízes permitidas, ou nenhuma
/// (passar a própria pasta do mod desliga a segunda camada em `ModRoots`).
fn valve_for(cfg: &Config, mod_root: &Path, a: &Args) -> Result<PathBuf, ToolError> {
    if let Some(v) = a.str("valve_root") {
        return Ok(cfg.roots.resolve_dir(v)?);
    }
    if let Some(sib) = mod_root.parent().map(|p| p.join("valve")).and_then(|p| p.canonicalize().ok()) {
        if sib.is_dir() && cfg.roots.contains(&sib) {
            return Ok(sib);
        }
    }
    Ok(mod_root.to_path_buf())
}

fn origin_of(a: &Args) -> Option<modfiles::Root> {
    match a.str("origin") {
        Some("mod") => Some(modfiles::Root::Mod),
        Some("valve") => Some(modfiles::Root::Valve),
        _ => None,
    }
}

fn mod_files(cfg: &Config, a: &Args) -> R {
    let root = dir_arg(cfg, a, "mod_root")?;
    let valve = valve_for(cfg, &root, a)?;
    let opts = modfiles::ListOptions { include_paks: a.bool_or("include_paks", true), max_entries: 100_000, ..Default::default() };
    let l = modfiles::list_mod_files(&root, Some(&valve), &opts)?;
    let filter = a.str("filter").map(str::to_lowercase);
    let kind = a.str("kind");
    let rows: Vec<Value> = l
        .entries
        .iter()
        .filter(|e| filter.as_ref().map_or(true, |f| e.path.to_lowercase().contains(f)))
        .filter(|e| kind.map_or(true, |k| serde_json::to_value(e.kind).ok().and_then(|v| v.as_str().map(|s| s == k)).unwrap_or(false)))
        .filter_map(|e| {
            let mut v = serde_json::to_value(e).ok()?;
            if let Value::Object(m) = &mut v {
                m.insert("open_path".into(), json!(e.open_path()));
            }
            Some(v)
        })
        .collect();
    let (win, mut meta) = page(&rows, a.usize_or("offset", 0), a.usize_or("limit", 200));
    meta.insert("mod_root".into(), json!(cfg.roots.show(&root)));
    meta.insert("valve_root".into(), json!(l.valve_root));
    meta.insert("totals".into(), to_value(&l.totals)?);
    meta.insert("listing_truncated".into(), json!(l.truncated));
    meta.insert("warnings".into(), json!(l.warnings));
    meta.insert("entries".into(), Value::Array(win));
    meta.insert("data_notice".into(), json!(UNTRUSTED_NOTE));
    Ok(ToolOutput::json(Value::Object(meta)))
}

/// Tira o que pesa e não ajuda um agente: envelope da onda e a lista de áreas do nav.
fn trim_analysis(v: &mut Value, max_areas: usize) {
    strip_key(v, "envelope");
    trim_areas(v, max_areas);
}

fn trim_areas(v: &mut Value, max_areas: usize) {
    match v {
        Value::Object(m) => {
            if let Some(Value::Array(areas)) = m.get_mut("areas") {
                let total = areas.len();
                areas.truncate(max_areas);
                m.insert("areas_total_listed".into(), json!(total));
                m.insert("areas_shown".into(), json!(total.min(max_areas)));
            }
            for c in m.values_mut() {
                trim_areas(c, max_areas);
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(|c| trim_areas(c, max_areas)),
        _ => {}
    }
}

fn mod_file_analyze(cfg: &Config, a: &Args) -> R {
    let root = dir_arg(cfg, a, "mod_root")?;
    let valve = valve_for(cfg, &root, a)?;
    let roots = modfiles::ModRoots::new(&root, Some(&valve))?;
    let an = roots.analyze(a.req_str("rel")?, origin_of(a))?;
    let mut v = to_value(&an)?;
    trim_analysis(&mut v, 20);
    Ok(ToolOutput::json(notice(v)))
}

fn audio_info(cfg: &Config, a: &Args) -> R {
    let p = file_arg(cfg, a, "path", &["wav", "mp3"], audio::MAX_AUDIO_BYTES)?;
    let info = audio::analyze_audio_file(&p, 0)?;
    let mut v = to_value(&info)?;
    strip_key(&mut v, "envelope");
    if let Value::Object(m) = &mut v {
        m.insert("name".into(), json!(name_of(&p)));
    }
    Ok(ToolOutput::json(notice(v)))
}

fn nav_info(cfg: &Config, a: &Args) -> R {
    let p = file_arg(cfg, a, "path", &["nav"], MAX_NAV_BYTES)?;
    let bytes = std::fs::read(&p).map_err(|e| format!("não leu {}: {e}", p.display()))?;
    let mut info = nav::parse_nav(&bytes)?;
    // o .bsp de mesmo nome (ou o pedido) precisa estar numa raiz permitida para ser conferido
    let bsp = match a.str("bsp_path") {
        Some(b) => Some(cfg.roots.resolve_file(b)?),
        None => cfg.roots.resolve_file(&p.with_extension("bsp").to_string_lossy()).ok(),
    };
    if let Some(b) = &bsp {
        if let Ok(md) = std::fs::metadata(b) {
            nav::check_bsp_size(&mut info, md.len());
        }
    }
    let mut v = to_value(&info)?;
    trim_areas(&mut v, a.usize_or("max_areas", 20));
    if let Value::Object(m) = &mut v {
        m.insert("name".into(), json!(name_of(&p)));
        m.insert("bsp_checked".into(), json!(bsp.as_ref().map(|b| name_of(b))));
    }
    Ok(ToolOutput::json(notice(v)))
}

fn demo_info(cfg: &Config, a: &Args) -> R {
    let p = file_arg(cfg, a, "path", &["dem"], MAX_DEMO_BYTES)?;
    let info = demo::parse_demo_file(&p)?;
    let mut v = to_value(&info)?;
    if let Value::Object(m) = &mut v {
        m.insert("name".into(), json!(name_of(&p)));
    }
    Ok(ToolOutput::json(notice(v)))
}
