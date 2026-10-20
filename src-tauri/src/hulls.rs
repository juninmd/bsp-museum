//! Malha dos hulls de colisão para o viewer (comando `map_hulls`).
//!
//! A geometria vem de `bsp::hull` (clipnodes -> polígonos). Aqui só vira malha
//! indexada e **paginada**: o hull 1 de um mapa grande passa de 100 mil triângulos, então
//! o chamador pede uma janela (`offset`, `limit`) e segue por `next_offset` até acabar.
//! Cada janela traz seus próprios vértices (índices locais), então cada pedaço vira uma
//! `BufferGeometry` independente.

use crate::bsp::hull::{hull_size, HullSize, HULL_SIZES};
use crate::bsp::Bsp;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// Triângulos por janela, se o chamador não pedir outro tamanho.
pub const DEFAULT_LIMIT: usize = 100_000;
/// Teto absoluto por janela.
pub const MAX_LIMIT: usize = 400_000;
/// Teto de polígonos varridos por hull (mapa hostil).
const MAX_POLYGONS: usize = 600_000;

#[derive(Debug, Clone, Copy)]
pub struct HullQuery {
    /// 1 = em pé, 2 = grande, 3 = agachado
    pub hull: usize,
    /// modelo (0 = mundo; `N` = brush entity `*N`)
    pub model: usize,
    /// primeiro triângulo da janela
    pub offset: usize,
    pub limit: usize,
}

impl HullQuery {
    pub fn new(hull: usize) -> Self {
        Self { hull, model: 0, offset: 0, limit: DEFAULT_LIMIT }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HullMesh {
    pub hull: u8,
    pub model: u32,
    /// caixa do corpo (a árvore já está expandida por ela)
    pub size: HullSize,
    /// xyz por vértice (3 floats), vértices deduplicados dentro da janela
    pub positions: Vec<f32>,
    /// 3 índices por triângulo, locais à janela; enrolamento anti-horário visto do lado vazio
    pub indices: Vec<u32>,
    /// triângulos nesta janela
    pub triangles: usize,
    /// triângulos do hull inteiro
    pub total_triangles: usize,
    pub offset: usize,
    /// `offset` da próxima janela, ou `None` se esta foi a última
    pub next_offset: Option<usize>,
    /// a varredura bateu no orçamento de trabalho: o hull pode estar incompleto
    pub truncated: bool,
}

/// Hull `1..=3` do mundo, primeira janela.
pub fn hull_mesh(path: &Path, hull: usize) -> Result<HullMesh, String> {
    hull_mesh_with(path, HullQuery::new(hull))
}

pub fn hull_mesh_with(path: &Path, query: HullQuery) -> Result<HullMesh, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    drop(bytes);
    mesh_of(&bsp, query)
}

pub fn mesh_of(bsp: &Bsp, query: HullQuery) -> Result<HullMesh, String> {
    let size = hull_size(query.hull).ok_or_else(|| format!("hull {} inválido: use 1, 2 ou 3", query.hull))?;
    if bsp.models.get(query.model).is_none() {
        return Err(format!("o mapa não tem o modelo {}", query.model));
    }
    let polys = bsp
        .hull_polygons(query.model, query.hull, MAX_POLYGONS)
        .ok_or_else(|| format!("o modelo {} não tem hull {} (lump de clipnodes vazio ou raiz inválida)", query.model, query.hull))?;

    // triângulos em leque, na ordem de varredura (estável entre janelas)
    let total_triangles: usize = polys.polygons.iter().map(|p| p.len().saturating_sub(2)).sum();
    let limit = query.limit.clamp(1, MAX_LIMIT);
    let end = query.offset.saturating_add(limit).min(total_triangles);

    let mut positions: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut index_of: HashMap<[i64; 3], u32> = HashMap::new();
    let mut vertex = |p: [f32; 3]| -> u32 {
        // 1/32 de unidade: dedupe sem fundir vértices distintos de verdade
        let key = [(p[0] * 32.0).round() as i64, (p[1] * 32.0).round() as i64, (p[2] * 32.0).round() as i64];
        *index_of.entry(key).or_insert_with(|| {
            positions.extend_from_slice(&p);
            (positions.len() / 3 - 1) as u32
        })
    };
    let mut tri = 0usize;
    'all: for poly in &polys.polygons {
        for i in 1..poly.len().saturating_sub(1) {
            if tri >= end {
                break 'all;
            }
            if tri >= query.offset {
                let (a, b, c) = (vertex(poly[0]), vertex(poly[i]), vertex(poly[i + 1]));
                indices.extend_from_slice(&[a, b, c]);
            }
            tri += 1;
        }
    }

    let triangles = indices.len() / 3;
    Ok(HullMesh {
        hull: size.hull,
        model: query.model as u32,
        size,
        positions,
        indices,
        triangles,
        total_triangles,
        offset: query.offset,
        next_offset: (end < total_triangles).then_some(end),
        truncated: polys.truncated,
    })
}

/// Resumo barato dos hulls do modelo 0 (para o `MapDetail`).
#[derive(Debug, Clone, Serialize)]
pub struct HullInfo {
    pub hull: u8,
    pub name: &'static str,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// o modelo 0 tem raiz válida para este hull
    pub available: bool,
}

pub fn hull_infos(bsp: &Bsp) -> Vec<HullInfo> {
    HULL_SIZES
        .iter()
        .map(|s| {
            let head = bsp.models.first().map(|m| m.hull_heads[s.hull as usize - 1]).unwrap_or(-1);
            HullInfo {
                hull: s.hull,
                name: s.name,
                mins: s.mins,
                maxs: s.maxs,
                available: head >= 0 && (head as usize) < bsp.clipnodes.len(),
            }
        })
        .collect()
}
