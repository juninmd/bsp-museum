//! Ferramentas sobre mapas `.bsp` (e pastas de mapas). Cada uma resolve o caminho pelas
//! raízes permitidas, chama o núcleo e enxuga o resultado (sem vértices, sem SVG).

use super::output::{page, strip_key, to_value, ToolError, ToolOutput, UNTRUSTED_NOTE};
use super::schema::{boolean, integer, object, string};
use super::tools::{
    confined_bsp_files, dir_arg, file_arg, load_bsp, map_name, rel, Args, Config, Handler, Tool, MAX_BSP_BYTES,
};
use crate::agentctx;
use crate::bsp::entities;
use crate::bsp::wad;
use crate::catalog::{self, Severity};
use crate::{audit, compare, entity_list, hulls, radar, scene, volumes};
use serde_json::{json, Map, Value};
use std::path::Path;

type R = Result<ToolOutput, ToolError>;

pub(super) fn p_path(desc: &str) -> Value {
    string(desc)
}

pub(super) fn p_offset() -> Value {
    integer("Quantos itens pular (paginação).", 0, 1_000_000, 0)
}

pub(super) fn p_limit(default: i64, max: i64) -> Value {
    integer("Quantos itens devolver no máximo (paginação).", 1, max, default)
}

pub(super) fn p_slots() -> Value {
    integer("Slots do servidor para a regra `poucos-spawns` (spawns por time vs jogadores).", 2, 128, 32)
}

const BSP_PATH: &str = "Caminho do .bsp: absoluto ou relativo a uma raiz permitida (veja list_roots).";

fn tool(name: &'static str, title: &'static str, desc: &str, schema: Value, handler: Handler) -> Tool {
    Tool { name, title, description: desc.to_string(), schema, read_only: true, handler }
}

pub(super) fn tools(cfg: &Config) -> Vec<Tool> {
    let mut radar_tool = tool(
        "map_radar",
        "Radar do mapa",
        "Renderiza o radar (vista de cima em tons de verde) do mapa e devolve o PNG como imagem, mais o texto do `overviews/<mapa>.txt` (ORIGIN/ZOOM). Com `out_dir` grava overviews/<mapa>.bmp|png|txt, mas só se o servidor foi iniciado com --allow-write e o destino estiver dentro de uma raiz permitida.",
        object(
            vec![
                ("path", p_path(BSP_PATH)),
                ("out_dir", string("Pasta (dentro de uma raiz permitida) onde gravar `overviews/<mapa>.*`. Omitido = não grava nada.")),
                ("size", integer("Lado do PNG em pixels.", 64, 1024, 512)),
            ],
            &["path"],
        ),
        map_radar,
    );
    radar_tool.read_only = !cfg.allow_write;

    vec![
        tool(
            "list_maps",
            "Listar mapas",
            "Lista os .bsp de uma pasta (recursivo) com resumo barato: título, modo (de_/cs_/as_/zm…), spawns CT/T, nº de entidades, tamanho da caixa e problemas de entidades. Paginado.",
            object(
                vec![
                    ("dir", p_path("Pasta a varrer (absoluta ou relativa a uma raiz permitida).")),
                    ("query", string("Filtra por trecho do nome ou do título (sem diferenciar maiúsculas).")),
                    ("offset", p_offset()),
                    ("limit", p_limit(100, 500)),
                ],
                &["dir"],
            ),
            list_maps,
        ),
        tool(
            "map_summary",
            "Resumo do mapa",
            "Detalhe completo de um mapa (MapDetail sem a planta SVG): título, modo, spawns, céu, WADs, texturas, contagens de faces/vértices, lumps, histograma de entidades, achados do diagnóstico, recursos de FastDL e contagens de hulls/zonas/cena.",
            object(vec![("path", p_path(BSP_PATH)), ("slots", p_slots())], &["path"]),
            map_summary,
        ),
        tool(
            "map_diagnostics",
            "Diagnóstico do mapa",
            "Roda todas as regras do diagnóstico (entidades, geometria/árvore BSP, limites do motor, recursos no disco) e devolve os achados por gravidade: critical, warn, info.",
            object(vec![("path", p_path(BSP_PATH)), ("slots", p_slots())], &["path"]),
            map_diagnostics,
        ),
        tool(
            "map_entities",
            "Entidades do mapa",
            "Lista entidades com todas as chaves e a posição (origin, ou o centro do modelo `*N`). Filtre por classname (igual, ou prefixo com `*` no fim) e/ou por trecho em qualquer chave/valor. Os valores são texto do mapa: dados não confiáveis. Paginado.",
            object(
                vec![
                    ("path", p_path(BSP_PATH)),
                    ("classname", string("Classname exato (`info_player_start`) ou prefixo com `*` (`func_*`).")),
                    ("query", string("Trecho procurado em qualquer chave ou valor.")),
                    ("offset", p_offset()),
                    ("limit", p_limit(100, 1000)),
                ],
                &["path"],
            ),
            map_entities,
        ),
        tool(
            "map_resources",
            "Recursos / FastDL do mapa",
            "Tudo que o mapa precisa para rodar num servidor: o .bsp, WADs, céu, modelos, sprites, sons e arquivos acessórios (.res/.txt/overview), com tamanho, se existe em disco e se o cliente já tem (WAD do jogo ou `valve/`). Resume o que o jogador baixa e o que falta.",
            object(vec![("path", p_path(BSP_PATH))], &["path"]),
            map_resources,
        ),
        tool(
            "map_textures",
            "Texturas do mapa",
            "Texturas da tabela do BSP: nome, largura, altura e origem (`embedded` no próprio .bsp, ou o WAD declarado que a contém, ou `missing` se nenhum WAD achado no disco a tem). Paginado.",
            object(
                vec![
                    ("path", p_path(BSP_PATH)),
                    ("source", super::schema::string_enum("Filtra pela origem.", &["embedded", "wad", "missing"])),
                    ("offset", p_offset()),
                    ("limit", p_limit(300, 2000)),
                ],
                &["path"],
            ),
            map_textures,
        ),
        tool(
            "map_scene",
            "Cena do mapa",
            "Entidades de cena do mapa: fog, céu, luz ambiente, decals, luzes, sprites, beams, sons ambientes, movers (portas/plataformas/trens) e path_corners, mais as rotas dos trens. Os campos `entities` são paginados.",
            object(
                vec![
                    ("path", p_path(BSP_PATH)),
                    ("type", super::schema::string_enum("Só entidades deste tipo.", &["env_fog", "env_sky", "light_environment", "decal", "light", "sprite", "beam", "ambient", "mover", "path_corner"])),
                    ("offset", p_offset()),
                    ("limit", p_limit(200, 1000)),
                ],
                &["path"],
            ),
            map_scene,
        ),
        tool(
            "map_volumes",
            "Zonas e volumes do mapa",
            "Zonas e gatilhos do mapa (buy zone, bomb target, resgate, trigger_*, func_*): classname, tipo, rótulo, alvo e AABB (mins/maxs), e as superfícies invisíveis (clip, null, aaatrigger…) por contagem. Sem malhas de triângulos.",
            object(vec![("path", p_path(BSP_PATH))], &["path"]),
            map_volumes,
        ),
        tool(
            "map_hulls_info",
            "Hulls de colisão",
            "Contagens da geometria de colisão: hulls 1-3 (em pé, agachado, ponto) e se o modelo 0 tem raiz válida para cada, planos, nós, clipnodes, folhas, modelos e a raiz de hull de cada modelo. Só contagens, sem triângulos.",
            object(
                vec![("path", p_path(BSP_PATH)), ("offset", p_offset()), ("limit", p_limit(100, 1000))],
                &["path"],
            ),
            map_hulls_info,
        ),
        tool(
            "audit_folder",
            "Auditar pasta de mapas",
            "Roda o diagnóstico completo em todos os mapas da pasta (em paralelo) e aponta arquivos duplicados (mesmo conteúdo, nomes diferentes). Devolve totais por gravidade e as linhas por mapa (paginadas). Pode demorar em pastas grandes.",
            object(
                vec![
                    ("dir", p_path("Pasta a auditar (absoluta ou relativa a uma raiz permitida).")),
                    ("slots", p_slots()),
                    ("only_problems", boolean("Só mapas com algum achado crítico ou aviso (ou que não abriram).", false)),
                    ("offset", p_offset()),
                    ("limit", p_limit(50, 300)),
                ],
                &["dir"],
            ),
            audit_folder,
        ),
        tool(
            "compare_maps",
            "Comparar dois mapas",
            "Compara dois mapas (ou duas versões do mesmo): diferença de lumps, entidades por classname, texturas e WADs exclusivos de cada um e quais achados mudaram.",
            object(vec![("a", p_path("Primeiro .bsp.")), ("b", p_path("Segundo .bsp.")), ("slots", p_slots())], &["a", "b"]),
            compare_maps,
        ),
        radar_tool,
        tool(
            "server_checklist",
            "Checklist de servidor",
            "Visão de servidor para uma pasta de mapas: auditoria + recursos ausentes + tamanho total do FastDL (arquivos únicos) + ranking de peso + duplicados, e próximos passos sugeridos. Mesma agregação do REPORT.md da exportação de contexto.",
            object(vec![("dir", p_path("Pasta de mapas (absoluta ou relativa a uma raiz permitida).")), ("slots", p_slots())], &["dir"]),
            server_checklist,
        ),
    ]
}

pub(super) fn notice(mut v: Value) -> Value {
    if let Value::Object(m) = &mut v {
        m.insert("data_notice".into(), json!(UNTRUSTED_NOTE));
    }
    v
}

fn sev_counts(findings: &[catalog::Finding]) -> Value {
    let n = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    json!({"critical": n(Severity::Critical), "warn": n(Severity::Warn), "info": n(Severity::Info)})
}

fn list_maps(cfg: &Config, a: &Args) -> R {
    let dir = dir_arg(cfg, a, "dir")?;
    let query = a.str("query").map(str::to_lowercase);
    let mut cache = catalog::IndexCache::default();
    let (maps, _) = catalog::scan_cached(&dir, &mut cache);
    let mut skipped = 0usize;
    let mut rows: Vec<Value> = Vec::new();
    for m in &maps {
        let inside = Path::new(&m.path).canonicalize().map(|c| cfg.roots.contains(&c)).unwrap_or(false);
        if !inside {
            skipped += 1;
            continue;
        }
        if let Some(q) = &query {
            let hay = format!("{} {}", m.name.to_lowercase(), m.title.as_deref().unwrap_or("").to_lowercase());
            if !hay.contains(q) {
                continue;
            }
        }
        rows.push(json!({
            "name": m.name,
            "path": rel(&dir, Path::new(&m.path)),
            "file_size": m.file_size,
            "title": m.title,
            "mode": m.mode,
            "mode_by_entities": m.mode_by_entities,
            "ct_spawns": m.ct_spawns,
            "t_spawns": m.t_spawns,
            "entity_count": m.entity_count,
            "bsp_version": m.bsp_version,
            "size": m.bounds.map(|b| b.size),
            "fullbright": m.fullbright,
            "problems": m.problems,
            "error": m.error,
        }));
    }
    let (win, mut meta) = page(&rows, a.usize_or("offset", 0), a.usize_or("limit", 100));
    meta.insert("dir".into(), json!(cfg.roots.show(&dir)));
    meta.insert("maps".into(), Value::Array(win));
    if skipped > 0 {
        meta.insert("skipped_outside_roots".into(), json!(skipped));
    }
    Ok(ToolOutput::json(notice(Value::Object(meta))))
}

fn map_summary(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let d = catalog::detail_lite(&path, a.slots())?;
    let mut v = to_value(&d)?;
    if let Value::Object(m) = &mut v {
        m.remove("svg");
        m.remove("polygons");
    }
    Ok(ToolOutput::json(notice(v)))
}

fn map_diagnostics(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let an = catalog::analyze(&path, a.slots())?;
    Ok(ToolOutput::json(notice(json!({
        "map": an.summary.name,
        "slots": a.slots(),
        "counts": sev_counts(&an.findings),
        "findings": to_value(&an.findings)?,
    }))))
}

fn class_matches(pattern: &str, class: &str) -> bool {
    let (p, c) = (pattern.to_ascii_lowercase(), class.to_ascii_lowercase());
    match p.strip_suffix('*') {
        Some(prefix) => c.starts_with(prefix),
        None => c == p,
    }
}

fn map_entities(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let (bsp, parsed) = load_bsp(&path)?;
    let summary = entities::summarize(&parsed);
    let class = a.str("classname");
    let query = a.str("query").map(str::to_lowercase);
    let rows: Vec<Value> = entity_list::list(&bsp, &parsed)
        .into_iter()
        .filter(|r| class.map_or(true, |p| class_matches(p, &r.classname)))
        .filter(|r| match &query {
            Some(q) => r.keys.iter().any(|(k, v)| k.to_lowercase().contains(q) || v.to_lowercase().contains(q)),
            None => true,
        })
        .map(|r| {
            let keys: Map<String, Value> = r.keys.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
            json!({
                "index": r.index,
                "classname": r.classname,
                "targetname": r.targetname,
                "origin": r.origin,
                "model": r.model,
                "keys": keys,
            })
        })
        .collect();
    let (win, mut meta) = page(&rows, a.usize_or("offset", 0), a.usize_or("limit", 100));
    meta.insert("map".into(), json!(map_name(&path)));
    meta.insert("entity_count".into(), json!(summary.total));
    meta.insert("classnames".into(), json!(summary.histogram.iter().take(60).collect::<Vec<_>>()));
    meta.insert("entities".into(), Value::Array(win));
    Ok(ToolOutput::json(notice(Value::Object(meta))))
}

fn map_resources(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let an = catalog::analyze(&path, 32)?;
    let r = &an.resources;
    let missing: Vec<_> = r.items.iter().filter(|i| !i.found && !i.shared).collect();
    Ok(ToolOutput::json(notice(json!({
        "map": an.summary.name,
        "download": {
            "files": r.download_count,
            "bytes": r.download_size,
            "bytes_human": agentctx::human_bytes(r.download_size),
        },
        "missing_count": missing.len(),
        "missing": to_value(&missing)?,
        "items": to_value(&r.items)?,
    }))))
}

fn map_textures(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let (bsp, parsed) = load_bsp(&path)?;
    let ents = entities::summarize(&parsed);
    let sets = wad::declared_named_sets(&path, &ents.wads);
    let filter = a.str("source");
    let mut counts = (0usize, 0usize, 0usize);
    let mut rows: Vec<Value> = Vec::new();
    for t in &bsp.textures {
        let (source, wad_name) = if t.embedded {
            counts.0 += 1;
            ("embedded", None)
        } else {
            let key = t.name.to_ascii_lowercase();
            match sets.iter().find(|(_, names)| names.contains(&key)) {
                Some((w, _)) => {
                    counts.1 += 1;
                    ("wad", Some(w.clone()))
                }
                None => {
                    counts.2 += 1;
                    ("missing", None)
                }
            }
        };
        if filter.is_some_and(|f| f != source) {
            continue;
        }
        rows.push(json!({"name": t.name, "width": t.width, "height": t.height, "source": source, "wad": wad_name}));
    }
    let (win, mut meta) = page(&rows, a.usize_or("offset", 0), a.usize_or("limit", 300));
    meta.insert("map".into(), json!(map_name(&path)));
    meta.insert("declared_wads".into(), json!(ents.wads));
    meta.insert("totals".into(), json!({"embedded": counts.0, "wad": counts.1, "missing": counts.2}));
    meta.insert("textures".into(), Value::Array(win));
    Ok(ToolOutput::json(notice(Value::Object(meta))))
}

fn map_scene(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let s = scene::scene_of(&path)?;
    let want = a.str("type");
    let rows: Vec<Value> = s
        .entities
        .iter()
        .filter(|e| want.map_or(true, |w| w == e.type_name()))
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    let (win, mut meta) = page(&rows, a.usize_or("offset", 0), a.usize_or("limit", 200));
    meta.insert("map".into(), json!(map_name(&path)));
    meta.insert("counts".into(), to_value(&s.counts)?);
    meta.insert("scene_truncated".into(), json!(s.truncated));
    meta.insert("routes".into(), to_value(&s.routes)?);
    meta.insert("entities".into(), Value::Array(win));
    Ok(ToolOutput::json(notice(Value::Object(meta))))
}

fn map_volumes(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let r = volumes::volumes(&path)?;
    let mut v = to_value(&r)?;
    // só contagens e AABBs: nada de malha de triângulos nem legenda de cores
    strip_key(&mut v, "positions");
    if let Value::Object(m) = &mut v {
        m.remove("legend");
        m.insert("map".into(), json!(map_name(&path)));
    }
    Ok(ToolOutput::json(notice(v)))
}

fn map_hulls_info(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let (bsp, parsed) = load_bsp(&path)?;
    let (zones, invisible_faces) = volumes::counts(&bsp, &parsed);
    let models: Vec<Value> = bsp
        .models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            json!({"index": i, "mins": m.mins, "maxs": m.maxs, "headnode": m.headnode, "hull_heads": m.hull_heads, "visleafs": m.visleafs, "num_faces": m.num_faces})
        })
        .collect();
    let (win, mut meta) = page(&models, a.usize_or("offset", 0), a.usize_or("limit", 100));
    meta.insert("map".into(), json!(map_name(&path)));
    meta.insert("hulls".into(), to_value(&hulls::hull_infos(&bsp))?);
    meta.insert(
        "counts".into(),
        json!({
            "planes": bsp.planes.len(),
            "nodes": bsp.nodes.len(),
            "clipnodes": bsp.clipnodes.len(),
            "leaves": bsp.leaves.len(),
            "models": bsp.models.len(),
            "faces": bsp.faces.len(),
            "vertices": bsp.vertices.len(),
            "invisible_faces": invisible_faces,
        }),
    );
    meta.insert("zones".into(), to_value(&zones)?);
    meta.insert("models".into(), Value::Array(win));
    Ok(ToolOutput::json(Value::Object(meta)))
}

fn audit_folder(cfg: &Config, a: &Args) -> R {
    let dir = dir_arg(cfg, a, "dir")?;
    let (files, skipped) = confined_bsp_files(cfg, &dir);
    let report = audit::run_files(&dir, files, a.slots());
    let (mut critical, mut warn, mut info) = (0, 0, 0);
    let (mut maps_critical, mut maps_warn, mut errors) = (0, 0, 0);
    for r in &report.rows {
        if r.error.is_some() {
            errors += 1;
        }
        let has = |s: Severity| r.findings.iter().any(|f| f.severity == s);
        maps_critical += usize::from(has(Severity::Critical));
        maps_warn += usize::from(has(Severity::Warn));
        for f in &r.findings {
            match f.severity {
                Severity::Critical => critical += 1,
                Severity::Warn => warn += 1,
                Severity::Info => info += 1,
            }
        }
    }
    let only = a.bool_or("only_problems", false);
    let rows: Vec<Value> = report
        .rows
        .iter()
        .filter(|r| !only || r.error.is_some() || r.findings.iter().any(|f| f.severity != Severity::Info))
        .map(|r| {
            json!({
                "name": r.name,
                "path": rel(&dir, Path::new(&r.path)),
                "file_size": r.file_size,
                "mode": r.mode,
                "ct_spawns": r.ct_spawns,
                "t_spawns": r.t_spawns,
                "bsp_version": r.bsp_version,
                "error": r.error,
                "findings": r.findings.iter().map(|f| json!({"id": f.id, "severity": f.severity, "title": f.title, "detail": f.detail})).collect::<Vec<_>>(),
            })
        })
        .collect();
    let dups: Vec<Vec<String>> =
        report.duplicates.iter().map(|g| g.iter().map(|p| rel(&dir, Path::new(p))).collect()).collect();
    let (win, mut meta) = page(&rows, a.usize_or("offset", 0), a.usize_or("limit", 50));
    meta.insert("dir".into(), json!(cfg.roots.show(&dir)));
    meta.insert("slots".into(), json!(report.slots));
    meta.insert("maps".into(), json!(report.rows.len()));
    meta.insert("unreadable".into(), json!(errors));
    meta.insert("findings".into(), json!({"critical": critical, "warn": warn, "info": info}));
    meta.insert("maps_with".into(), json!({"critical": maps_critical, "warn": maps_warn}));
    meta.insert("duplicates".into(), json!(dups));
    if skipped > 0 {
        meta.insert("skipped_outside_roots".into(), json!(skipped));
    }
    meta.insert("rows".into(), Value::Array(win));
    Ok(ToolOutput::json(notice(Value::Object(meta))))
}

fn compare_maps(cfg: &Config, a: &Args) -> R {
    let pa = file_arg(cfg, a, "a", &["bsp"], MAX_BSP_BYTES)?;
    let pb = file_arg(cfg, a, "b", &["bsp"], MAX_BSP_BYTES)?;
    let c = compare::compare(&pa, &pb, a.slots())?;
    Ok(ToolOutput::json(notice(to_value(&c)?)))
}

fn map_radar(cfg: &Config, a: &Args) -> R {
    let path = file_arg(cfg, a, "path", &["bsp"], MAX_BSP_BYTES)?;
    let name = map_name(&path);
    let (bsp, _) = load_bsp(&path)?;
    let size = a.usize_or("size", 512);
    let radar = radar::render(&bsp, size).ok_or("mapa sem modelo 0: não há o que desenhar")?;
    let png = radar.png().ok_or("falha ao codificar o radar")?;
    let overview = radar.overview_txt(&name);
    let mut written: Vec<String> = Vec::new();
    if let Some(out_dir) = a.str("out_dir") {
        if !cfg.allow_write {
            return Err("gravação desativada: inicie o servidor com --allow-write (ou BSP_MUSEUM_ALLOW_WRITE=1) para usar out_dir".into());
        }
        let base = cfg.roots.resolve_for_write(out_dir)?;
        let dir = base.join("overviews");
        std::fs::create_dir_all(&dir).map_err(|e| format!("não criou {}: {e}", dir.display()))?;
        // o destino acabou de ser criado/confirmado: confere de novo depois de existir (symlink plantado no meio)
        let real = dir.canonicalize().map_err(|e| e.to_string())?;
        if !cfg.roots.contains(&real) {
            return Err("destino fora das raízes permitidas".into());
        }
        for (file, data) in
            [(format!("{name}.bmp"), radar.bmp()), (format!("{name}.png"), png.clone()), (format!("{name}.txt"), overview.clone().into_bytes())]
        {
            let target = real.join(file);
            std::fs::write(&target, data).map_err(|e| format!("não gravou {}: {e}", target.display()))?;
            written.push(cfg.roots.show(&target));
        }
    }
    let out = ToolOutput::json(json!({
        "map": name,
        "size": radar.size,
        "origin": radar.origin,
        "units_per_pixel": radar.units_per_pixel,
        "overview_txt": overview,
        "written": written,
    }));
    Ok(out.with_image(&png))
}

fn server_checklist(cfg: &Config, a: &Args) -> R {
    let dir = dir_arg(cfg, a, "dir")?;
    let (files, skipped) = confined_bsp_files(cfg, &dir);
    let collected = agentctx::collect(&dir, files, a.slots());
    let mut o = agentctx::overview(&collected);
    o.dir = cfg.roots.show(&dir);
    // versão enxuta: o REPORT.md/fastdl.txt da exportação têm a lista inteira
    for g in &mut o.groups {
        g.entries.truncate(10);
    }
    o.ranking.truncate(15);
    o.missing.truncate(100);
    let fast_total = o.fastdl.files;
    o.fastdl.list.sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
    o.fastdl.list.truncate(25);
    let mut v = to_value(&o)?;
    if let Value::Object(m) = &mut v {
        let fastdl = m.get_mut("fastdl").and_then(Value::as_object_mut);
        if let Some(f) = fastdl {
            if let Some(list) = f.remove("list") {
                f.insert("largest".into(), list);
                f.insert("files_total".into(), json!(fast_total));
            }
        }
        if skipped > 0 {
            m.insert("skipped_outside_roots".into(), json!(skipped));
        }
        m.insert("note".into(), json!("Listas cortadas (maiores/primeiros); para a lista completa use export-context (REPORT.md, fastdl.txt) ou audit_folder/map_resources."));
    }
    Ok(ToolOutput::json(notice(v)))
}
