//! Tudo que um mapa precisa para rodar num servidor com FastDL: o `.bsp`, WADs,
//! céu, modelos, sprites, sons e os arquivos acessórios (`.res`, `.txt`, overview).
//!
//! Serve a duas telas: a lista "o que o jogador baixa" (com o tamanho total) e
//! a regra de diagnóstico `recurso-ausente` — a mesma varredura, um só lugar.

use crate::bsp::entities::{Entity, EntitySummary};
use crate::bsp::wad;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

/// WADs que acompanham o jogo: o cliente já tem, não entram no FastDL.
const STOCK_WADS: [&str; 10] = [
    "halflife.wad",
    "liquids.wad",
    "xeno.wad",
    "decals.wad",
    "cstrike.wad",
    "cs_dust.wad",
    "cs_havana.wad",
    "cs_italy.wad",
    "cs_office.wad",
    "cs_assault.wad",
];

const SKY_SUFFIXES: [&str; 6] = ["up", "dn", "lf", "rt", "ft", "bk"];

#[derive(Debug, Clone, Serialize)]
pub struct ResourceItem {
    /// mapa · res · txt · overview · wad · sky · model · sprite · sound · extra
    pub kind: &'static str,
    /// caminho relativo ao mod, com `/`
    pub path: String,
    pub size: u64,
    pub found: bool,
    /// existe só no `valve/` ou é WAD do jogo: o cliente já tem, não precisa baixar
    pub shared: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceReport {
    pub items: Vec<ResourceItem>,
    /// soma do que o jogador precisa baixar (achado, não padrão)
    pub download_size: u64,
    pub download_count: usize,
    /// referenciado pelo mapa mas ausente no disco
    pub missing: usize,
}

fn locate(mod_dir: &Path, rel: &str, kind: &'static str) -> ResourceItem {
    let rel = rel.trim().replace('\\', "/");
    let found = wad::find_asset(mod_dir, &rel);
    let (size, shared) = match &found {
        Some(path) => {
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            (size, !path.starts_with(mod_dir))
        }
        None => (0, false),
    };
    ResourceItem { kind, path: rel, size, found: found.is_some(), shared }
}

/// Coleta os recursos referenciados pelo mapa. `map_path` é `<mod>/maps/<nome>.bsp`.
pub fn collect(map_path: &Path, ents: &EntitySummary, entities: &[Entity]) -> ResourceReport {
    let mod_dir = wad::mod_dir_of(map_path);
    let name = map_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut items: Vec<ResourceItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut add = |item: ResourceItem, items: &mut Vec<ResourceItem>| {
        if seen.insert(item.path.to_ascii_lowercase()) {
            items.push(item);
        }
    };

    // o próprio mapa
    let bsp_size = std::fs::metadata(map_path).map(|m| m.len()).unwrap_or(0);
    add(
        ResourceItem { kind: "mapa", path: format!("maps/{name}.bsp"), size: bsp_size, found: true, shared: false },
        &mut items,
    );

    for (ext, kind) in [("res", "res"), ("txt", "txt")] {
        let item = locate(&mod_dir, &format!("maps/{name}.{ext}"), kind);
        if item.found {
            add(item, &mut items);
        }
    }
    for ext in ["txt", "bmp", "tga"] {
        let item = locate(&mod_dir, &format!("overviews/{name}.{ext}"), "overview");
        if item.found {
            add(item, &mut items);
        }
    }

    // arquivos listados no .res do mapa (um caminho por linha, `//` comenta)
    if let Some(res_path) = wad::find_asset(&mod_dir, &format!("maps/{name}.res")) {
        if let Ok(text) = std::fs::read_to_string(res_path) {
            for line in text.lines() {
                let line = line.split("//").next().unwrap_or("").trim();
                if !line.is_empty() {
                    add(locate(&mod_dir, line, "extra"), &mut items);
                }
            }
        }
    }

    for wad_name in &ents.wads {
        let mut item = locate(&mod_dir, wad_name, "wad");
        if STOCK_WADS.contains(&wad_name.to_ascii_lowercase().as_str()) {
            item.shared = true;
        }
        add(item, &mut items);
    }

    if let Some(sky) = ents.sky.as_deref().filter(|s| !s.is_empty()) {
        for suffix in SKY_SUFFIXES {
            add(locate(&mod_dir, &format!("gfx/env/{sky}{suffix}.tga"), "sky"), &mut items);
        }
    }

    for entity in entities {
        if let Some(model) = entity.get("model") {
            let lower = model.to_ascii_lowercase();
            if lower.ends_with(".mdl") {
                add(locate(&mod_dir, model, "model"), &mut items);
            } else if lower.ends_with(".spr") {
                add(locate(&mod_dir, model, "sprite"), &mut items);
            }
        }
        if entity.get("classname").map(String::as_str) == Some("ambient_generic") {
            if let Some(msg) = entity.get("message") {
                let lower = msg.to_ascii_lowercase();
                if lower.ends_with(".wav") || lower.ends_with(".mp3") {
                    add(locate(&mod_dir, &format!("sound/{}", msg.trim_start_matches('/')), "sound"), &mut items);
                }
            }
        }
    }

    let download: Vec<&ResourceItem> = items.iter().filter(|i| i.found && !i.shared).collect();
    ResourceReport {
        download_size: download.iter().map(|i| i.size).sum(),
        download_count: download.len(),
        missing: items.iter().filter(|i| !i.found && matches!(i.kind, "model" | "sprite" | "sound" | "extra")).count(),
        items,
    }
}

/// Lista de FastDL: um caminho por linha, só o que o jogador precisa baixar.
pub fn fastdl_list(report: &ResourceReport) -> String {
    let mut out = String::new();
    for item in report.items.iter().filter(|i| i.found && !i.shared) {
        out.push_str(&item.path);
        out.push('\n');
    }
    out
}
