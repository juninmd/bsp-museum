//! Auditoria da pasta inteira: roda o diagnóstico completo em cada mapa e
//! aponta arquivos duplicados (mesmo conteúdo com nomes diferentes).
//!
//! Devolve só dados; quem formata (Markdown/CSV/HTML, no idioma da UI) é o
//! frontend — assim o relatório sai traduzido sem duplicar texto aqui.

use crate::catalog::{self, Finding};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct AuditRow {
    pub name: String,
    pub path: String,
    pub file_size: u64,
    pub mode: crate::bsp::entities::GameMode,
    pub ct_spawns: usize,
    pub t_spawns: usize,
    pub bsp_version: i32,
    pub error: Option<String>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    pub dir: String,
    pub slots: usize,
    pub rows: Vec<AuditRow>,
    /// grupos de caminhos com o mesmo conteúdo
    pub duplicates: Vec<Vec<String>>,
}

/// FNV-1a de 64 bits: barato, sem dependência, e estável entre execuções
/// (o `DefaultHasher` da std não promete isso).
pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn audit_one(path: &Path, slots: usize) -> AuditRow {
    let summary = catalog::summarize_file(path);
    let mut row = AuditRow {
        name: summary.name.clone(),
        path: summary.path.clone(),
        file_size: summary.file_size,
        mode: summary.mode,
        ct_spawns: summary.ct_spawns,
        t_spawns: summary.t_spawns,
        bsp_version: summary.bsp_version,
        error: summary.error.clone(),
        findings: Vec::new(),
    };
    if row.error.is_some() {
        return row;
    }
    match catalog::analyze(path, slots) {
        Ok(a) => row.findings = a.findings,
        Err(e) => row.error = Some(e),
    }
    row
}

/// Grupos de arquivos idênticos. Só lê o conteúdo de quem tem tamanho igual a
/// outro — a maioria dos mapas nem chega a ser relida.
pub fn find_duplicates(files: &[(String, u64)]) -> Vec<Vec<String>> {
    let mut by_size: HashMap<u64, Vec<&str>> = HashMap::new();
    for (path, size) in files {
        by_size.entry(*size).or_default().push(path);
    }
    let mut groups: Vec<Vec<String>> = Vec::new();
    for (_, same_size) in by_size.into_iter().filter(|(_, v)| v.len() > 1) {
        let mut by_hash: HashMap<u64, Vec<String>> = HashMap::new();
        for path in same_size {
            if let Ok(bytes) = std::fs::read(path) {
                by_hash.entry(fnv64(&bytes)).or_default().push(path.to_string());
            }
        }
        groups.extend(by_hash.into_values().filter(|v| v.len() > 1));
    }
    for g in &mut groups {
        g.sort();
    }
    groups.sort();
    groups
}

pub fn run(dir: &Path, slots: usize) -> AuditReport {
    let files = catalog::find_bsp_files(dir);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let rows: Vec<AuditRow> = if files.len() < 2 || workers < 2 {
        files.iter().map(|p| audit_one(p, slots)).collect()
    } else {
        let chunk = files.len().div_ceil(workers);
        let mut parts: Vec<Vec<AuditRow>> = Vec::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = files
                .chunks(chunk)
                .map(|slice| scope.spawn(move || slice.iter().map(|p| audit_one(p, slots)).collect::<Vec<_>>()))
                .collect();
            for h in handles {
                parts.push(h.join().unwrap_or_default());
            }
        });
        parts.into_iter().flatten().collect()
    };
    let sizes: Vec<(String, u64)> = rows.iter().map(|r| (r.path.clone(), r.file_size)).collect();
    AuditReport { dir: dir.to_string_lossy().to_string(), slots, duplicates: find_duplicates(&sizes), rows }
}
