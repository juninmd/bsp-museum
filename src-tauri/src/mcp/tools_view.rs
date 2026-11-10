//! Ferramentas que *mostram* coisas ao agente como imagem PNG: o mapa visto por dentro ou de
//! cima (render por software, `render3d`), o modelo `.mdl` posado, a skin, imagens TGA/BMP
//! e uma folha de texturas de WAD. Tudo só lê arquivos e chama o núcleo.

use super::output::{page, strip_key, to_value, ToolError, ToolOutput};
use super::schema::{boolean, integer, number, object, string, string_enum};
use super::tools::{file_arg, Args, Config, Handler, Tool, MAX_BSP_BYTES, MAX_MDL_BYTES};
use super::tools_map::{p_limit, p_offset, p_path};
use crate::render3d::{self, Camera, MeshInput, Options, Tex};
use crate::{catalog, image, mdl, wadlist};
use serde_json::{json, Value};

type R = Result<ToolOutput, ToolError>;

fn tool(name: &'static str, title: &'static str, desc: &str, schema: Value, handler: Handler) -> Tool {
    Tool { name, title, description: desc.to_string(), schema, read_only: true, handler }
}

pub(super) fn tools(_cfg: &Config) -> Vec<Tool> {
    let size = |d: i64| integer("Largura da imagem em pixels (a altura segue 3:2 em mapas, 1:1 em modelos).", 64, 1600, d);
    vec![
        tool(
            "map_view",
            "Ver o mapa (render 3D em PNG)",
            "Renderiza o mapa com texturas (embutidas e de WADs) e lightmap, por software, e devolve a imagem PNG para você VER o layout. \
             `preset`: `overview` (vista isométrica de cima, com corte do teto — padrão), `top` (planta de cima), `spawn_ct` / `spawn_t` (olhos de um spawn, olhando para o centro) ou `free` (use x/y/z/yaw/pitch). \
             Spawns aparecem como bolinhas (azul CT, laranja T). Coordenadas GoldSrc: Z para cima; yaw 0 olha para +X, 90 para +Y. Use `map_entities` para achar posições e depois `free` para olhar de perto.",
            object(
                vec![
                    ("path", p_path("Caminho do .bsp.")),
                    ("preset", string_enum("Ponto de vista.", &["overview", "top", "spawn_ct", "spawn_t", "free"])),
                    ("x", number("Posição X da câmera (`free`).", -1e6, 1e6)),
                    ("y", number("Posição Y da câmera (`free`).", -1e6, 1e6)),
                    ("z", number("Posição Z da câmera (`free`).", -1e6, 1e6)),
                    ("yaw", number("Direção horizontal em graus (0 = +X, 90 = +Y). Em `overview`/`top` gira a vista.", -360.0, 360.0)),
                    ("pitch", number("Inclinação em graus (positivo olha para cima).", -89.0, 89.0)),
                    ("fov", number("Campo de visão horizontal em graus.", 30.0, 140.0)),
                    ("clip_z", number("Descarta o que está acima desta altura (ver dentro do mapa). `overview`/`top` já cortam o teto; passe um valor alto para desligar.", -1e6, 1e6)),
                    ("lighting", boolean("Aplica o lightmap do mapa (se houver).", true)),
                    ("width", size(960)),
                ],
                &["path"],
            ),
            map_view,
        ),
        tool(
            "model_view",
            "Ver um modelo .mdl (render 3D em PNG)",
            "Renderiza um modelo GoldSrc (jogador, arma, prop) com as texturas da skin, na pose de repouso ou num quadro de uma sequência, e devolve a imagem PNG. \
             Sem `yaw`/`pitch` usa uma vista 3/4. Use `model_info` antes para listar sequências, bodygroups e skins.",
            object(
                vec![
                    ("path", p_path("Caminho do .mdl.")),
                    ("yaw", number("Giro em graus em volta do modelo (padrão 135: 3/4 pela frente).", -360.0, 360.0)),
                    ("pitch", number("Inclinação da câmera em graus (padrão -15).", -89.0, 89.0)),
                    ("skin", integer("Família de skin.", 0, 255, 0)),
                    ("body", integer("Valor de bodygroup (`bodyparts`).", 0, 100000, 0)),
                    ("sequence", integer("Sequência a posar (índice de `model_info`). Omitido = pose de repouso.", 0, 4096, 0)),
                    ("frame", integer("Quadro da sequência.", 0, 100000, 0)),
                    ("top", integer("Cor de time de cima (matiz 0..255), em modelos com remap.", 0, 255, 0)),
                    ("bottom", integer("Cor de time de baixo (matiz 0..255), em modelos com remap.", 0, 255, 0)),
                    ("size", integer("Lado da imagem quadrada em pixels.", 64, 1200, 640)),
                ],
                &["path"],
            ),
            model_view,
        ),
        tool(
            "model_texture",
            "Textura (skin) de um modelo",
            "Devolve como imagem PNG a textura `index` do `.mdl` (as texturas estão listadas em `model_info`). Com `skin`, a textura que aquela família usa no lugar.",
            object(
                vec![
                    ("path", p_path("Caminho do .mdl.")),
                    ("index", integer("Índice da textura em `model_info.textures`.", 0, 1024, 0)),
                    ("skin", integer("Família de skin (troca a textura base pela da família).", 0, 255, 0)),
                ],
                &["path"],
            ),
            model_texture,
        ),
        tool(
            "image_view",
            "Ver uma imagem (TGA/BMP/LMP)",
            "Decodifica uma imagem do jogo (`.tga`, `.bmp`, `.lmp`: céu, spray, HUD, overview/radar) e devolve o PNG, mais formato, dimensões, bits, RLE e tipo de paleta.",
            object(vec![("path", p_path("Caminho do .tga, .bmp ou .lmp."))], &["path"]),
            image_view,
        ),
        tool(
            "wad_sheet",
            "Folha de texturas de um WAD",
            "Monta uma folha de contato PNG (grade) com as texturas de um WAD — a forma mais rápida de ver o que o WAD contém. Devolve também `cells` (índice, nome, linha, coluna de cada célula, na ordem). Paginado por `offset`/`limit`.",
            object(
                vec![
                    ("path", p_path("Caminho do .wad.")),
                    ("query", string("Só texturas cujo nome contém este trecho.")),
                    ("offset", p_offset()),
                    ("limit", p_limit(36, 100)),
                    ("cell", integer("Lado de cada célula em pixels.", 32, 256, 96)),
                    ("columns", integer("Colunas da grade.", 1, 16, 6)),
                ],
                &["path"],
            ),
            wad_sheet,
        ),
    ]
}

fn png_bytes(w: usize, h: usize, rgba: &[u8]) -> Result<Vec<u8>, ToolError> {
    crate::bsp::png_encode(w, h, rgba).ok_or_else(|| "falha ao codificar o PNG".into())
}

fn decode_textures(list: impl Iterator<Item = (String, Option<String>)>) -> Vec<Option<Tex>> {
    list.map(|(name, png)| {
        png.as_deref().and_then(render3d::decode_png_url).map(|mut t| {
            t.masked = name.starts_with('{');
            t
        })
    })
    .collect()
}

fn map_view(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let mesh = catalog::mesh(&path)?;
    let b = mesh.bounds.as_ref().ok_or("mapa sem geometria para desenhar")?;
    let width = a.usize_or("width", 960).clamp(64, 1600);
    let height = width * 2 / 3;
    let fov = a.opt_f32("fov").unwrap_or(75.0);
    let preset = a.str("preset").unwrap_or("overview");
    let center = [(b.mins[0] + b.maxs[0]) / 2.0, (b.mins[1] + b.maxs[1]) / 2.0, (b.mins[2] + b.maxs[2]) / 2.0];

    let spawn_of = |team: &str| mesh.spawns.iter().find(|s| s.team == team).map(|s| s.position);
    let eye_at = |p: [f32; 3]| {
        let (dx, dy) = (center[0] - p[0], center[1] - p[1]);
        let yaw = if dx.abs() + dy.abs() < 1.0 { 0.0 } else { dy.atan2(dx).to_degrees() };
        Camera { pos: [p[0], p[1], p[2] + 17.0], yaw: a.opt_f32("yaw").unwrap_or(yaw), pitch: a.opt_f32("pitch").unwrap_or(0.0), fov }
    };
    let mut clip = a.opt_f32("clip_z");
    let cam = match preset {
        "overview" | "top" => {
            let (pitch, yaw) = if preset == "top" { (-89.0, 90.0) } else { (-50.0, 45.0) };
            if clip.is_none() {
                clip = Some(b.mins[2] + b.size[2] * 0.6);
            }
            // enquadra só o que sobra depois do corte
            let mut maxs = b.maxs;
            maxs[2] = clip.unwrap_or(b.maxs[2]).min(b.maxs[2]).max(b.mins[2] + 1.0);
            render3d::frame_box(b.mins, maxs, a.opt_f32("yaw").unwrap_or(yaw), a.opt_f32("pitch").unwrap_or(pitch), fov.min(60.0))
        }
        "spawn_ct" => eye_at(spawn_of("CT").ok_or("o mapa não tem spawn de CT")?),
        "spawn_t" => eye_at(spawn_of("T").ok_or("o mapa não tem spawn de T")?),
        _ => Camera {
            pos: [a.opt_f32("x").unwrap_or(center[0]), a.opt_f32("y").unwrap_or(center[1]), a.opt_f32("z").unwrap_or(center[2])],
            yaw: a.opt_f32("yaw").unwrap_or(0.0),
            pitch: a.opt_f32("pitch").unwrap_or(0.0),
            fov,
        },
    };

    let textures = decode_textures(mesh.textures.iter().map(|t| (t.name.clone(), t.png.clone())));
    let lit = a.bool_or("lighting", true);
    let lightmap = if lit { mesh.lightmap.as_deref().and_then(render3d::decode_png_url) } else { None };
    let input = MeshInput {
        positions: &mesh.positions,
        uvs: &mesh.uvs,
        texindex: &mesh.texindex,
        textures: &textures,
        lm_uvs: if lit { &mesh.lm_uvs } else { &[] },
        lightmap: lightmap.as_ref(),
    };
    let mut opt = Options::new(width, height);
    opt.clip_z = clip;
    opt.markers = mesh
        .spawns
        .iter()
        .map(|s| render3d::Marker { pos: [s.position[0], s.position[1], s.position[2] + 20.0], rgb: if s.team == "T" { [255, 140, 40] } else { [60, 140, 255] } })
        .collect();
    let rgba = render3d::render(&input, &cam, &opt);
    let png = png_bytes(opt.width, opt.height, &rgba)?;
    Ok(ToolOutput::json(json!({
        "map": super::tools::map_name(&path),
        "preset": preset,
        "camera": {"position": cam.pos, "yaw": cam.yaw, "pitch": cam.pitch, "fov": cam.fov},
        "size": [opt.width, opt.height],
        "clip_z": clip,
        "bounds": {"mins": b.mins, "maxs": b.maxs},
        "triangles": mesh.triangles,
        "textures": {"total": textures.len(), "decoded": textures.iter().flatten().count()},
        "lightmap": lightmap.is_some(),
        "legend": "bolinha azul = spawn CT, laranja = spawn T; cinza liso = textura de WAD não achado",
    }))
    .with_image(&png))
}

/// Posa os vértices locais de cada bone com a pose de um quadro (`pose[bone] * local`).
fn skin_pose(model: &catalog::MdlSummary, seq: &mdl::SeqFrames, frame: usize) -> Option<Vec<f32>> {
    let bones = seq.bones as usize;
    let frames = seq.frames as usize;
    if bones == 0 || frames == 0 || model.local_positions.len() != model.vert_bones.len() * 3 {
        return None;
    }
    let f = frame.min(frames - 1);
    let base = f * bones * mdl::POSE_STRIDE;
    let pose = seq.data.get(base..base + bones * mdl::POSE_STRIDE)?;
    let mut out = Vec::with_capacity(model.local_positions.len());
    for (i, &bone) in model.vert_bones.iter().enumerate() {
        let b = (bone as usize).min(bones - 1) * mdl::POSE_STRIDE;
        let (p, q) = (&pose[b..b + 3], &pose[b + 3..b + 7]);
        let v = [model.local_positions[i * 3], model.local_positions[i * 3 + 1], model.local_positions[i * 3 + 2]];
        // v' = v + 2w(u×v) + 2u×(u×v), com u = (qx,qy,qz)
        let u = [q[0], q[1], q[2]];
        let w = q[3];
        let uv = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let uuv = [u[1] * uv[2] - u[2] * uv[1], u[2] * uv[0] - u[0] * uv[2], u[0] * uv[1] - u[1] * uv[0]];
        for k in 0..3 {
            out.push(v[k] + 2.0 * (w * uv[k] + uuv[k]) + p[k]);
        }
    }
    Some(out)
}

fn model_view(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["mdl"], MAX_MDL_BYTES)?;
    let opt_u8 = |k: &str| a.opt_usize(k).map(|v| v.min(255) as u8);
    let model = catalog::load_model_ex(
        &path,
        a.opt_usize("body").map(|v| v as u32),
        a.opt_usize("skin").map(|v| v as u32),
        opt_u8("top"),
        opt_u8("bottom"),
    )?;
    if model.positions.is_empty() {
        return Err("o modelo não tem triângulos para desenhar".into());
    }
    let mut note = Value::Null;
    let posed = match a.opt_usize("sequence") {
        Some(seq) => {
            let frames = catalog::load_sequence(&path, seq)?;
            let frame = a.usize_or("frame", 0);
            match skin_pose(&model, &frames, frame) {
                Some(p) => Some((p, seq, frame.min(frames.frames.saturating_sub(1) as usize), frames.frames)),
                None => {
                    note = json!("sequência sem quadros: desenhado em repouso");
                    None
                }
            }
        }
        None => None,
    };
    let positions: &[f32] = posed.as_ref().map(|p| p.0.as_slice()).unwrap_or(&model.positions);
    let (mut mins, mut maxs) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in positions.chunks_exact(3) {
        for k in 0..3 {
            mins[k] = mins[k].min(v[k]);
            maxs[k] = maxs[k].max(v[k]);
        }
    }
    let textures = decode_textures(model.textures.iter().map(|t| (t.name.clone(), t.png.clone())));
    // `masked` vem da flag da textura do .mdl, não do nome
    let mut textures = textures;
    for (t, info) in textures.iter_mut().zip(model.extra.texture_info.iter()) {
        if let Some(t) = t {
            t.masked = info.masked;
        }
    }
    let input = MeshInput { positions, uvs: &model.uvs, texindex: &model.texindex, textures: &textures, lm_uvs: &[], lightmap: None };
    let yaw = a.opt_f32("yaw").unwrap_or(135.0);
    let pitch = a.opt_f32("pitch").unwrap_or(-15.0);
    let cam = render3d::frame_box(mins, maxs, yaw, pitch, 45.0);
    let side = a.usize_or("size", 640).clamp(64, 1200);
    let mut opt = Options::new(side, side);
    opt.bg = [38, 42, 52];
    let rgba = render3d::render(&input, &cam, &opt);
    let png = png_bytes(opt.width, opt.height, &rgba)?;
    let mut v = json!({
        "model": path.file_name().map(|s| s.to_string_lossy().to_string()),
        "triangles": positions.len() / 9,
        "bounds": {"mins": mins, "maxs": maxs},
        "camera": {"yaw": yaw, "pitch": pitch},
        "textures": textures.len(),
        "sequences": model.sequences.len(),
        "skin_families": model.skin_families.len(),
        "pose": "rest",
    });
    if let Some((_, seq, frame, total)) = &posed {
        v["pose"] = json!({"sequence": seq, "name": model.sequences.get(*seq), "frame": frame, "frames": total});
    }
    if !note.is_null() {
        v["note"] = note;
    }
    Ok(ToolOutput::json(v).with_image(&png))
}

fn png_bytes_of(data_url: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(data_url.strip_prefix("data:image/png;base64,")?).ok()
}

fn model_texture(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["mdl"], MAX_MDL_BYTES)?;
    let skin = a.usize_or("skin", 0);
    let m = mdl::files::parse_file(&path, &mdl::ParseOptions { skin: skin as u32, ..Default::default() })?;
    let want = a.usize_or("index", 0);
    // a família diz qual textura substitui a base `index`
    let real = if skin > 0 { m.skin_families.get(skin).and_then(|f| f.get(want)).map(|&i| i as usize).unwrap_or(want) } else { want };
    let t = m.textures.get(real).ok_or_else(|| ToolError::Failed(format!("o modelo tem {} textura(s); índice {want} não existe", m.textures.len())))?;
    let png = png_bytes_of(&t.png).ok_or("a textura não gerou um PNG válido")?;
    Ok(ToolOutput::json(json!({"name": t.name, "index": real, "width": t.width, "height": t.height, "skin": skin})).with_image(&png))
}

fn image_view(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["tga", "bmp", "lmp"], image::MAX_FILE_BYTES)?;
    let info = image::load_image(&path)?;
    let png = png_bytes_of(&info.png).ok_or("a imagem não gerou um PNG válido")?;
    let mut v = to_value(&info)?;
    strip_key(&mut v, "png");
    if let Value::Object(m) = &mut v {
        m.insert("name".into(), json!(path.file_name().map(|s| s.to_string_lossy().to_string())));
    }
    Ok(ToolOutput::json(v).with_image(&png))
}

/// Copia `src` (RGBA `sw`x`sh`) para dentro da célula `cell`x`cell` de `dst`, centralizada, por vizinho mais próximo.
fn blit_fit(dst: &mut [u8], dw: usize, ox: usize, oy: usize, cell: usize, src: &Tex) {
    let scale = (cell as f32 / src.w as f32).min(cell as f32 / src.h as f32);
    let (tw, th) = (((src.w as f32 * scale) as usize).clamp(1, cell), ((src.h as f32 * scale) as usize).clamp(1, cell));
    let (px, py) = (ox + (cell - tw) / 2, oy + (cell - th) / 2);
    for y in 0..th {
        for x in 0..tw {
            let (sx, sy) = (x * src.w / tw, y * src.h / th);
            let s = (sy * src.w + sx) * 4;
            let d = ((py + y) * dw + px + x) * 4;
            let a = src.rgba[s + 3] as u32;
            for k in 0..3 {
                // transparente sobre o xadrez de fundo
                dst[d + k] = ((src.rgba[s + k] as u32 * a + dst[d + k] as u32 * (255 - a)) / 255) as u8;
            }
        }
    }
}

fn wad_sheet(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["wad"], wadlist::MAX_WAD_BYTES)?;
    let listing = wadlist::list_wad(&path)?;
    let q = a.str("query").map(str::to_ascii_lowercase);
    let entries: Vec<&wadlist::WadEntry> = listing
        .entries
        .iter()
        .filter(|e| e.decodable && q.as_ref().map_or(true, |q| e.name.to_ascii_lowercase().contains(q)))
        .collect();
    let (win, mut meta) = page(&entries, a.usize_or("offset", 0), a.usize_or("limit", 36));
    if win.is_empty() {
        return Err("nenhuma textura decodificável nesta janela".into());
    }
    let cell = a.usize_or("cell", 96).clamp(32, 256);
    let cols = a.usize_or("columns", 6).clamp(1, 16).min(win.len());
    let rows = win.len().div_ceil(cols);
    let gap = 4;
    let (w, h) = (cols * (cell + gap) + gap, rows * (cell + gap) + gap);
    let mut sheet = vec![0u8; w * h * 4];
    for (i, px) in sheet.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i % w, i / w);
        let c = if (x / 8 + y / 8) % 2 == 0 { 44 } else { 56 };
        px.copy_from_slice(&[c, c, c + 4, 255]);
    }
    let mut cells = Vec::new();
    let offset = meta.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    for (i, e) in win.iter().enumerate() {
        let (col, row) = (i % cols, i / cols);
        let ok = wadlist::wad_texture(&path, &e.name, false)
            .ok()
            .and_then(|t| render3d::decode_png_url(&t.png))
            .map(|t| blit_fit(&mut sheet, w, gap + col * (cell + gap), gap + row * (cell + gap), cell, &t))
            .is_some();
        cells.push(json!({"index": offset + i, "name": e.name, "kind": e.kind, "width": e.width, "height": e.height, "row": row, "col": col, "drawn": ok}));
    }
    let png = png_bytes(w, h, &sheet)?;
    meta.insert("wad".into(), json!(path.file_name().map(|s| s.to_string_lossy().to_string())));
    meta.insert("image_size".into(), json!([w, h]));
    meta.insert("cell".into(), json!(cell));
    meta.insert("columns".into(), json!(cols));
    meta.insert("cells".into(), Value::Array(cells));
    Ok(ToolOutput::json(Value::Object(meta)).with_image(&png))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_do_quadro_0_de_idle_bate_com_o_repouso() {
        let t = crate::mcp::testutil::Tmp::new();
        let p = t.put("m.mdl", &crate::mdl::synthetic_anim_data());
        let model = catalog::load_model(&p).unwrap();
        let seq = catalog::load_sequence(&p, 0).unwrap();
        let posed = skin_pose(&model, &seq, 0).unwrap();
        assert_eq!(posed.len(), model.positions.len());
        for (a, b) in posed.iter().zip(&model.positions) {
            assert!((a - b).abs() < 1e-3, "pose {a} != repouso {b}");
        }
        // quadro além do fim fica preso no último
        assert!(skin_pose(&model, &seq, 999).is_some());
    }
}
