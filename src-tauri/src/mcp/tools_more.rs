//! Mais ferramentas de "ver e investigar": folha de texturas do mapa, céu, lightmap, entidades
//! por vizinhança, uso de texturas, vistas de modelo (4 ângulos e quadros de uma animação),
//! folha de quadros de sprite e forma de onda de áudio. Tudo reaproveita o núcleo.

use super::output::{page, ToolError, ToolOutput};
use super::schema::{integer, number, object, string};
use super::tools::{confined_bsp_files, dir_arg, file_arg, load_bsp, map_name, Args, Config, Handler, Tool, MAX_BSP_BYTES, MAX_MDL_BYTES};
use super::tools_map::{p_limit, p_offset, p_path};
use super::tools_view::{blit_fit, decode_textures, png_bytes, prepare_model, render_model};
use crate::render3d::{self, Tex};
use crate::{audio, bsp, catalog, entity_list, spr, wadlist};
use serde_json::{json, Value};

type R = Result<ToolOutput, ToolError>;

fn tool(name: &'static str, title: &'static str, desc: &str, schema: Value, handler: Handler) -> Tool {
    Tool { name, title, description: desc.to_string(), schema, read_only: true, handler }
}

pub(super) fn tools(_cfg: &Config) -> Vec<Tool> {
    let cell = || integer("Lado de cada célula em pixels.", 32, 256, 96);
    let cols = |d: i64| integer("Colunas da grade.", 1, 16, d);
    vec![
        tool(
            "map_texture_sheet",
            "Folha das texturas de um mapa",
            "Folha de contato PNG com as texturas que o mapa usa (as embutidas no .bsp e as dos WADs achados no disco), com `cells` dizendo o nome de cada célula e se foi desenhada (`drawn: false` = textura de WAD que não está no disco). Ótima para entender o visual do mapa e achar texturas ausentes. Paginado.",
            object(
                vec![("path", p_path("Caminho do .bsp.")), ("query", string("Só texturas cujo nome contém este trecho.")), ("offset", p_offset()), ("limit", p_limit(36, 100)), ("cell", cell()), ("columns", cols(6))],
                &["path"],
            ),
            map_texture_sheet,
        ),
        tool(
            "map_skybox",
            "Céu do mapa (planificação)",
            "Monta a planificação em cruz dos 6 lados do céu do mapa (`skyname` do worldspawn, lido de gfx/env do mod ou do valve) como imagem PNG e diz quantos lados foram achados. Sem céu achado, devolve só o aviso.",
            object(vec![("path", p_path("Caminho do .bsp.")), ("cell", integer("Lado de cada face em pixels.", 64, 384, 192))], &["path"]),
            map_skybox,
        ),
        tool(
            "map_lightmap",
            "Atlas de lightmaps do mapa",
            "Devolve o atlas de lightmaps do mapa como imagem PNG (a luz que multiplica as texturas), com as dimensões e quantas faces têm luz e quantas camadas de estilo animado existem. Mapa fullbright não tem atlas.",
            object(vec![("path", p_path("Caminho do .bsp."))], &["path"]),
            map_lightmap,
        ),
        tool(
            "map_nearby",
            "Entidades perto de um ponto",
            "Entidades do mapa dentro de um raio de um ponto (x, y, z), da mais perto para a mais longe, com a distância. Use depois de `map_view` para saber o que há em volta de uma posição. Os valores são texto do mapa: dados não confiáveis.",
            object(
                vec![
                    ("path", p_path("Caminho do .bsp.")),
                    ("x", number("X do ponto.", -1e6, 1e6)),
                    ("y", number("Y do ponto.", -1e6, 1e6)),
                    ("z", number("Z do ponto.", -1e6, 1e6)),
                    ("radius", number("Raio de busca em unidades.", 1.0, 1e6)),
                    ("classname", string("Classname exato ou prefixo com `*` no fim.")),
                    ("limit", p_limit(30, 200)),
                ],
                &["path", "x", "y", "z"],
            ),
            map_nearby,
        ),
        tool(
            "texture_usage",
            "Quais mapas usam uma textura",
            "Procura uma textura pelo nome nos mapas de uma pasta (só lê o índice de nomes de cada .bsp) e lista os mapas que a usam. Com `exact` falso aceita um trecho do nome (por exemplo `brick`).",
            object(
                vec![
                    ("maps_dir", p_path("Pasta de mapas (recursivo).")),
                    ("texture", string("Nome da textura ou trecho dele.")),
                    ("exact", super::schema::boolean("Nome igual (sem diferenciar maiúsculas) em vez de trecho.", false)),
                    ("limit", p_limit(100, 500)),
                ],
                &["maps_dir", "texture"],
            ),
            texture_usage,
        ),
        tool(
            "model_turnaround",
            "Modelo em 4 ângulos",
            "Imagem PNG única com o modelo visto de 4 lados (frente, lado direito, costas, lado esquerdo) na mesma escala, com skin, bodygroup e, opcionalmente, um quadro de sequência. Bom para conferir uma skin por inteiro de uma vez.",
            object(
                vec![
                    ("path", p_path("Caminho do .mdl.")),
                    ("skin", integer("Família de skin.", 0, 255, 0)),
                    ("body", integer("Valor de bodygroup.", 0, 100000, 0)),
                    ("sequence", integer("Sequência a posar.", 0, 4096, 0)),
                    ("frame", integer("Quadro da sequência.", 0, 100000, 0)),
                    ("size", integer("Lado de cada vista em pixels.", 64, 600, 320)),
                ],
                &["path"],
            ),
            model_turnaround,
        ),
        tool(
            "model_filmstrip",
            "Quadros de uma animação do modelo",
            "Tira de quadros PNG de uma sequência do modelo (quadros igualmente espaçados, mesmo enquadramento), para ver o movimento sem tocar a animação. `model_info` lista as sequências.",
            object(
                vec![
                    ("path", p_path("Caminho do .mdl.")),
                    ("sequence", integer("Índice da sequência.", 0, 4096, 0)),
                    ("frames", integer("Quantos quadros mostrar.", 2, 16, 8)),
                    ("skin", integer("Família de skin.", 0, 255, 0)),
                    ("yaw", number("Giro da câmera em graus.", -360.0, 360.0)),
                    ("size", integer("Lado de cada quadro em pixels.", 64, 400, 220)),
                    ("columns", cols(4)),
                ],
                &["path"],
            ),
            model_filmstrip,
        ),
        tool(
            "sprite_sheet",
            "Quadros de um sprite",
            "Folha de contato PNG com todos os quadros de um `.spr` (sobre xadrez, para ver a transparência), com `cells` (quadro, tamanho, origem, duração).",
            object(vec![("path", p_path("Caminho do .spr.")), ("cell", cell()), ("columns", cols(8))], &["path"]),
            sprite_sheet,
        ),
        tool(
            "audio_waveform",
            "Forma de onda de um áudio",
            "Desenha a forma de onda de um WAV como imagem PNG, com a região de loop destacada (cue/smpl), e devolve duração, taxa, canais e pico. MP3 e ADPCM não têm envelope: só os metadados.",
            object(vec![("path", p_path("Caminho do .wav ou .mp3.")), ("width", integer("Largura em pixels.", 200, 1600, 800)), ("height", integer("Altura em pixels.", 60, 400, 200))], &["path"]),
            audio_waveform,
        ),
    ]
}

/// Grade de células `cell` x `cell` (xadrez ao fundo) com as texturas, na ordem.
fn sheet(items: &[Option<Tex>], cell: usize, cols: usize) -> (usize, usize, Vec<u8>) {
    let cols = cols.clamp(1, 16).min(items.len().max(1));
    let rows = items.len().max(1).div_ceil(cols);
    let gap = 4;
    let (w, h) = (cols * (cell + gap) + gap, rows * (cell + gap) + gap);
    let mut buf = vec![0u8; w * h * 4];
    for (i, px) in buf.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i % w, i / w);
        let c = if (x / 8 + y / 8) % 2 == 0 { 44 } else { 56 };
        px.copy_from_slice(&[c, c, c + 4, 255]);
    }
    for (i, t) in items.iter().enumerate() {
        if let Some(t) = t {
            blit_fit(&mut buf, w, gap + (i % cols) * (cell + gap), gap + (i / cols) * (cell + gap), cell, t);
        }
    }
    (w, h, buf)
}

fn map_texture_sheet(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let mesh = catalog::mesh(&path)?;
    let q = a.str("query").map(str::to_ascii_lowercase);
    let all: Vec<(usize, &catalog::MeshTexture)> =
        mesh.textures.iter().enumerate().filter(|(_, t)| q.as_ref().map_or(true, |q| t.name.to_ascii_lowercase().contains(q))).collect();
    let (win, mut meta) = page(&all, a.usize_or("offset", 0), a.usize_or("limit", 36));
    if win.is_empty() {
        return Err("nenhuma textura nesta janela".into());
    }
    let decoded = decode_textures(win.iter().map(|(_, t)| (t.name.clone(), t.png.clone())));
    let cell = a.usize_or("cell", 96).clamp(32, 256);
    let cols = a.usize_or("columns", 6);
    let (w, h, rgba) = sheet(&decoded, cell, cols);
    let ncols = cols.clamp(1, 16).min(win.len());
    let offset = meta.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let cells: Vec<Value> = win
        .iter()
        .enumerate()
        .map(|(i, (ix, t))| json!({"index": offset + i, "texture_slot": ix, "name": t.name, "row": i / ncols, "col": i % ncols, "drawn": decoded[i].is_some(), "size": decoded[i].as_ref().map(|d| [d.w, d.h])}))
        .collect();
    meta.insert("map".into(), json!(map_name(&path)));
    meta.insert("textures_total".into(), json!(mesh.textures.len()));
    meta.insert("from_wad".into(), json!(mesh.wad_textures));
    meta.insert("image_size".into(), json!([w, h]));
    meta.insert("cells".into(), Value::Array(cells));
    let png = png_bytes(w, h, &rgba)?;
    Ok(ToolOutput::json(Value::Object(meta)).with_image(&png))
}

fn map_skybox(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let (_, parsed) = load_bsp(&path)?;
    let ents = bsp::entities::summarize(&parsed);
    let sky = ents.sky.clone().unwrap_or_default();
    if sky.trim().is_empty() {
        return Ok(ToolOutput::json(json!({"map": map_name(&path), "skyname": null, "found": 0, "note": "o mapa não declara skyname"})));
    }
    let faces = bsp::sky::load_sky(&path, &sky);
    let names = ["up", "dn", "lf", "rt", "ft", "bk"];
    let Some(faces) = faces else {
        return Ok(ToolOutput::json(json!({"map": map_name(&path), "skyname": sky, "found": 0, "note": "nome de céu inválido ou gfx/env não encontrado"})));
    };
    let tex: Vec<Option<Tex>> = faces.iter().map(|f| f.as_deref().and_then(render3d::decode_png_url)).collect();
    let found = tex.iter().flatten().count();
    let cell = a.usize_or("cell", 192).clamp(64, 384);
    // cruz: linha 0 = up; linha 1 = lf ft rt bk; linha 2 = dn
    let gap = 4;
    let (w, h) = (4 * (cell + gap) + gap, 3 * (cell + gap) + gap);
    let mut buf = vec![0u8; w * h * 4];
    for px in buf.chunks_exact_mut(4) {
        px.copy_from_slice(&[24, 28, 36, 255]);
    }
    // índices: 0 up, 1 dn, 2 lf, 3 rt, 4 ft, 5 bk
    for (face, col, row) in [(0usize, 1usize, 0usize), (1, 1, 2), (2, 0, 1), (4, 1, 1), (3, 2, 1), (5, 3, 1)] {
        if let Some(Some(t)) = tex.get(face) {
            blit_fit(&mut buf, w, gap + col * (cell + gap), gap + row * (cell + gap), cell, t);
        }
    }
    let png = png_bytes(w, h, &buf)?;
    let sides: Vec<Value> = names.iter().zip(&tex).map(|(n, t)| json!({"side": n, "found": t.is_some(), "size": t.as_ref().map(|t| [t.w, t.h])})).collect();
    Ok(ToolOutput::json(json!({"map": map_name(&path), "skyname": sky, "found": found, "of": 6, "sides": sides, "layout": "cruz: up em cima, dn embaixo, lf ft rt bk na linha do meio"})).with_image(&png))
}

fn map_lightmap(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let (b, _) = load_bsp(&path)?;
    let atlas = bsp::light::build_atlas(&b).ok_or("mapa sem lightmap (fullbright): não há atlas")?;
    let with_light = atlas.faces.iter().flatten().count();
    let png = png_bytes(atlas.width, atlas.height, &atlas.rgba)?;
    Ok(ToolOutput::json(json!({
        "map": map_name(&path),
        "atlas": [atlas.width, atlas.height],
        "faces": atlas.faces.len(),
        "faces_with_light": with_light,
        "style_layers": atlas.layers.iter().map(|l| l.slot).collect::<Vec<_>>(),
    }))
    .with_image(&png))
}

fn map_nearby(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let need = |k: &str| a.opt_f32(k).ok_or_else(|| ToolError::InvalidParams(format!("argumento obrigatório ausente: {k}")));
    let p = [need("x")?, need("y")?, need("z")?];
    let radius = a.opt_f32("radius").unwrap_or(512.0);
    let want = a.str("classname");
    let (b, parsed) = load_bsp(&path)?;
    let mut rows: Vec<(f32, entity_list::EntityRow)> = entity_list::list(&b, &parsed)
        .into_iter()
        .filter(|r| match want {
            None => true,
            Some(w) => w.strip_suffix('*').map_or(r.classname == w, |pre| r.classname.starts_with(pre)),
        })
        .filter_map(|r| {
            let o = r.origin?;
            let d = ((o[0] - p[0]).powi(2) + (o[1] - p[1]).powi(2) + (o[2] - p[2]).powi(2)).sqrt();
            (d <= radius).then_some((d, r))
        })
        .collect();
    rows.sort_by(|x, y| x.0.total_cmp(&y.0));
    let items: Vec<Value> = rows
        .iter()
        .map(|(d, r)| json!({"distance": (d * 10.0).round() / 10.0, "index": r.index, "classname": r.classname, "targetname": r.targetname, "origin": r.origin, "model": r.model, "keys": r.keys}))
        .collect();
    let (win, mut meta) = page(&items, 0, a.usize_or("limit", 30));
    meta.insert("map".into(), json!(map_name(&path)));
    meta.insert("center".into(), json!(p));
    meta.insert("radius".into(), json!(radius));
    meta.insert("entities".into(), Value::Array(win));
    meta.insert("data_notice".into(), json!(super::output::UNTRUSTED_NOTE));
    Ok(ToolOutput::json(Value::Object(meta)))
}

fn texture_usage(cfg: &Config, a: &Args) -> R {
    let dir = dir_arg(cfg, a, "maps_dir")?;
    let q = a.req_str("texture")?.to_ascii_lowercase();
    let exact = a.bool_or("exact", false);
    let (files, skipped) = confined_bsp_files(cfg, &dir);
    const MAX_MAPS: usize = 2000;
    let mut hits: Vec<Value> = Vec::new();
    let (mut scanned, mut unreadable) = (0usize, 0usize);
    for f in files.iter().take(MAX_MAPS) {
        scanned += 1;
        match wadlist::bsp_texture_names(f) {
            Some(names) => {
                let m: Vec<&String> = names.iter().filter(|n| if exact { **n == q } else { n.contains(&q) }).collect();
                if !m.is_empty() {
                    hits.push(json!({"map": map_name(f), "path": cfg.roots.show(f), "textures": m}));
                }
            }
            None => unreadable += 1,
        }
    }
    let (win, mut meta) = page(&hits, 0, a.usize_or("limit", 100));
    meta.insert("texture".into(), json!(q));
    meta.insert("maps_scanned".into(), json!(scanned));
    meta.insert("maps_unreadable".into(), json!(unreadable));
    meta.insert("maps_truncated".into(), json!(files.len() > MAX_MAPS));
    meta.insert("skipped_outside_roots".into(), json!(skipped));
    meta.insert("maps".into(), Value::Array(win));
    Ok(ToolOutput::json(Value::Object(meta)))
}

fn compose_grid(views: &[Vec<u8>], side: usize, cols: usize) -> (usize, usize, Vec<u8>) {
    let cols = cols.clamp(1, 16).min(views.len().max(1));
    let rows = views.len().max(1).div_ceil(cols);
    let (w, h) = (cols * side, rows * side);
    let mut buf = vec![0u8; w * h * 4];
    for (i, v) in views.iter().enumerate() {
        let (ox, oy) = ((i % cols) * side, (i / cols) * side);
        for y in 0..side {
            let src = y * side * 4;
            let dst = ((oy + y) * w + ox) * 4;
            buf[dst..dst + side * 4].copy_from_slice(&v[src..src + side * 4]);
        }
    }
    (w, h, buf)
}

fn model_turnaround(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["mdl"], MAX_MDL_BYTES)?;
    let seq = a.opt_usize("sequence").map(|s| (s, a.usize_or("frame", 0)));
    let scene = prepare_model(&path, a.opt_usize("body").map(|v| v as u32), a.opt_usize("skin").map(|v| v as u32), None, None, seq)?;
    let side = a.usize_or("size", 320).clamp(64, 600);
    // enquadramento único (esfera da caixa) para as 4 vistas ficarem na mesma escala
    let views_def = [("frente", 180.0f32), ("lado direito", 90.0), ("costas", 0.0), ("lado esquerdo", -90.0)];
    let views: Vec<Vec<u8>> = views_def.iter().map(|(_, yaw)| render_model(&scene, *yaw, -8.0, side, None)).collect();
    let (w, h, rgba) = compose_grid(&views, side, 2);
    let png = png_bytes(w, h, &rgba)?;
    Ok(ToolOutput::json(json!({
        "model": path.file_name().map(|s| s.to_string_lossy().to_string()),
        "views": views_def.iter().enumerate().map(|(i, (n, y))| json!({"name": n, "yaw": y, "row": i / 2, "col": i % 2})).collect::<Vec<_>>(),
        "image_size": [w, h],
        "bounds": {"mins": scene.mins, "maxs": scene.maxs},
        "note": "yaw 0 olha para +X do modelo; a 'frente' é um palpite (yaw 180): confira pelo modelo",
    }))
    .with_image(&png))
}

fn model_filmstrip(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["mdl"], MAX_MDL_BYTES)?;
    let seq = a.usize_or("sequence", 0);
    let want = a.usize_or("frames", 8).clamp(2, 16);
    let total = catalog::load_sequence(&path, seq)?.frames as usize;
    if total == 0 {
        return Err("a sequência não tem quadros".into());
    }
    let n = want.min(total);
    let picks: Vec<usize> = (0..n).map(|i| if n == 1 { 0 } else { i * (total - 1) / (n - 1) }).collect();
    let skin = a.opt_usize("skin").map(|v| v as u32);
    let scenes: Vec<_> = picks.iter().map(|f| prepare_model(&path, None, skin, None, None, Some((seq, *f)))).collect::<Result<_, _>>()?;
    // enquadramento comum: união das caixas de todos os quadros
    let (mut mins, mut maxs) = ([f32::MAX; 3], [f32::MIN; 3]);
    for s in &scenes {
        for k in 0..3 {
            mins[k] = mins[k].min(s.mins[k]);
            maxs[k] = maxs[k].max(s.maxs[k]);
        }
    }
    let side = a.usize_or("size", 220).clamp(64, 400);
    let yaw = a.opt_f32("yaw").unwrap_or(135.0);
    let views: Vec<Vec<u8>> = scenes.iter().map(|s| render_model(s, yaw, -12.0, side, Some((mins, maxs)))).collect();
    let (w, h, rgba) = compose_grid(&views, side, a.usize_or("columns", 4));
    let png = png_bytes(w, h, &rgba)?;
    Ok(ToolOutput::json(json!({
        "model": path.file_name().map(|s| s.to_string_lossy().to_string()),
        "sequence": seq,
        "name": scenes[0].model.sequences.get(seq),
        "frames_shown": picks,
        "frames_total": total,
        "image_size": [w, h],
        "order": "esquerda para a direita, de cima para baixo",
    }))
    .with_image(&png))
}

fn sprite_sheet(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["spr"], spr::MAX_FILE_BYTES)?;
    let info = spr::load_sprite(&path)?;
    const MAX_CELLS: usize = 64;
    let mut items: Vec<Option<Tex>> = Vec::new();
    let mut cells: Vec<Value> = Vec::new();
    'outer: for (fi, f) in info.frames.iter().enumerate() {
        for (ii, im) in f.images.iter().enumerate() {
            if items.len() >= MAX_CELLS {
                break 'outer;
            }
            items.push(render3d::decode_png_url(&im.png));
            cells.push(json!({"index": items.len() - 1, "frame": fi, "image": ii, "width": im.width, "height": im.height, "origin": [im.origin_x, im.origin_y], "duration": f.durations.get(ii)}));
        }
    }
    if items.is_empty() {
        return Err("o sprite não tem imagens".into());
    }
    let cell = a.usize_or("cell", 96).clamp(32, 256);
    let (w, h, rgba) = sheet(&items, cell, a.usize_or("columns", 8));
    let png = png_bytes(w, h, &rgba)?;
    Ok(ToolOutput::json(json!({
        "name": path.file_name().map(|s| s.to_string_lossy().to_string()),
        "sprite_type": info.sprite_type, "tex_format": info.tex_format, "frame_count": info.frame_count,
        "image_size": [w, h], "cells": cells,
        "truncated": info.frames.iter().map(|f| f.images.len()).sum::<usize>() > MAX_CELLS,
    }))
    .with_image(&png))
}

fn audio_waveform(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["wav", "mp3"], 32 * 1024 * 1024)?;
    let (w, h) = (a.usize_or("width", 800).clamp(200, 1600), a.usize_or("height", 200).clamp(60, 400));
    let info = audio::analyze_audio_file(&path, w)?;
    let mut meta = json!({
        "name": path.file_name().map(|s| s.to_string_lossy().to_string()),
        "codec": info.codec, "channels": info.channels, "sample_rate": info.sample_rate, "bits_per_sample": info.bits_per_sample,
        "duration_secs": info.duration_secs, "peak": info.peak, "loops": info.loops, "cue_points": info.cue_points, "warnings": info.warnings,
    });
    let Some(env) = info.envelope.as_ref().filter(|e| !e.is_empty()) else {
        meta["note"] = json!("sem envelope (MP3/ADPCM ou formato não suportado): só os metadados");
        return Ok(ToolOutput::json(meta));
    };
    let mut buf = vec![0u8; w * h * 4];
    for px in buf.chunks_exact_mut(4) {
        px.copy_from_slice(&[10, 14, 22, 255]);
    }
    // região de loop
    if let (Some(l), Some(total)) = (info.loops.first(), info.sample_frames.filter(|t| *t > 0)) {
        let x0 = (l.start_sample as f64 / total as f64 * w as f64) as usize;
        let x1 = l.end_sample.map_or(w, |e| (e as f64 / total as f64 * w as f64) as usize).min(w);
        for y in 0..h {
            for x in x0.min(w)..x1 {
                let i = (y * w + x) * 4;
                buf[i] = 40;
                buf[i + 1] = 32;
                buf[i + 2] = 14;
            }
        }
    }
    let mid = h as f32 / 2.0;
    for x in 0..w {
        buf[((h / 2) * w + x) * 4..((h / 2) * w + x) * 4 + 3].copy_from_slice(&[40, 52, 70]);
    }
    for x in 0..w {
        let [lo, hi] = env[(x * env.len() / w).min(env.len() - 1)];
        let (y0, y1) = (((mid - hi.clamp(-1.0, 1.0) * mid).max(0.0)) as usize, ((mid - lo.clamp(-1.0, 1.0) * mid).min(h as f32 - 1.0)) as usize);
        for y in y0..=y1.max(y0) {
            let i = (y * w + x) * 4;
            buf[i..i + 3].copy_from_slice(&[56, 217, 195]);
        }
    }
    let png = png_bytes(w, h, &buf)?;
    meta["image_size"] = json!([w, h]);
    meta["legend"] = json!("ciano = onda; faixa âmbar escura = região de loop; eixo = tempo (esquerda para a direita)");
    Ok(ToolOutput::json(meta).with_image(&png))
}
