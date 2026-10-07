//! Comparação lado a lado de dois mapas (ou de duas versões do mesmo mapa).

use crate::catalog::{self, Bounds, Severity};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Side {
    pub name: String,
    pub title: Option<String>,
    pub file_size: u64,
    pub mode: crate::bsp::entities::GameMode,
    pub bsp_version: i32,
    pub ct_spawns: usize,
    pub t_spawns: usize,
    pub entities: usize,
    pub faces: usize,
    pub vertices: usize,
    pub textures: usize,
    pub bounds: Option<Bounds>,
    pub fullbright: bool,
    pub findings: Vec<(String, Severity)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CountDiff {
    pub name: String,
    pub a: usize,
    pub b: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub a: Side,
    pub b: Side,
    /// tamanho de cada lump nos dois (só os que diferem), maior variação primeiro
    pub lumps: Vec<CountDiff>,
    /// classname -> contagem nos dois (só os que diferem)
    pub entities: Vec<CountDiff>,
    pub textures_only_a: Vec<String>,
    pub textures_only_b: Vec<String>,
    pub wads_only_a: Vec<String>,
    pub wads_only_b: Vec<String>,
}

struct Loaded {
    side: Side,
    lumps: Vec<(String, usize)>,
    entities: BTreeMap<String, usize>,
    textures: BTreeSet<String>,
    wads: BTreeSet<String>,
}

fn load(path: &Path, slots: usize) -> Result<Loaded, String> {
    let a = catalog::analyze(path, slots)?;
    let side = Side {
        name: a.summary.name.clone(),
        title: a.summary.title.clone(),
        file_size: a.summary.file_size,
        mode: a.summary.mode,
        bsp_version: a.bsp.version,
        ct_spawns: a.summary.ct_spawns,
        t_spawns: a.summary.t_spawns,
        entities: a.ents.total,
        faces: a.bsp.faces.len(),
        vertices: a.bsp.vertices.len(),
        textures: a.bsp.textures.len(),
        bounds: a.summary.bounds,
        fullbright: a.summary.fullbright,
        findings: a.findings.iter().map(|f| (f.id.to_string(), f.severity)).collect(),
    };
    Ok(Loaded {
        lumps: crate::bsp::LUMP_NAMES.iter().zip(a.bsp.lumps.iter()).map(|(n, l)| (n.to_string(), l.length)).collect(),
        entities: a.ents.histogram.iter().cloned().collect(),
        textures: a.bsp.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect(),
        wads: a.ents.wads.iter().map(|w| w.to_ascii_lowercase()).collect(),
        side,
    })
}

fn diff_counts(a: &BTreeMap<String, usize>, b: &BTreeMap<String, usize>) -> Vec<CountDiff> {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    let mut out: Vec<CountDiff> = keys
        .into_iter()
        .map(|k| CountDiff { name: k.clone(), a: a.get(k).copied().unwrap_or(0), b: b.get(k).copied().unwrap_or(0) })
        .filter(|d| d.a != d.b)
        .collect();
    out.sort_by(|x, y| x.a.abs_diff(x.b).cmp(&y.a.abs_diff(y.b)).reverse().then(x.name.cmp(&y.name)));
    out
}

pub fn compare(path_a: &Path, path_b: &Path, slots: usize) -> Result<Comparison, String> {
    let a = load(path_a, slots)?;
    let b = load(path_b, slots)?;
    let la: BTreeMap<String, usize> = a.lumps.iter().cloned().collect();
    let lb: BTreeMap<String, usize> = b.lumps.iter().cloned().collect();
    Ok(Comparison {
        lumps: diff_counts(&la, &lb),
        entities: diff_counts(&a.entities, &b.entities),
        textures_only_a: a.textures.difference(&b.textures).cloned().collect(),
        textures_only_b: b.textures.difference(&a.textures).cloned().collect(),
        wads_only_a: a.wads.difference(&b.wads).cloned().collect(),
        wads_only_b: b.wads.difference(&a.wads).cloned().collect(),
        a: a.side,
        b: b.side,
    })
}
