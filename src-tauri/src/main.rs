// Sem console extra no Windows quando roda em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bsp;
mod catalog;
mod mdl;
#[cfg(test)]
mod tests;

use catalog::{MapDetail, MapSummary};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

#[derive(Default)]
struct Cache {
    /// path -> SVG da miniatura, memorizado enquanto o app está aberto
    thumbs: HashMap<String, String>,
}

struct AppState {
    cache: Mutex<Cache>,
    cache_dir: PathBuf,
    config_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Settings {
    last_dir: Option<String>,
    #[serde(default = "default_slots")]
    slots: usize,
}

fn default_slots() -> usize {
    32
}

/// Chave de cache: caminho + tamanho + mtime. Mapa recompilado invalida sozinho.
fn cache_key(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(&mut hasher);
        if let Ok(modified) = meta.modified() {
            if let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH) {
                since.as_secs().hash(&mut hasher);
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

#[tauri::command]
fn scan_maps(dir: String) -> Result<Vec<MapSummary>, String> {
    let path = PathBuf::from(&dir);
    if !path.is_dir() {
        return Err(format!("não é uma pasta: {dir}"));
    }
    Ok(catalog::scan(&path))
}

#[tauri::command]
fn map_thumbnail(state: tauri::State<'_, AppState>, path: String) -> Result<String, String> {
    if let Ok(cache) = state.cache.lock() {
        if let Some(svg) = cache.thumbs.get(&path) {
            return Ok(svg.clone());
        }
    }

    let file = PathBuf::from(&path);
    let disk = state.cache_dir.join(format!("{}.svg", cache_key(&file)));
    if let Ok(svg) = std::fs::read_to_string(&disk) {
        if let Ok(mut cache) = state.cache.lock() {
            cache.thumbs.insert(path, svg.clone());
        }
        return Ok(svg);
    }

    let svg = catalog::thumbnail(&file)?;
    let _ = std::fs::create_dir_all(&state.cache_dir);
    let _ = std::fs::write(&disk, &svg);
    if let Ok(mut cache) = state.cache.lock() {
        cache.thumbs.insert(path, svg.clone());
    }
    Ok(svg)
}

#[tauri::command]
fn map_detail(path: String, slots: usize) -> Result<MapDetail, String> {
    catalog::detail(&PathBuf::from(path), bsp::render::RenderOptions::detail(), slots.max(2))
}

#[tauri::command]
fn map_mesh(path: String) -> Result<catalog::MeshDetail, String> {
    catalog::mesh(&PathBuf::from(path))
}

#[tauri::command]
fn export_svg(target: String, svg: String) -> Result<(), String> {
    std::fs::write(&target, svg).map_err(|e| format!("não gravou {target}: {e}"))
}

#[tauri::command]
fn list_model_dirs(root: String) -> Result<Vec<catalog::ModelDir>, String> {
    let path = PathBuf::from(&root);
    if !path.is_dir() {
        return Err(format!("não é uma pasta: {root}"));
    }
    Ok(catalog::list_model_dirs(&path))
}

#[tauri::command]
fn list_models(dir: String) -> Result<Vec<String>, String> {
    Ok(catalog::list_models(&PathBuf::from(dir)))
}

#[tauri::command]
fn load_model(path: String) -> Result<catalog::MdlSummary, String> {
    catalog::load_model(&PathBuf::from(path))
}

#[tauri::command]
fn load_sequence(path: String, index: usize) -> Result<mdl::SeqFrames, String> {
    catalog::load_sequence(&PathBuf::from(path), index)
}

#[tauri::command]
fn load_settings(state: tauri::State<'_, AppState>) -> Settings {
    std::fs::read_to_string(&state.config_file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| Settings { last_dir: None, slots: default_slots() })
}

#[tauri::command]
fn save_settings(state: tauri::State<'_, AppState>, settings: Settings) -> Result<(), String> {
    if let Some(parent) = state.config_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&state.config_file, raw).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_cache(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    let mut removed = 0;
    if let Ok(entries) = std::fs::read_dir(&state.cache_dir) {
        for entry in entries.flatten() {
            if std::fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    if let Ok(mut cache) = state.cache.lock() {
        cache.thumbs.clear();
    }
    Ok(removed)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let cache_dir = app.path().app_cache_dir().unwrap_or_else(|_| PathBuf::from(".cache")).join("thumbs");
            let config_file = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("settings.json");
            let _ = std::fs::create_dir_all(&cache_dir);
            app.manage(AppState { cache: Mutex::new(Cache::default()), cache_dir, config_file });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_maps,
            map_thumbnail,
            map_detail,
            map_mesh,
            export_svg,
            list_model_dirs,
            list_models,
            load_model,
            load_sequence,
            load_settings,
            save_settings,
            clear_cache
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o bsp-museum");
}
