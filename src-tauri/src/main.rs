// Sem console extra no Windows quando roda em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audit;
mod bsp;
mod catalog;
mod compare;
mod diagnostics;
mod entity_list;
mod mdl;
mod radar;
mod resources;
#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_novos;

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
    /// índice persistente da varredura (resumo de cada mapa + tamanho + data)
    index_file: PathBuf,
}

/// Anotação do usuário sobre um mapa; a chave é o nome do arquivo (sem pasta),
/// então sobrevive a mover o acervo de lugar.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Annotation {
    #[serde(default)]
    favorite: bool,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    last_dir: Option<String>,
    #[serde(default = "default_slots")]
    slots: usize,
    /// "pt" | "en"; `None` = decidir pelo idioma do sistema
    #[serde(default)]
    lang: Option<String>,
    /// "dark" | "light"; `None` = decidir pelo sistema
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    annotations: HashMap<String, Annotation>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { last_dir: None, slots: default_slots(), lang: None, theme: None, annotations: HashMap::new() }
    }
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

/// Roda trabalho pesado fora da thread da UI: sem isso a janela congela durante
/// a varredura/auditoria de uma pasta grande.
async fn blocking<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job).await.map_err(|e| format!("tarefa interrompida: {e}"))
}

#[tauri::command]
async fn scan_maps(state: tauri::State<'_, AppState>, dir: String) -> Result<Vec<MapSummary>, String> {
    let path = PathBuf::from(&dir);
    if !path.is_dir() {
        return Err(format!("não é uma pasta: {dir}"));
    }
    let index_file = state.index_file.clone();
    blocking(move || {
        let mut cache: catalog::IndexCache = std::fs::read_to_string(&index_file)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        let (maps, _hits) = catalog::scan_cached(&path, &mut cache);
        if let Ok(raw) = serde_json::to_string(&cache) {
            if let Some(parent) = index_file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&index_file, raw);
        }
        maps
    })
    .await
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
async fn map_detail(path: String, slots: usize) -> Result<MapDetail, String> {
    blocking(move || catalog::detail(&PathBuf::from(path), bsp::render::RenderOptions::detail(), slots.max(2))).await?
}

#[tauri::command]
async fn map_mesh(path: String) -> Result<catalog::MeshDetail, String> {
    blocking(move || catalog::mesh(&PathBuf::from(path))).await?
}

/// Lista de entidades (com posição) para o painel clicável.
#[tauri::command]
async fn map_entities(path: String) -> Result<Vec<entity_list::EntityRow>, String> {
    blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("não leu o arquivo: {e}"))?;
        let bsp = bsp::Bsp::parse(&bytes).map_err(|e| e.to_string())?;
        let parsed = bsp::entities::parse(&bsp.entities_raw);
        Ok(entity_list::list(&bsp, &parsed))
    })
    .await?
}

/// Auditoria da pasta inteira (diagnóstico completo + duplicados).
#[tauri::command]
async fn audit_folder(dir: String, slots: usize) -> Result<audit::AuditReport, String> {
    let path = PathBuf::from(&dir);
    if !path.is_dir() {
        return Err(format!("não é uma pasta: {dir}"));
    }
    blocking(move || audit::run(&path, slots.max(2))).await
}

#[tauri::command]
async fn compare_maps(a: String, b: String, slots: usize) -> Result<compare::Comparison, String> {
    blocking(move || compare::compare(&PathBuf::from(a), &PathBuf::from(b), slots.max(2))).await?
}

/// Prévia do radar (PNG em data URL).
#[tauri::command]
async fn map_radar(path: String) -> Result<String, String> {
    blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("não leu o arquivo: {e}"))?;
        let bsp = bsp::Bsp::parse(&bytes).map_err(|e| e.to_string())?;
        let radar = radar::render(&bsp, 512).ok_or("mapa sem modelo 0")?;
        let png = radar.png().ok_or("falha ao codificar o radar")?;
        use base64::Engine as _;
        Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
    })
    .await?
}

/// Grava `overviews/<mapa>.bmp|.png|.txt` dentro de `out_dir` e devolve os caminhos.
#[tauri::command]
async fn export_radar(path: String, out_dir: String) -> Result<Vec<String>, String> {
    blocking(move || {
        let map = PathBuf::from(&path);
        let name = map.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let bytes = std::fs::read(&map).map_err(|e| format!("não leu o arquivo: {e}"))?;
        let bsp = bsp::Bsp::parse(&bytes).map_err(|e| e.to_string())?;
        let radar = radar::render(&bsp, 512).ok_or("mapa sem modelo 0")?;
        let dir = PathBuf::from(&out_dir).join("overviews");
        std::fs::create_dir_all(&dir).map_err(|e| format!("não criou {}: {e}", dir.display()))?;
        let mut written = Vec::new();
        let mut put = |file: String, data: Vec<u8>| -> Result<(), String> {
            let target = dir.join(file);
            std::fs::write(&target, data).map_err(|e| format!("não gravou {}: {e}", target.display()))?;
            written.push(target.to_string_lossy().to_string());
            Ok(())
        };
        put(format!("{name}.bmp"), radar.bmp())?;
        put(format!("{name}.png"), radar.png().ok_or("falha ao codificar o radar")?)?;
        put(format!("{name}.txt"), radar.overview_txt(&name).into_bytes())?;
        Ok(written)
    })
    .await?
}

/// Grava um arquivo de texto (planta SVG, relatório de auditoria, lista de FastDL).
#[tauri::command]
fn export_text(target: String, content: String) -> Result<(), String> {
    std::fs::write(&target, content).map_err(|e| format!("não gravou {target}: {e}"))
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
        .unwrap_or_default()
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
            let index_file = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| PathBuf::from(".cache"))
                .join("index.json");
            app.manage(AppState { cache: Mutex::new(Cache::default()), cache_dir, config_file, index_file });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_maps,
            map_thumbnail,
            map_detail,
            map_mesh,
            map_entities,
            audit_folder,
            compare_maps,
            map_radar,
            export_radar,
            export_text,
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
