use crate::bsp::entities::{self, EntitySummary, GameMode};
use crate::bsp::render::{self, RenderOptions};
use crate::bsp::{Bsp, Lump, LUMP_ENTITIES, LUMP_LIGHTING, LUMP_MODELS, LUMP_NAMES};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Bounds {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub size: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct MapSummary {
    pub path: String,
    pub name: String,
    pub file_size: u64,
    pub title: Option<String>,
    /// modo que o prefixo do arquivo promete
    pub mode: GameMode,
    pub mode_label: String,
    /// modo que as entidades entregam
    pub mode_by_entities: GameMode,
    pub ct_spawns: usize,
    pub t_spawns: usize,
    pub entity_count: usize,
    pub bounds: Option<Bounds>,
    pub fullbright: bool,
    /// preenchido quando o arquivo não pôde ser lido
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LumpInfo {
    pub name: &'static str,
    pub length: usize,
    pub percent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warn,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: &'static str,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MapDetail {
    pub summary: MapSummary,
    pub sky: Option<String>,
    pub wads: Vec<String>,
    pub textures: Vec<String>,
    pub texture_count: usize,
    pub embedded_textures: usize,
    pub face_count: usize,
    pub vertex_count: usize,
    pub model_count: usize,
    pub histogram: Vec<(String, usize)>,
    pub lumps: Vec<LumpInfo>,
    pub findings: Vec<Finding>,
    pub svg: String,
    pub polygons: usize,
}

fn map_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn bounds_of(model: Option<crate::bsp::Model>) -> Option<Bounds> {
    model.map(|m| Bounds {
        mins: m.mins,
        maxs: m.maxs,
        size: [m.maxs[0] - m.mins[0], m.maxs[1] - m.mins[1], m.maxs[2] - m.mins[2]],
    })
}

/// Lê só os bytes de um lump, sem carregar o mapa inteiro.
/// É o que torna a varredura de 200 mapas instantânea em vez de 1 GB de I/O.
fn read_lump(file: &mut File, lump: Lump) -> std::io::Result<Vec<u8>> {
    if lump.length == 0 {
        return Ok(Vec::new());
    }
    file.seek(SeekFrom::Start(lump.offset as u64))?;
    let mut buf = vec![0u8; lump.length];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Resumo barato: cabeçalho + lump de entidades + lump de modelos.
pub fn summarize_file(path: &Path) -> MapSummary {
    let name = map_name(path);
    let mode = entities::mode_from_prefix(&name);
    let mut summary = MapSummary {
        path: path.to_string_lossy().to_string(),
        name,
        file_size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        title: None,
        mode,
        mode_label: mode.label().to_string(),
        mode_by_entities: GameMode::Unknown,
        ct_spawns: 0,
        t_spawns: 0,
        entity_count: 0,
        bounds: None,
        fullbright: false,
        error: None,
    };

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(err) => {
            summary.error = Some(format!("não abriu: {err}"));
            return summary;
        }
    };

    let mut header = [0u8; 4 + 15 * 8];
    if let Err(err) = file.read_exact(&mut header) {
        summary.error = Some(format!("cabeçalho ilegível: {err}"));
        return summary;
    }
    let lumps = match Bsp::header(&header) {
        Ok((_, lumps)) => lumps,
        Err(err) => {
            summary.error = Some(err.to_string());
            return summary;
        }
    };

    let ent_bytes = match read_lump(&mut file, lumps[LUMP_ENTITIES]) {
        Ok(bytes) => bytes,
        Err(err) => {
            summary.error = Some(format!("lump de entidades ilegível: {err}"));
            return summary;
        }
    };
    let end = ent_bytes.iter().position(|&b| b == 0).unwrap_or(ent_bytes.len());
    let text = String::from_utf8_lossy(&ent_bytes[..end]);
    let parsed = entities::parse(&text);
    let ents = entities::summarize(&parsed);

    let model_bytes = read_lump(&mut file, lumps[LUMP_MODELS]).unwrap_or_default();
    let first_model = parse_first_model(&model_bytes);

    summary.title = ents.title.clone();
    summary.mode_by_entities = ents.mode_by_entities;
    summary.ct_spawns = ents.ct_spawns;
    summary.t_spawns = ents.t_spawns;
    summary.entity_count = ents.total;
    summary.bounds = bounds_of(first_model);
    summary.fullbright = lumps[LUMP_LIGHTING].length == 0;
    summary
}

fn parse_first_model(bytes: &[u8]) -> Option<crate::bsp::Model> {
    use crate::bsp::reader::Cursor;
    if bytes.len() < 64 {
        return None;
    }
    let mut cur = Cursor::new(bytes);
    let mins = cur.vec3().ok()?;
    let maxs = cur.vec3().ok()?;
    let origin = cur.vec3().ok()?;
    cur.skip(16).ok()?;
    cur.skip(4).ok()?;
    let first_face = cur.i32().ok()?;
    let num_faces = cur.i32().ok()?;
    Some(crate::bsp::Model { mins, maxs, origin, first_face, num_faces })
}

pub fn find_bsp_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("bsp")) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Varredura paralela: disco e CPU juntos, sem dependência externa.
pub fn scan(dir: &Path) -> Vec<MapSummary> {
    let files = find_bsp_files(dir);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    if files.len() < 2 || workers < 2 {
        return files.iter().map(|p| summarize_file(p)).collect();
    }

    let chunk = files.len().div_ceil(workers);
    let mut results: Vec<Vec<MapSummary>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|slice| scope.spawn(move || slice.iter().map(|p| summarize_file(p)).collect::<Vec<_>>()))
            .collect();
        for handle in handles {
            results.push(handle.join().unwrap_or_default());
        }
    });
    results.into_iter().flatten().collect()
}

/// Regras herdadas do ritual de subir mapa em servidor: o que faz o round não
/// terminar, o jogador não comprar, ou o mapa carregar como deathmatch.
pub fn findings(summary: &MapSummary, ents: &EntitySummary, bsp: &Bsp, slots: usize) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut push = |id: &'static str, severity: Severity, title: String, detail: String, hint: &str| {
        out.push(Finding { id, severity, title, detail, hint: hint.to_string() });
    };

    match summary.mode {
        GameMode::Bomb if ents.bomb_targets == 0 => push(
            "de-sem-bomb-target",
            Severity::Critical,
            "Prefixo de_ sem func_bomb_target".into(),
            "O CS entra em modo bomba pelo prefixo, mas não existe alvo para plantar.".into(),
            "O round nunca termina por objetivo — só por tempo ou eliminação. Adicione func_bomb_target (ou info_bomb_target) e um info_map_parameters se quiser afinar o tempo.",
        ),
        GameMode::Hostage if ents.hostages == 0 => push(
            "cs-sem-refem",
            Severity::Critical,
            "Prefixo cs_ sem hostage_entity".into(),
            "Mapa de resgate sem refém nenhum.".into(),
            "Sem hostage_entity o objetivo não existe. Adicione os reféns e a func_hostage_rescue correspondente.",
        ),
        GameMode::Hostage if ents.rescue_zones == 0 => push(
            "cs-sem-resgate",
            Severity::Critical,
            "Reféns sem zona de resgate".into(),
            format!("{} refém(ns), nenhuma func_hostage_rescue.", ents.hostages),
            "Os CTs não têm onde entregar. Sem a zona, o round não fecha por objetivo.",
        ),
        GameMode::Assassination if ents.vip_starts == 0 || ents.vip_safety == 0 => push(
            "as-incompleto",
            Severity::Critical,
            "Prefixo as_ incompleto".into(),
            format!("info_vip_start={}, func_vip_safetyzone={}", ents.vip_starts, ents.vip_safety),
            "Modo VIP precisa dos dois: um spawn de VIP e ao menos uma zona segura.",
        ),
        _ => {}
    }

    // O caminho inverso: as entidades prometem um modo que o nome não entrega.
    if ents.mode_by_entities != GameMode::Unknown
        && ents.mode_by_entities != GameMode::Deathmatch
        && summary.mode != ents.mode_by_entities
    {
        push(
            "prefixo-divergente",
            Severity::Warn,
            format!(
                "Entidades de {} num arquivo {}",
                ents.mode_by_entities.label(),
                summary.mode.label()
            ),
            "O CS decide o modo pelo prefixo do arquivo, não pelo conteúdo.".into(),
            "Renomeie o BSP para o prefixo certo, ou o objetivo montado no mapa nunca será usado.",
        );
    }

    let total_spawns = ents.ct_spawns + ents.t_spawns;
    if total_spawns == 0 {
        push(
            "sem-spawn",
            Severity::Critical,
            "Nenhum ponto de spawn".into(),
            "Nem info_player_start nem info_player_deathmatch.".into(),
            "Ninguém entra. Este BSP não sobe em servidor.",
        );
    } else {
        if ents.ct_spawns == 0 || ents.t_spawns == 0 {
            push(
                "spawn-de-um-time-so",
                Severity::Critical,
                "Spawn de um time só".into(),
                format!("CT={} · T={}", ents.ct_spawns, ents.t_spawns),
                "O time sem spawn não consegue entrar em jogo. Em mapa de Zombie Plague isso às vezes é proposital, mas confira.",
            );
        }
        if total_spawns < slots {
            push(
                "poucos-spawns",
                Severity::Warn,
                format!("{total_spawns} spawns para {slots} slots"),
                format!("CT={} · T={}", ents.ct_spawns, ents.t_spawns),
                "Com mais jogadores que spawns, o servidor empilha gente no mesmo ponto (telefrag e queda de FPS no início do round).",
            );
        }
    }

    if summary.fullbright {
        push(
            "fullbright",
            Severity::Warn,
            "Mapa sem lightmap (fullbright)".into(),
            "O lump de iluminação está vazio.".into(),
            "Ou faltou rodar o RAD na compilação, ou houve leak. Visualmente é aquele mapa chapado, sem sombra.",
        );
    }

    if matches!(summary.mode, GameMode::Bomb | GameMode::Hostage | GameMode::Assassination)
        && ents.buy_zones == 0
    {
        push(
            "sem-buyzone",
            Severity::Warn,
            "Mapa competitivo sem func_buyzone".into(),
            "Nenhuma zona de compra encontrada.".into(),
            "Sem buyzone o jogador só tem a pistola inicial. Em mapa de zumbi isso costuma ser intencional.",
        );
    }

    let from_wad = bsp.textures.iter().filter(|t| !t.embedded).count();
    if from_wad > 0 && ents.wads.is_empty() {
        push(
            "wad-nao-declarado",
            Severity::Warn,
            format!("{from_wad} textura(s) de WAD, mas o worldspawn não lista nenhum"),
            "A chave \"wad\" do worldspawn está vazia.".into(),
            "Quem não tiver o WAD certo vê tudo rosa e preto. Ou embuta as texturas, ou declare o WAD.",
        );
    }

    if summary.file_size > 8 * 1024 * 1024 {
        push(
            "mapa-pesado",
            Severity::Info,
            format!("{:.1} MB", summary.file_size as f64 / 1024.0 / 1024.0),
            "Mapa grande para download de jogador em servidor público.".into(),
            "Acima de ~8 MB muita gente desiste no meio do download. Vale checar o que domina os lumps.",
        );
    }

    out
}

pub fn detail(path: &Path, opts: RenderOptions, slots: usize) -> Result<MapDetail, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    let parsed = entities::parse(&bsp.entities_raw);
    let ents = entities::summarize(&parsed);
    let mut summary = summarize_file(path);
    // O resumo leve não enxerga o lump de iluminação com precisão; aqui sim.
    summary.fullbright = bsp.lumps[LUMP_LIGHTING].length == 0;

    let total: usize = bsp.lumps.iter().map(|l| l.length).sum();
    let mut lumps: Vec<LumpInfo> = bsp
        .lumps
        .iter()
        .enumerate()
        .map(|(i, l)| LumpInfo {
            name: LUMP_NAMES[i],
            length: l.length,
            percent: if total > 0 { l.length as f32 / total as f32 * 100.0 } else { 0.0 },
        })
        .collect();
    lumps.sort_by(|a, b| b.length.cmp(&a.length));

    let rendered = render::top_down(&bsp, &ents.spawns, opts);
    let findings = findings(&summary, &ents, &bsp, slots);

    let mut textures: Vec<String> = bsp.textures.iter().map(|t| t.name.clone()).collect();
    textures.sort();
    textures.dedup();

    Ok(MapDetail {
        summary,
        sky: ents.sky.clone(),
        wads: ents.wads.clone(),
        texture_count: bsp.textures.len(),
        embedded_textures: bsp.textures.iter().filter(|t| t.embedded).count(),
        textures,
        face_count: bsp.faces.len(),
        vertex_count: bsp.vertices.len(),
        model_count: bsp.models.len(),
        histogram: ents.histogram.clone(),
        lumps,
        findings,
        svg: rendered.svg,
        polygons: rendered.polygons,
    })
}

pub fn thumbnail(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    let parsed = entities::parse(&bsp.entities_raw);
    let ents = entities::summarize(&parsed);
    Ok(render::top_down(&bsp, &ents.spawns, RenderOptions::thumbnail()).svg)
}
