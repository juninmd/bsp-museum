use crate::bsp::entities::{self, EntitySummary, GameMode};
use crate::bsp::render::{self, RenderOptions};
use crate::bsp::wad;
use crate::bsp::{Bsp, Lump, LUMP_ENTITIES, LUMP_LIGHTING, LUMP_MODELS, LUMP_NAMES, LUMP_TEXTURES};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Bounds {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub size: [f32; 3],
}

/// Achado barato (id + severidade), calculado na varredura sem abrir o BSP inteiro.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub id: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// regras que dá para decidir só com entidades + cabeçalho (sem `poucos-spawns`,
    /// que depende do nº de slots escolhido na UI)
    #[serde(default)]
    pub problems: Vec<Problem>,
    /// versão do BSP (30 GoldSrc, 29 Quake)
    #[serde(default)]
    pub bsp_version: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LumpInfo {
    pub name: &'static str,
    pub length: usize,
    pub percent: f32,
}

pub use crate::diagnostics::{Finding, Severity};

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
    pub resources: crate::resources::ResourceReport,
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
        problems: Vec::new(),
        bsp_version: 0,
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
        Ok((version, lumps)) => {
            summary.bsp_version = version;
            lumps
        }
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
    summary.problems = crate::diagnostics::entity_findings(&summary, &ents, 0)
        .into_iter()
        .map(|f| Problem { id: f.id.to_string(), severity: f.severity })
        .collect();
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
    let headnode = cur.i32().ok()?;
    cur.skip(12).ok()?;
    let visleafs = cur.i32().ok()?;
    let first_face = cur.i32().ok()?;
    let num_faces = cur.i32().ok()?;
    Some(crate::bsp::Model { mins, maxs, origin, headnode, visleafs, first_face, num_faces })
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

/// Índice persistente da varredura: o resumo de cada mapa, guardado com tamanho e
/// data do arquivo. Reabrir uma pasta de 2000 mapas só relê o que mudou.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexCache {
    /// muda quando o formato do `MapSummary` muda: índice antigo é descartado
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub entries: HashMap<String, IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub size: u64,
    pub mtime: u64,
    pub summary: MapSummary,
}

pub const INDEX_VERSION: u32 = 2;

fn stamp(path: &Path) -> (u64, u64) {
    let meta = std::fs::metadata(path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (size, mtime)
}

/// Varredura que reaproveita o índice: só os arquivos novos ou alterados são lidos.
/// Devolve os resumos e quantos vieram do cache.
pub fn scan_cached(dir: &Path, cache: &mut IndexCache) -> (Vec<MapSummary>, usize) {
    if cache.version != INDEX_VERSION {
        *cache = IndexCache { version: INDEX_VERSION, entries: HashMap::new() };
    }
    let files = find_bsp_files(dir);
    let mut slots: Vec<Option<MapSummary>> = Vec::with_capacity(files.len());
    let mut stale: Vec<(usize, PathBuf)> = Vec::new();
    for (i, path) in files.iter().enumerate() {
        let key = path.to_string_lossy().to_string();
        let (size, mtime) = stamp(path);
        match cache.entries.get(&key) {
            Some(entry) if entry.size == size && entry.mtime == mtime => slots.push(Some(entry.summary.clone())),
            _ => {
                slots.push(None);
                stale.push((i, path.clone()));
            }
        }
    }
    let hits = files.len() - stale.len();

    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let fresh: Vec<(usize, MapSummary)> = if stale.len() < 2 || workers < 2 {
        stale.iter().map(|(i, p)| (*i, summarize_file(p))).collect()
    } else {
        let chunk = stale.len().div_ceil(workers);
        let mut parts: Vec<Vec<(usize, MapSummary)>> = Vec::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = stale
                .chunks(chunk)
                .map(|slice| scope.spawn(move || slice.iter().map(|(i, p)| (*i, summarize_file(p))).collect::<Vec<_>>()))
                .collect();
            for h in handles {
                parts.push(h.join().unwrap_or_default());
            }
        });
        parts.into_iter().flatten().collect()
    };
    for (i, summary) in fresh {
        let (size, mtime) = stamp(&files[i]);
        cache.entries.insert(files[i].to_string_lossy().to_string(), IndexEntry { size, mtime, summary: summary.clone() });
        slots[i] = Some(summary);
    }
    (slots.into_iter().flatten().collect(), hits)
}

/// Todas as regras que não precisam olhar o disco: entidades + BSP.
pub fn findings(summary: &MapSummary, ents: &EntitySummary, bsp: &Bsp, slots: usize) -> Vec<Finding> {
    let mut out = crate::diagnostics::entity_findings(summary, ents, slots);
    out.extend(crate::diagnostics::bsp_findings(ents, bsp));
    out
}

/// Mapa aberto por inteiro + achados completos (entidades, BSP e disco).
/// É o que o detalhe, a auditoria e a comparação compartilham.
pub struct Analysis {
    pub bsp: Bsp,
    pub parsed: Vec<entities::Entity>,
    pub ents: EntitySummary,
    pub summary: MapSummary,
    pub findings: Vec<Finding>,
    pub resources: crate::resources::ResourceReport,
}

pub fn analyze(path: &Path, slots: usize) -> Result<Analysis, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    drop(bytes);
    let parsed = entities::parse(&bsp.entities_raw);
    let ents = entities::summarize(&parsed);
    let mut summary = summarize_file(path);
    // O resumo leve não enxerga o lump de iluminação com precisão; aqui sim.
    summary.fullbright = bsp.lumps[LUMP_LIGHTING].length == 0;

    let resources = crate::resources::collect(path, &ents, &parsed);
    let mut findings = findings(&summary, &ents, &bsp, slots);
    findings.extend(crate::diagnostics::disk_findings(path, &ents, &bsp, &resources));
    findings.sort_by_key(|f| f.severity);
    Ok(Analysis { bsp, parsed, ents, summary, findings, resources })
}

pub fn lump_infos(bsp: &Bsp) -> Vec<LumpInfo> {
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
    lumps
}

pub fn detail(path: &Path, opts: RenderOptions, slots: usize) -> Result<MapDetail, String> {
    let Analysis { bsp, ents, summary, findings, resources, .. } = analyze(path, slots)?;
    let rendered = render::top_down(&bsp, &ents.spawns, opts);

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
        lumps: lump_infos(&bsp),
        findings,
        svg: rendered.svg,
        polygons: rendered.polygons,
        resources,
    })
}

pub fn thumbnail(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    let parsed = entities::parse(&bsp.entities_raw);
    let ents = entities::summarize(&parsed);
    Ok(render::top_down(&bsp, &ents.spawns, RenderOptions::thumbnail()).svg)
}

/// Textura embutida no BSP, decodificada para o frontend.
#[derive(Debug, Clone, Serialize)]
pub struct MeshTexture {
    pub name: String,
    /// `data:image/png;base64,...` ou `None` para textura que vem de WAD externo.
    pub png: Option<String>,
}

/// Os 6 lados do céu (formato `gfx/env` do GoldSrc/CS 1.6).
#[derive(Debug, Clone, Serialize)]
pub struct SkyBox {
    pub up: String,
    pub down: String,
    pub left: String,
    pub right: String,
    pub front: String,
    pub back: String,
}

/// Malha 3D pronta para o WebGL: triângulos não indexados em um array plano.
#[derive(Debug, Clone, Serialize)]
pub struct MeshDetail {
    /// xyz por vértice, 9 floats por triângulo.
    pub positions: Vec<f32>,
    /// uv por vértice, 6 floats por triângulo.
    pub uvs: Vec<f32>,
    /// índice de textura por triângulo; `u32::MAX` = sem imagem (só cor).
    pub texindex: Vec<u32>,
    /// texturas na ordem referenciada por `texindex`.
    pub textures: Vec<MeshTexture>,
    pub spawns: Vec<entities::SpawnPoint>,
    pub bounds: Option<Bounds>,
    /// céu do mapa (6 lados decodificados), quando o `gfx/env` existe.
    pub skybox: Option<SkyBox>,
    /// quantas texturas vieram de WAD (não das embutidas no BSP)
    pub wad_textures: usize,
    pub triangles: usize,
    pub skipped: usize,
    /// atlas de lightmaps (`data:image/png`), ausente em mapa fullbright
    pub lightmap: Option<String>,
    /// uv no atlas por vértice (6 floats por triângulo); vazio sem atlas
    pub lm_uvs: Vec<f32>,
    /// face do BSP de cada triângulo (`-1` = prop `.mdl`, sempre visível)
    pub tri_face: Vec<i32>,
    /// árvore + visibilidade para o viewer desenhar só o que a câmera enxerga
    pub pvs: Option<PvsData>,
    /// versão do BSP (30 GoldSrc, 29 Quake)
    pub bsp_version: i32,
}

/// Dados mínimos de PVS: o viewer acha a folha da câmera descendo a árvore e
/// liga só as faces das folhas visíveis.
#[derive(Debug, Clone, Serialize)]
pub struct PvsData {
    /// 4 floats por plano: nx, ny, nz, dist
    pub planes: Vec<f32>,
    /// 3 ints por nó: plano, filho 0, filho 1 (negativo = folha `-1 - n`)
    pub nodes: Vec<i32>,
    /// 4 ints por folha: contents, visofs, primeira marksurface, quantidade
    pub leaves: Vec<i32>,
    pub marksurfaces: Vec<u16>,
    /// lump de visibilidade em base64
    pub visibility: String,
    pub headnode: i32,
    pub visleafs: i32,
    /// faces do modelo 0 (as demais são brush entities, sempre desenhadas)
    pub world_first_face: i32,
    pub world_face_count: i32,
}

fn pvs_data(bsp: &Bsp) -> Option<PvsData> {
    use base64::Engine as _;
    let world = bsp.models.first()?;
    if bsp.nodes.is_empty() || bsp.leaves.len() < 2 || bsp.visibility.is_empty() || world.visleafs <= 0 {
        return None;
    }
    Some(PvsData {
        planes: bsp.planes.iter().flat_map(|p| [p.normal[0], p.normal[1], p.normal[2], p.dist]).collect(),
        nodes: bsp.nodes.iter().flat_map(|n| [n.plane, i32::from(n.children[0]), i32::from(n.children[1])]).collect(),
        leaves: bsp
            .leaves
            .iter()
            .flat_map(|l| [l.contents, l.visofs, i32::from(l.first_marksurface), i32::from(l.num_marksurfaces)])
            .collect(),
        marksurfaces: bsp.marksurfaces.clone(),
        visibility: base64::engine::general_purpose::STANDARD.encode(&bsp.visibility),
        headnode: world.headnode,
        visleafs: world.visleafs,
        world_first_face: world.first_face,
        world_face_count: world.num_faces,
    })
}

/// Decode de uma textura com cache: primeiro os pixels embutidos no BSP,
/// depois — nome em mãos — a textura do WAD externo. Retorna `(slot, w, h)` ou
/// `None` quando não há imagem nenhuma (a face fica com cor neutra).
fn texture_slot(
    cache: &mut HashMap<usize, Option<(usize, f32, f32)>>,
    textures: &mut Vec<MeshTexture>,
    bytes: &[u8],
    lump: Lump,
    raw: usize,
    raw_name: &str,
    wads: &mut wad::WadSet,
    quake: bool,
) -> Option<(usize, f32, f32)> {
    if let Some(known) = cache.get(&raw) {
        return *known;
    }
    let mut resolved = crate::bsp::texture_image_for(bytes, lump, raw, quake).map(|img| {
        let slot = textures.len();
        textures.push(MeshTexture { name: img.name, png: Some(img.png) });
        (slot, img.width as f32, img.height as f32)
    });
    if resolved.is_none() && !raw_name.is_empty() {
        if let Some((png, w, h)) = wads.resolve(raw_name) {
            let slot = textures.len();
            textures.push(MeshTexture { name: raw_name.to_string(), png: Some(png) });
            resolved = Some((slot, w as f32, h as f32));
        }
    }
    cache.insert(raw, resolved);
    resolved
}

/// Malha 3D do mapa. Diferente da planta, inclui tetos e paredes: é para ver o
/// estado do mapa andando por dentro dele.
pub fn mesh(path: &Path) -> Result<MeshDetail, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    let ents = entities::summarize(&entities::parse(&bsp.entities_raw));
    let tex_lump = bsp.lumps[LUMP_TEXTURES];
    let mut wads = wad::WadSet::for_map(path, &ents.wads);
    let skybox = crate::bsp::sky::load_sky(path, ents.sky.as_deref().unwrap_or("")).and_then(|s| {
        if let (Some(up), Some(down), Some(left), Some(right), Some(front), Some(back)) =
            (&s[0], &s[1], &s[2], &s[3], &s[4], &s[5])
        {
            Some(SkyBox {
                up: up.clone(),
                down: down.clone(),
                left: left.clone(),
                right: right.clone(),
                front: front.clone(),
                back: back.clone(),
            })
        } else {
            None
        }
    });

    let empty = |spawns, skybox| MeshDetail {
        positions: Vec::new(),
        uvs: Vec::new(),
        texindex: Vec::new(),
        textures: Vec::new(),
        spawns,
        bounds: None,
        skybox,
        wad_textures: 0,
        triangles: 0,
        skipped: 0,
        lightmap: None,
        lm_uvs: Vec::new(),
        tri_face: Vec::new(),
        pvs: None,
        bsp_version: bsp.version,
    };
    let Some(bounds) = bsp.bounds() else {
        return Ok(empty(ents.spawns, skybox));
    };

    let mut cache: HashMap<usize, Option<(usize, f32, f32)>> = HashMap::new();
    let mut textures: Vec<MeshTexture> = Vec::new();
    let mut wad_textures = 0usize;
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut texindex = Vec::new();
    let mut triangles = 0usize;
    let mut skipped = 0usize;
    let atlas = crate::bsp::light::build_atlas(&bsp);
    let mut lm_uvs: Vec<f32> = Vec::new();
    let mut tri_face: Vec<i32> = Vec::new();

    for (face_index, face) in bsp.faces.iter().enumerate() {
        if let Some(tex) = bsp.texture_of(face) {
            if render::is_invisible(&tex.name) {
                skipped += 1;
                continue;
            }
        }
        let Some(points) = bsp.face_polygon(face) else {
            skipped += 1;
            continue;
        };
        if points.len() < 3 {
            skipped += 1;
            continue;
        }

        let texinfo_ix = face.texinfo as usize;
        let miptex = bsp.texinfo_miptex.get(texinfo_ix).copied();
        let raw_name = miptex
            .and_then(|m| bsp.raw_texture_name.get(m as usize))
            .map(String::as_str)
            .unwrap_or("");
        let mut slot = None;
        if let Some(m) = miptex {
            slot = texture_slot(&mut cache, &mut textures, &bytes, tex_lump, m as usize, raw_name, &mut wads, bsp.is_quake());
        }
        // Índice de textura para o frontend, com sentinela para "sem imagem".
        let index = slot.map(|(s, _, _)| s as u32).unwrap_or(u32::MAX);
        // Eixos de textura: transformam cada vértice do mundo em UV.
        let vecs = bsp.texinfo_vecs.get(texinfo_ix).copied();

        let mut face_uv: Vec<[f32; 2]> = vec![[0.0, 0.0]; points.len()];
        let mut face_lm: Vec<[f32; 2]> = Vec::new();
        if let Some(v) = vecs {
            let (w, h) = slot.map(|(_, w, h)| (w, h)).unwrap_or((1.0, 1.0));
            for (i, p) in points.iter().enumerate() {
                let (s, t) = crate::bsp::light::st_of(&v, *p);
                face_uv[i] = [s / w, t / h];
                if let Some(atlas) = &atlas {
                    face_lm.push(atlas.uv(face_index, s, t));
                }
            }
        }
        if let Some(atlas) = &atlas {
            if face_lm.is_empty() {
                face_lm = vec![atlas.white_uv(); points.len()];
            }
        }

        // Fan triangulation: faces de brush do GoldSrc são convexas.
        for i in 1..points.len() - 1 {
            let a = points[0];
            let b = points[i];
            let c = points[i + 1];
            positions.extend_from_slice(&[a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]);
            let ua = face_uv[0];
            let ub = face_uv[i];
            let uc = face_uv[i + 1];
            uvs.extend_from_slice(&[ua[0], ua[1], ub[0], ub[1], uc[0], uc[1]]);
            if atlas.is_some() {
                let (la, lb, lc) = (face_lm[0], face_lm[i], face_lm[i + 1]);
                lm_uvs.extend_from_slice(&[la[0], la[1], lb[0], lb[1], lc[0], lc[1]]);
            }
            texindex.push(index);
            tri_face.push(face_index as i32);
            triangles += 1;
        }
    }

    // Conta quantas texturas vieram de WAD (não embutidas no BSP).
    let mut embedded_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in &bsp.textures {
        if t.embedded {
            embedded_names.insert(t.name.to_ascii_lowercase());
        }
    }
    for t in &textures {
        if !embedded_names.contains(&t.name.to_ascii_lowercase()) {
            wad_textures += 1;
        }
    }

    // Props: entidades com `.mdl` explícito no BSP (cycler, monster_generic
    // etc.) entram nos mesmos arrays da malha do brush — sem controle
    // separado na UI, o toggle "texturizado" que já existe liga tudo junto.
    // Arquivo `.mdl` ausente ou corrompido é pulado em silêncio: não pode
    // derrubar a leitura do mapa inteiro por causa de um prop.
    let mod_dir = wad::mod_dir_of(path);
    for inst in &ents.model_instances {
        let Some(model_path) = wad::find_asset(&mod_dir, &inst.model) else { continue };
        let Ok(model_bytes) = std::fs::read(&model_path) else { continue };
        let Ok(prop) = crate::mdl::parse(&model_bytes) else { continue };

        let base_tex = textures.len() as u32;
        for t in &prop.textures {
            textures.push(MeshTexture { name: t.name.clone(), png: Some(t.png.clone()) });
        }
        let place = crate::mdl::entity_transform(inst.origin, inst.angles);
        for v in prop.positions.chunks_exact(3) {
            let world = place([v[0], v[1], v[2]]);
            positions.extend_from_slice(&world);
        }
        uvs.extend_from_slice(&prop.uvs);
        texindex.extend(prop.texindex.iter().map(|t| base_tex + t));
        triangles += prop.texindex.len();
        // prop sem lightmap: aponta todo vértice para o texel branco do atlas.
        tri_face.extend(std::iter::repeat(-1).take(prop.texindex.len()));
        if let Some(atlas) = &atlas {
            let white = atlas.white_uv();
            for _ in 0..prop.texindex.len() * 3 {
                lm_uvs.extend_from_slice(&white);
            }
        }
    }

    let lightmap = atlas.as_ref().and_then(|a| crate::bsp::rgba_png(a.width, a.height, &a.rgba));
    if lightmap.is_none() {
        lm_uvs.clear();
    }

    Ok(MeshDetail {
        positions,
        uvs,
        texindex,
        textures,
        spawns: ents.spawns,
        bounds: bounds_of(Some(bounds)),
        skybox,
        wad_textures,
        triangles,
        skipped,
        lightmap,
        lm_uvs,
        tri_face,
        pvs: pvs_data(&bsp),
        bsp_version: bsp.version,
    })
}

// ------------------------------------------------------------- visualizador avulso

/// Pasta com `.mdl` (recursiva, agrupada pela subpasta imediata) — a árvore
/// que o visualizador avulso navega (`models/player`, `models/weapons`, a
/// raiz de `models/` etc.), sem depender de um `.bsp` aberto.
#[derive(Debug, Clone, Serialize)]
pub struct ModelDir {
    /// caminho relativo à raiz escolhida (`.` pra a própria raiz)
    pub name: String,
    pub path: String,
    pub count: usize,
}

fn find_mdl_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mdl")) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Agrupa os `.mdl` encontrados sob `root` pela pasta imediata que os contém.
pub fn list_model_dirs(root: &Path) -> Vec<ModelDir> {
    let mut counts: HashMap<PathBuf, usize> = HashMap::new();
    for file in find_mdl_files(root) {
        if let Some(parent) = file.parent() {
            *counts.entry(parent.to_path_buf()).or_insert(0) += 1;
        }
    }
    let mut out: Vec<ModelDir> = counts
        .into_iter()
        .map(|(path, count)| {
            let name = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            ModelDir { name: if name.is_empty() { ".".to_string() } else { name }, path: path.to_string_lossy().to_string(), count }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `.mdl` diretamente dentro de `dir` (não recursivo — cada `ModelDir` já é
/// uma pasta folha da árvore acima).
pub fn list_models(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<String> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mdl")))
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    out.sort();
    out
}

/// Modelo `.mdl` isolado, decodificado pro frontend — mesma malha não-indexada
/// de `MeshDetail`, mais os nomes de sequência (metadado, ver `mdl` module).
#[derive(Debug, Clone, Serialize)]
pub struct MdlSummary {
    pub positions: Vec<f32>,
    pub uvs: Vec<f32>,
    pub texindex: Vec<u32>,
    pub textures: Vec<MeshTexture>,
    pub sequences: Vec<String>,
}

pub fn load_model(path: &Path) -> Result<MdlSummary, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let model = crate::mdl::parse(&bytes).map_err(|e| e.to_string())?;
    Ok(MdlSummary {
        positions: model.positions,
        uvs: model.uvs,
        texindex: model.texindex,
        textures: model
            .textures
            .into_iter()
            .map(|t| MeshTexture { name: t.name, png: Some(t.png) })
            .collect(),
        sequences: model.sequences,
    })
}
