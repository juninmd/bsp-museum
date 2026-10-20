//! Explorador de arquivos do mod (cstrike, valve…): lista classificada por tipo e
//! camada, pré-visualização limitada e análise por formato. **Somente leitura**.
//!
//! Confinamento: todo caminho vindo do frontend passa por [`sanitize_rel`] (recusa
//! `..`, absoluto, `:`, NUL) e pela regra de `bsp::wad::find_asset` (mod primeiro, depois
//! o `valve/` irmão). O resultado ainda é canonicalizado e precisa estar dentro de uma
//! das duas raízes — link simbólico que foge é recusado; na listagem, links para fora
//! são pulados e links para pastas nunca são seguidos (evita ciclos).
//!
//! Camadas: `mod` (solto na pasta do mod), `valve` (solto em `valve/`) e `pak` (dentro
//! de um `.pak` da raiz de uma das duas). A busca por caminho virtual é simplificada:
//! solto no mod, solto no valve, pak do mod, pak do valve (os `.pak` do maior número
//! primeiro, como o Quake). A ordem real do filesystem do GoldSrc não foi conferida.

use crate::audio::{self, AudioData, AudioInfo};
use crate::bsp::wad::find_asset;
use crate::demo::{self, DemoInfo};
use crate::nav::{self, NavInfo};
use crate::pak::{self, PakListing};
use crate::textfiles::{self, DetailTextures, HudSprites, Liblist, MapCycle, TextDoc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Teto da pré-visualização (o chamador pode pedir menos).
pub const MAX_PREVIEW_BYTES: usize = 16 * 1024 * 1024;
/// Teto de arquivo carregado para análise de nav/demo em pak/etc.
pub const MAX_ANALYZE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TEXT_READ: usize = 4 * 1024 * 1024;
const MAX_WARNINGS: usize = 30;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Map,
    Wad,
    Model,
    Sprite,
    Sound,
    Image,
    Nav,
    Demo,
    Text,
    Pak,
    Dll,
    Cfg,
    Other,
}

const ALL_KINDS: [FileKind; 13] = [
    FileKind::Map,
    FileKind::Wad,
    FileKind::Model,
    FileKind::Sprite,
    FileKind::Sound,
    FileKind::Image,
    FileKind::Nav,
    FileKind::Demo,
    FileKind::Text,
    FileKind::Pak,
    FileKind::Dll,
    FileKind::Cfg,
    FileKind::Other,
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Mod,
    Valve,
    Pak,
}

/// De qual pasta raiz o arquivo vem (um `.pak` mora numa delas).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Root {
    Mod,
    Valve,
}

/// Classifica pelo nome (extensão, sem diferenciar maiúsculas).
pub fn classify(name: &str) -> FileKind {
    let ext = match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() || !e.is_empty() => e.to_ascii_lowercase(),
        _ => return FileKind::Other,
    };
    match ext.as_str() {
        "bsp" => FileKind::Map,
        "wad" => FileKind::Wad,
        "mdl" => FileKind::Model,
        "spr" => FileKind::Sprite,
        "wav" | "mp3" | "ogg" => FileKind::Sound,
        "bmp" | "tga" | "png" | "jpg" | "jpeg" | "gif" | "pcx" | "dds" => FileKind::Image,
        "nav" => FileKind::Nav,
        "dem" => FileKind::Demo,
        "txt" | "res" | "lst" | "log" | "html" | "htm" | "xml" | "md" | "json" | "csv" | "inf" => FileKind::Text,
        "pak" => FileKind::Pak,
        "dll" | "so" | "dylib" | "exe" => FileKind::Dll,
        "cfg" | "rc" | "ini" | "gam" | "scr" | "vdf" => FileKind::Cfg,
        _ => FileKind::Other,
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModFileEntry {
    /// caminho virtual, relativo à raiz e com `/` — para um item de pak é o caminho dentro do pak
    pub path: String,
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    pub layer: Layer,
    /// pasta raiz onde o arquivo (ou o pak que o contém) está
    pub origin: Root,
    /// só na camada `pak`: caminho do `.pak` relativo à raiz `origin`
    pub pak: Option<String>,
}

impl ModFileEntry {
    /// Caminho que o `read_mod_file`/`analyze_mod_file` abrem exatamente esta entrada:
    /// `pak0.pak/sound/x.wav` para itens de pak, o próprio `path` para arquivos soltos.
    pub fn open_path(&self) -> String {
        match &self.pak {
            Some(p) => format!("{p}/{}", self.path),
            None => self.path.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct KindTotal {
    pub kind: FileKind,
    pub count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModListing {
    pub mod_root: String,
    pub valve_root: Option<String>,
    pub entries: Vec<ModFileEntry>,
    pub totals: Vec<KindTotal>,
    /// `true` se a listagem parou em `max_entries`
    pub truncated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    /// lista também o conteúdo dos `.pak` da raiz de cada pasta
    pub include_paks: bool,
    pub max_entries: usize,
    pub max_depth: usize,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self { include_paks: true, max_entries: 200_000, max_depth: 24 }
    }
}

/// Identificação do que foi aberto.
#[derive(Debug, Clone, Serialize)]
pub struct ModFileInfo {
    pub path: String,
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    pub layer: Layer,
    pub origin: Root,
    pub pak: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilePreview {
    #[serde(flatten)]
    pub info: ModFileInfo,
    pub bytes_read: u64,
    pub truncated: bool,
    pub is_text: bool,
    /// conteúdo decodificado quando `is_text`
    pub text: Option<String>,
    /// conteúdo em base64 quando binário (o frontend monta o hex)
    pub base64: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Analysis {
    Audio(AudioInfo),
    Nav(NavInfo),
    Demo(DemoInfo),
    Pak(PakListing),
    Liblist(Liblist),
    HudSprites(HudSprites),
    DetailTextures(DetailTextures),
    MapCycle(MapCycle),
    Text(TextDoc),
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModFileAnalysis {
    #[serde(flatten)]
    pub info: ModFileInfo,
    pub analysis: Analysis,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub kind: Option<FileKind>,
    /// arquivo: tamanho; pasta: soma dos filhos
    pub size: u64,
    pub file_count: u64,
    /// camadas em que o caminho existe
    pub layers: Vec<Layer>,
    pub children: Vec<ModTreeNode>,
}

// ---------------------------------------------------------------------------------
// confinamento
// ---------------------------------------------------------------------------------

/// Normaliza um caminho relativo vindo do frontend e recusa tudo que sairia da raiz.
pub fn sanitize_rel(rel: &str) -> Result<String, String> {
    let r = rel.replace('\\', "/");
    if r.contains('\0') {
        return Err("caminho com byte nulo".to_string());
    }
    if r.starts_with('/') {
        return Err(format!("caminho absoluto não é permitido: {rel}"));
    }
    let mut parts: Vec<&str> = Vec::new();
    for c in r.split('/') {
        match c {
            "" | "." => {}
            ".." => return Err(format!("\"..\" não é permitido: {rel}")),
            c if c.contains(':') => return Err(format!("caminho com \":\" não é permitido: {rel}")),
            c => parts.push(c),
        }
    }
    if parts.is_empty() {
        return Err("caminho vazio".to_string());
    }
    Ok(parts.join("/"))
}

fn rel_in(root: &Path, p: &Path) -> Option<String> {
    let r = p.strip_prefix(root).ok()?;
    let parts: Vec<String> = r.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    Some(parts.join("/"))
}

fn warn(w: &mut Vec<String>, msg: String) {
    if w.len() < MAX_WARNINGS {
        w.push(msg);
    }
}

// ---------------------------------------------------------------------------------
// raízes
// ---------------------------------------------------------------------------------

/// A pasta do mod e (opcionalmente) a `valve`, já canonicalizadas.
pub struct ModRoots {
    mod_root: PathBuf,
    valve_root: Option<PathBuf>,
}

enum Resolved {
    Loose { path: PathBuf, rel: String, root: Root, size: u64 },
    Pak { pak_path: PathBuf, pak_rel: String, member: String, root: Root, size: u64 },
}

impl ModRoots {
    /// `valve_root = None` procura `valve/` ao lado da pasta do mod (mesma regra do
    /// `find_asset`); se a própria pasta do mod for `valve`, não há segunda camada.
    pub fn new(mod_root: &Path, valve_root: Option<&Path>) -> Result<Self, String> {
        let m = mod_root.canonicalize().map_err(|e| format!("pasta do mod inválida ({}): {e}", mod_root.display()))?;
        if !m.is_dir() {
            return Err(format!("{} não é uma pasta", mod_root.display()));
        }
        let v = match valve_root {
            Some(v) => Some(v.canonicalize().map_err(|e| format!("pasta valve inválida ({}): {e}", v.display()))?),
            None => m.parent().map(|p| p.join("valve")).and_then(|p| p.canonicalize().ok()),
        };
        let v = v.filter(|v| v.is_dir() && *v != m);
        Ok(Self { mod_root: m, valve_root: v })
    }

    fn bases(&self, only: Option<Root>) -> Vec<(Root, &Path)> {
        let mut v = vec![(Root::Mod, self.mod_root.as_path())];
        if let Some(p) = &self.valve_root {
            v.push((Root::Valve, p.as_path()));
        }
        v.retain(|(r, _)| only.is_none() || only == Some(*r));
        v
    }

    fn resolve_loose(&self, rel: &str, only: Option<Root>) -> Option<Resolved> {
        for (root, base) in self.bases(only) {
            let Some(found) = find_asset(base, rel) else { continue };
            let Ok(canon) = found.canonicalize() else { continue };
            if !canon.starts_with(base) {
                continue; // link simbólico fugindo da raiz (ou o fallback do find_asset para outra camada)
            }
            let Ok(meta) = std::fs::metadata(&canon) else { continue };
            return Some(Resolved::Loose { path: canon, rel: rel.to_string(), root, size: meta.len() });
        }
        None
    }

    /// `.pak` soltos na raiz, do maior número para o menor (`pak10` antes de `pak2`).
    fn root_paks(base: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(base)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pak")) && p.is_file())
            .collect();
        let key = |p: &PathBuf| {
            let n = p.file_name().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            (n.len(), n)
        };
        v.sort_by_key(|p| std::cmp::Reverse(key(p)));
        v
    }

    fn resolve_in_paks(&self, rel: &str, only: Option<Root>) -> Option<Resolved> {
        for (root, base) in self.bases(only) {
            for pak_path in Self::root_paks(base) {
                let Ok(canon) = pak_path.canonicalize() else { continue };
                if !canon.starts_with(base) {
                    continue;
                }
                let Ok(listing) = pak::list_pak_file(&canon) else { continue };
                if let Some(e) = pak::find_entry(&listing, rel) {
                    return Some(Resolved::Pak {
                        pak_rel: rel_in(base, &canon)?,
                        member: e.name.clone(),
                        pak_path: canon,
                        root,
                        size: e.size as u64,
                    });
                }
            }
        }
        None
    }

    /// `pak0.pak/sound/x.wav`: o primeiro componente que é um `.pak` existente separa
    /// o pak do membro.
    fn resolve_pak_qualified(&self, rel: &str, only: Option<Root>) -> Option<Resolved> {
        let comps: Vec<&str> = rel.split('/').collect();
        for i in 0..comps.len().saturating_sub(1) {
            if !comps[i].to_ascii_lowercase().ends_with(".pak") {
                continue;
            }
            let pak_rel = comps[..=i].join("/");
            let member = comps[i + 1..].join("/");
            let Some(Resolved::Loose { path, rel: pak_rel, root, .. }) = self.resolve_loose(&pak_rel, only) else {
                continue;
            };
            let Ok(listing) = pak::list_pak_file(&path) else { continue };
            if let Some(e) = pak::find_entry(&listing, &member) {
                return Some(Resolved::Pak { pak_path: path, pak_rel, member: e.name.clone(), root, size: e.size as u64 });
            }
        }
        None
    }

    fn resolve(&self, rel: &str, only: Option<Root>) -> Result<Resolved, String> {
        let rel = sanitize_rel(rel)?;
        self.resolve_loose(&rel, only)
            .or_else(|| self.resolve_pak_qualified(&rel, only))
            .or_else(|| self.resolve_in_paks(&rel, only))
            .ok_or_else(|| format!("\"{rel}\" não existe no mod (ou aponta para fora da raiz)"))
    }

    // ---- listagem ----

    pub fn list(&self, opts: &ListOptions) -> ModListing {
        let mut out = Vec::new();
        let mut warnings = Vec::new();
        let mut truncated = false;
        for (root, base) in self.bases(None) {
            let mut paks: Vec<(String, PathBuf)> = Vec::new();
            let mut w = Walk { base, root, opts, out: &mut out, warnings: &mut warnings, truncated: &mut truncated, paks: &mut paks };
            w.dir(base, "", 0);
            if opts.include_paks {
                for (pak_rel, pak_path) in paks {
                    if out.len() >= opts.max_entries {
                        truncated = true;
                        break;
                    }
                    match pak::list_pak_file(&pak_path) {
                        Ok(l) => {
                            for e in l.entries {
                                if out.len() >= opts.max_entries {
                                    truncated = true;
                                    break;
                                }
                                out.push(ModFileEntry {
                                    name: base_name(&e.name).to_string(),
                                    kind: classify(&e.name),
                                    size: e.size as u64,
                                    layer: Layer::Pak,
                                    origin: root,
                                    pak: Some(pak_rel.clone()),
                                    path: e.name,
                                });
                            }
                        }
                        Err(err) => warn(&mut warnings, format!("{pak_rel}: {err}")),
                    }
                }
            }
        }
        let mut totals = Vec::new();
        for k in ALL_KINDS {
            let (count, bytes) = out.iter().filter(|e| e.kind == k).fold((0u64, 0u64), |a, e| (a.0 + 1, a.1 + e.size));
            if count > 0 {
                totals.push(KindTotal { kind: k, count, bytes });
            }
        }
        ModListing {
            mod_root: self.mod_root.display().to_string(),
            valve_root: self.valve_root.as_ref().map(|p| p.display().to_string()),
            entries: out,
            totals,
            truncated,
            warnings,
        }
    }

    // ---- leitura ----

    fn info_of(&self, r: &Resolved) -> ModFileInfo {
        match r {
            Resolved::Loose { rel, root, size, .. } => ModFileInfo {
                path: rel.clone(),
                name: base_name(rel).to_string(),
                kind: classify(rel),
                size: *size,
                layer: match root {
                    Root::Mod => Layer::Mod,
                    Root::Valve => Layer::Valve,
                },
                origin: *root,
                pak: None,
            },
            Resolved::Pak { pak_rel, member, root, size, .. } => ModFileInfo {
                path: member.clone(),
                name: base_name(member).to_string(),
                kind: classify(member),
                size: *size,
                layer: Layer::Pak,
                origin: *root,
                pak: Some(pak_rel.clone()),
            },
        }
    }

    fn read_resolved(r: &Resolved, max: usize) -> Result<(Vec<u8>, bool), String> {
        match r {
            Resolved::Loose { path, size, .. } => {
                let f = std::fs::File::open(path).map_err(|e| format!("não abriu {}: {e}", path.display()))?;
                let mut buf = Vec::new();
                f.take(max as u64).read_to_end(&mut buf).map_err(|e| format!("erro de leitura: {e}"))?;
                let truncated = (buf.len() as u64) < *size;
                Ok((buf, truncated))
            }
            Resolved::Pak { pak_path, member, .. } => {
                let d = pak::read_pak_file_entry(pak_path, member, max)?;
                Ok((d.bytes, d.truncated))
            }
        }
    }

    /// Lê o arquivo inteiro para análise, recusando o que passa de `cap`.
    fn read_whole(r: &Resolved, info: &ModFileInfo, cap: u64) -> Result<Vec<u8>, String> {
        if info.size > cap {
            return Err(format!(
                "{} tem {:.1} MB, acima do limite de análise ({:.0} MB)",
                info.name,
                info.size as f64 / 1_048_576.0,
                cap as f64 / 1_048_576.0
            ));
        }
        Ok(Self::read_resolved(r, cap as usize)?.0)
    }

    /// Pré-visualização limitada. `only` restringe a uma das raízes (útil quando o mesmo
    /// caminho existe no mod e no valve).
    pub fn read(&self, rel: &str, max_bytes: usize, only: Option<Root>) -> Result<FilePreview, String> {
        let max = max_bytes.clamp(1, MAX_PREVIEW_BYTES);
        let r = self.resolve(rel, only)?;
        let info = self.info_of(&r);
        let (bytes, truncated) = Self::read_resolved(&r, max)?;
        let is_text = looks_like_text(info.kind, &bytes);
        let (text, base64) = if is_text {
            (Some(textfiles::decode_text(&bytes)), None)
        } else {
            use base64::Engine as _;
            (None, Some(base64::engine::general_purpose::STANDARD.encode(&bytes)))
        };
        Ok(FilePreview { info, bytes_read: bytes.len() as u64, truncated, is_text, text, base64 })
    }

    /// Áudio como `data:` URL para o `<audio>` (limite de [`audio::MAX_AUDIO_BYTES`]).
    pub fn read_audio(&self, rel: &str, only: Option<Root>) -> Result<AudioData, String> {
        let r = self.resolve(rel, only)?;
        let info = self.info_of(&r);
        let bytes = Self::read_whole(&r, &info, audio::MAX_AUDIO_BYTES)?;
        audio::audio_data_url(&bytes)
    }

    /// Analisa pelo formato: áudio, nav, demo, pak, liblist, hud/weapon, detail,
    /// mapcycle ou texto genérico (`maps/<mapa>.txt`, `motd.txt`…).
    pub fn analyze(&self, rel: &str, only: Option<Root>) -> Result<ModFileAnalysis, String> {
        let r = self.resolve(rel, only)?;
        let info = self.info_of(&r);
        let lname = info.name.to_ascii_lowercase();
        let analysis = if matches!(lname.rsplit_once('.'), Some((_, "wav" | "mp3"))) {
            let bytes = Self::read_whole(&r, &info, audio::MAX_AUDIO_BYTES)?;
            Analysis::Audio(audio::analyze_audio_bytes(&bytes, audio::DEFAULT_ENVELOPE)?)
        } else if info.kind == FileKind::Nav {
            let bytes = Self::read_whole(&r, &info, MAX_ANALYZE_BYTES)?;
            let mut n = nav::parse_nav(&bytes)?;
            let stem = lname.trim_end_matches(".nav");
            if let Ok(b) = self.resolve(&format!("maps/{stem}.bsp"), Some(info.origin)) {
                nav::check_bsp_size(&mut n, self.info_of(&b).size);
            }
            Analysis::Nav(n)
        } else if info.kind == FileKind::Demo {
            match &r {
                Resolved::Loose { path, .. } => Analysis::Demo(demo::parse_demo_file(path)?),
                Resolved::Pak { .. } => {
                    Analysis::Demo(demo::parse_demo(&Self::read_whole(&r, &info, MAX_ANALYZE_BYTES)?)?)
                }
            }
        } else if info.kind == FileKind::Pak {
            match &r {
                Resolved::Loose { path, .. } => Analysis::Pak(pak::list_pak_file(path)?),
                Resolved::Pak { .. } => {
                    Analysis::Pak(pak::list_pak_bytes(&Self::read_whole(&r, &info, MAX_ANALYZE_BYTES)?)?)
                }
            }
        } else if matches!(info.kind, FileKind::Text | FileKind::Cfg) {
            let (bytes, _) = Self::read_resolved(&r, MAX_TEXT_READ)?;
            if lname == "liblist.gam" {
                Analysis::Liblist(textfiles::parse_liblist(&bytes))
            } else if lname == "hud.txt" || (lname.starts_with("weapon_") && lname.ends_with(".txt")) {
                Analysis::HudSprites(textfiles::parse_hud_sprites(&bytes))
            } else if lname.ends_with("_detail.txt") {
                Analysis::DetailTextures(textfiles::parse_detail_textures(&bytes))
            } else if lname == "mapcycle.txt" {
                Analysis::MapCycle(textfiles::parse_mapcycle(&bytes))
            } else {
                Analysis::Text(textfiles::parse_text(&bytes, textfiles::MAX_TEXT_CHARS))
            }
        } else {
            Analysis::None
        };
        Ok(ModFileAnalysis { info, analysis })
    }
}

struct Walk<'a> {
    base: &'a Path,
    root: Root,
    opts: &'a ListOptions,
    out: &'a mut Vec<ModFileEntry>,
    warnings: &'a mut Vec<String>,
    truncated: &'a mut bool,
    paks: &'a mut Vec<(String, PathBuf)>,
}

impl Walk<'_> {
    fn dir(&mut self, dir: &Path, rel: &str, depth: usize) {
        let rd = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(e) => {
                warn(self.warnings, format!("não leu a pasta {}: {e}", if rel.is_empty() { "." } else { rel }));
                return;
            }
        };
        let mut items: Vec<(String, PathBuf)> =
            rd.flatten().map(|e| (e.file_name().to_string_lossy().to_string(), e.path())).collect();
        items.sort_by_key(|(n, _)| n.to_ascii_lowercase());
        for (name, path) in items {
            if self.out.len() >= self.opts.max_entries {
                *self.truncated = true;
                return;
            }
            let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let Ok(lmeta) = std::fs::symlink_metadata(&path) else { continue };
            let meta = if lmeta.file_type().is_symlink() {
                match path.canonicalize() {
                    Ok(c) if c.starts_with(self.base) => match std::fs::metadata(&c) {
                        Ok(m) if m.is_file() => m,
                        _ => {
                            warn(self.warnings, format!("link simbólico para pasta/inexistente não seguido: {child_rel}"));
                            continue;
                        }
                    },
                    _ => {
                        warn(self.warnings, format!("link simbólico para fora da raiz ignorado: {child_rel}"));
                        continue;
                    }
                }
            } else {
                lmeta
            };
            if meta.is_dir() {
                if depth + 1 < self.opts.max_depth {
                    self.dir(&path, &child_rel, depth + 1);
                } else {
                    warn(self.warnings, format!("profundidade máxima atingida em {child_rel}"));
                }
            } else if meta.is_file() {
                let kind = classify(&name);
                if kind == FileKind::Pak && self.opts.include_paks {
                    self.paks.push((child_rel.clone(), path.clone()));
                }
                self.out.push(ModFileEntry {
                    path: child_rel,
                    name,
                    kind,
                    size: meta.len(),
                    layer: match self.root {
                        Root::Mod => Layer::Mod,
                        Root::Valve => Layer::Valve,
                    },
                    origin: self.root,
                    pak: None,
                });
            }
        }
    }
}

fn looks_like_text(kind: FileKind, bytes: &[u8]) -> bool {
    if !matches!(kind, FileKind::Text | FileKind::Cfg | FileKind::Other) {
        return false;
    }
    let head = &bytes[..bytes.len().min(4096)];
    if head.contains(&0) {
        return false;
    }
    if kind != FileKind::Other {
        return true;
    }
    if head.is_empty() {
        return true;
    }
    let printable = head.iter().filter(|&&b| b == b'\n' || b == b'\r' || b == b'\t' || (0x20..0x7F).contains(&b) || b >= 0x80).count();
    printable * 100 >= head.len() * 95
}

// ---------------------------------------------------------------------------------
// árvore
// ---------------------------------------------------------------------------------

#[derive(Default)]
struct Builder {
    children: BTreeMap<String, Builder>,
    is_file: bool,
    kind: Option<FileKind>,
    size: u64,
    layers: BTreeSet<Layer>,
}

fn finish(name: &str, path: &str, b: Builder) -> ModTreeNode {
    let is_dir = !b.children.is_empty() || !b.is_file;
    let mut children: Vec<ModTreeNode> = b
        .children
        .into_iter()
        .map(|(n, c)| {
            let p = if path.is_empty() { n.clone() } else { format!("{path}/{n}") };
            finish(&n, &p, c)
        })
        .collect();
    children.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase())));
    let mut layers = b.layers;
    let (size, file_count) = if is_dir {
        for c in &children {
            layers.extend(c.layers.iter().copied());
        }
        (children.iter().map(|c| c.size).sum::<u64>(), children.iter().map(|c| c.file_count).sum::<u64>())
    } else {
        (b.size, 1)
    };
    ModTreeNode {
        name: name.to_string(),
        path: path.to_string(),
        is_dir,
        kind: if is_dir { None } else { b.kind },
        size,
        file_count,
        layers: layers.into_iter().collect(),
        children,
    }
}

/// Árvore de pastas/arquivos a partir da lista plana. O mesmo caminho em várias
/// camadas vira um nó só, com as camadas somadas (o tamanho é o da primeira entrada).
pub fn build_tree(entries: &[ModFileEntry]) -> ModTreeNode {
    let mut root = Builder::default();
    for e in entries {
        let mut node = &mut root;
        for part in e.path.split('/').filter(|p| !p.is_empty()) {
            node = node.children.entry(part.to_string()).or_default();
        }
        if !node.is_file {
            node.is_file = true;
            node.kind = Some(e.kind);
            node.size = e.size;
        }
        node.layers.insert(e.layer);
    }
    finish("", "", root)
}

// ---------------------------------------------------------------------------------
// atalhos por caminho (o que os comandos Tauri chamam)
// ---------------------------------------------------------------------------------

/// Lista o mod (e o `valve/` irmão, ou o informado) com opções padrão.
pub fn list_mod_files(mod_root: &Path, valve_root: Option<&Path>, opts: &ListOptions) -> Result<ModListing, String> {
    Ok(ModRoots::new(mod_root, valve_root)?.list(opts))
}

/// Pré-visualização de um arquivo do mod (solto no mod, solto no valve irmão, ou dentro
/// de um `.pak`; também aceita `pak0.pak/caminho/interno`). Lê no máximo `max_bytes`.
pub fn read_mod_file(root: &Path, rel: &str, max_bytes: usize) -> Result<FilePreview, String> {
    ModRoots::new(root, None)?.read(rel, max_bytes, None)
}

/// Como [`read_mod_file`], com `valve` explícito e restrição opcional de raiz.
pub fn read_mod_file_in(
    root: &Path,
    valve_root: Option<&Path>,
    rel: &str,
    max_bytes: usize,
    only: Option<Root>,
) -> Result<FilePreview, String> {
    ModRoots::new(root, valve_root)?.read(rel, max_bytes, only)
}

/// Análise por formato de um arquivo do mod.
pub fn analyze_mod_file(root: &Path, rel: &str) -> Result<ModFileAnalysis, String> {
    ModRoots::new(root, None)?.analyze(rel, None)
}

/// Áudio do mod como `data:` URL.
pub fn read_mod_audio(root: &Path, rel: &str) -> Result<AudioData, String> {
    ModRoots::new(root, None)?.read_audio(rel, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testutil::wav16;
    use crate::demo::build_demo;
    use crate::nav::testutil::{area, build_nav};
    use crate::pak::build_pak;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    /// Pasta temporária única, apagada ao sair de escopo.
    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("bsp-museum-modfiles-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Tmp(p.canonicalize().unwrap())
        }
        fn put(&self, rel: &str, data: &[u8]) {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, data).unwrap();
        }
        fn path(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// `game/cstrike` + `game/valve` com um pouco de tudo.
    fn fixture() -> Tmp {
        let t = Tmp::new();
        t.put("game/cstrike/maps/de_x.bsp", &[0u8; 1000]);
        let nav = build_nav(5, 1000, &["A"], &[area(1, [0.0; 3], [10.0, 10.0, 1.0])]);
        t.put("game/cstrike/maps/de_x.nav", &nav);
        t.put("game/cstrike/maps/de_x.txt", "Planta a bomba\r\nem A ou B".as_bytes());
        t.put("game/cstrike/sound/a.wav", &wav16(1, 11025, &[1000; 400], &[]));
        t.put("game/cstrike/liblist.gam", b"game \"Counter-Strike\"\r\ngamedll \"dlls\\mp.dll\"\r\n");
        t.put("game/cstrike/mapcycle.txt", b"de_x\nde_y 2 10\n");
        t.put("game/cstrike/sprites/hud.txt", b"1\nammo 640 640hud7 0 0 24 24\n");
        t.put("game/cstrike/maps/de_x_detail.txt", b"wall detail/d 2 2\n");
        t.put("game/cstrike/models/p.mdl", b"IDST....");
        t.put("game/cstrike/cstrike.wad", b"WAD3");
        t.put("game/cstrike/dlls/mp.dll", b"MZ\0\0");
        t.put("game/cstrike/autoexec.cfg", b"sv_gravity 800\n");
        t.put("game/cstrike/demos/x.dem", &build_demo("de_x", "cstrike", &[(0, "LOADING", 1.0, 5), (1, "Playback", 60.0, 3000)]));
        t.put(
            "game/cstrike/pak0.pak",
            &build_pak(&[("sound/pak/ping.wav", &wav16(1, 8000, &[5; 50], &[])), ("readme.txt", b"dentro do pak"), ("maps/de_p.bsp", b"BSP")]),
        );
        t.put("game/cstrike/semextensao", b"texto qualquer\nlinha 2\n");
        t.put("game/valve/halflife.wad", b"WAD3");
        t.put("game/valve/sound/a.wav", &wav16(1, 11025, &[5; 100], &[]));
        t.put("game/valve/liblist.gam", b"game \"Half-Life\"\n");
        t.put("game/valve/pak0.pak", &build_pak(&[("sound/v/pak.wav", b"RIFF")]));
        t
    }

    fn cs(t: &Tmp) -> PathBuf {
        t.path("game/cstrike")
    }

    #[test]
    fn classificacao_por_extensao() {
        let casos = [
            ("a.BSP", FileKind::Map),
            ("halflife.wad", FileKind::Wad),
            ("p.mdl", FileKind::Model),
            ("s.spr", FileKind::Sprite),
            ("a.wav", FileKind::Sound),
            ("a.MP3", FileKind::Sound),
            ("a.bmp", FileKind::Image),
            ("a.tga", FileKind::Image),
            ("de_x.nav", FileKind::Nav),
            ("x.dem", FileKind::Demo),
            ("motd.txt", FileKind::Text),
            ("de_x.res", FileKind::Text),
            ("pak0.pak", FileKind::Pak),
            ("mp.dll", FileKind::Dll),
            ("cs.so", FileKind::Dll),
            ("config.cfg", FileKind::Cfg),
            ("liblist.gam", FileKind::Cfg),
            ("userconfig.rc", FileKind::Cfg),
            ("lixo.xyz", FileKind::Other),
            ("semextensao", FileKind::Other),
            (".oculto", FileKind::Other),
            ("termina.", FileKind::Other),
        ];
        for (n, k) in casos {
            assert_eq!(classify(n), k, "{n}");
        }
    }

    #[test]
    fn sanitize_recusa_fuga() {
        for ruim in ["../x", "a/../../b", "..", "/etc/passwd", "\\windows\\x", "C:\\x", "a/b:c", "a\0b", "", "   ".trim(), "./", "a/..\\..\\b"] {
            assert!(sanitize_rel(ruim).is_err(), "{ruim:?}");
        }
        assert_eq!(sanitize_rel("./maps//de_x.bsp").unwrap(), "maps/de_x.bsp");
        assert_eq!(sanitize_rel("maps\\de_x.bsp").unwrap(), "maps/de_x.bsp");
    }

    #[test]
    fn listagem_com_camadas_e_totais() {
        let t = fixture();
        let l = list_mod_files(&cs(&t), None, &ListOptions::default()).unwrap();
        assert!(l.valve_root.is_some());
        assert!(!l.truncated);
        let find = |path: &str, layer: Layer| l.entries.iter().find(|e| e.path == path && e.layer == layer);
        assert_eq!(find("maps/de_x.bsp", Layer::Mod).unwrap().kind, FileKind::Map);
        assert_eq!(find("maps/de_x.bsp", Layer::Mod).unwrap().size, 1000);
        assert_eq!(find("halflife.wad", Layer::Valve).unwrap().origin, Root::Valve);
        let ping = find("sound/pak/ping.wav", Layer::Pak).unwrap();
        assert_eq!(ping.pak.as_deref(), Some("pak0.pak"));
        assert_eq!(ping.open_path(), "pak0.pak/sound/pak/ping.wav");
        assert_eq!(ping.kind, FileKind::Sound);
        assert_eq!(find("sound/v/pak.wav", Layer::Pak).unwrap().origin, Root::Valve);
        assert!(find("sound/a.wav", Layer::Mod).is_some() && find("sound/a.wav", Layer::Valve).is_some());
        assert_eq!(find("autoexec.cfg", Layer::Mod).unwrap().kind, FileKind::Cfg);
        assert_eq!(find("dlls/mp.dll", Layer::Mod).unwrap().kind, FileKind::Dll);
        assert_eq!(find("demos/x.dem", Layer::Mod).unwrap().kind, FileKind::Demo);
        let maps = l.totals.iter().find(|t| t.kind == FileKind::Map).unwrap();
        assert_eq!(maps.count, 2); // de_x.bsp + de_p.bsp (pak)
        assert!(l.totals.iter().all(|t| t.count > 0));
    }

    #[test]
    fn listagem_sem_paks_e_com_valve_explicito() {
        let t = fixture();
        let opts = ListOptions { include_paks: false, ..Default::default() };
        let l = list_mod_files(&cs(&t), Some(&t.path("game/valve")), &opts).unwrap();
        assert!(l.entries.iter().all(|e| e.layer != Layer::Pak));
        // pasta do mod sendo a própria valve: uma camada só
        let l = list_mod_files(&t.path("game/valve"), None, &opts).unwrap();
        assert!(l.valve_root.is_none());
        assert!(l.entries.iter().all(|e| e.layer == Layer::Mod));
        // valve inexistente explícito = erro claro
        assert!(list_mod_files(&cs(&t), Some(&t.path("nao-existe")), &opts).is_err());
        assert!(list_mod_files(&t.path("nao-existe"), None, &opts).is_err());
    }

    #[test]
    fn listagem_respeita_o_teto() {
        let t = fixture();
        let l = list_mod_files(&cs(&t), None, &ListOptions { max_entries: 5, ..Default::default() }).unwrap();
        assert!(l.truncated);
        assert_eq!(l.entries.len(), 5);
        let l = list_mod_files(&cs(&t), None, &ListOptions { max_depth: 1, ..Default::default() }).unwrap();
        assert!(l.entries.iter().all(|e| !e.path.contains('/') || e.layer == Layer::Pak));
        assert!(l.warnings.iter().any(|w| w.contains("profundidade")));
    }

    #[test]
    fn arvore() {
        let t = fixture();
        let l = list_mod_files(&cs(&t), None, &ListOptions::default()).unwrap();
        let tree = build_tree(&l.entries);
        assert!(tree.is_dir);
        assert_eq!(tree.file_count as usize, {
            // arquivos distintos por caminho
            let mut s = std::collections::BTreeSet::new();
            l.entries.iter().for_each(|e| {
                s.insert(e.path.clone());
            });
            s.len()
        });
        // pastas antes de arquivos
        assert!(tree.children[0].is_dir);
        let sound = tree.children.iter().find(|c| c.name == "sound").unwrap();
        let a = sound.children.iter().find(|c| c.name == "a.wav").unwrap();
        assert_eq!(a.layers, vec![Layer::Mod, Layer::Valve]);
        assert_eq!(a.path, "sound/a.wav");
        assert!(sound.layers.contains(&Layer::Pak));
        let json = serde_json::to_value(&tree).unwrap();
        assert_eq!(json["children"][0]["is_dir"], true);
        assert_eq!(build_tree(&[]).children.len(), 0);
    }

    #[test]
    fn leitura_de_texto_e_binario() {
        let t = fixture();
        let p = read_mod_file(&cs(&t), "maps/de_x.txt", 1024).unwrap();
        assert!(p.is_text && !p.truncated);
        assert_eq!(p.text.as_deref(), Some("Planta a bomba\nem A ou B"));
        assert_eq!(p.info.layer, Layer::Mod);
        let p = read_mod_file(&cs(&t), "models/p.mdl", 1024).unwrap();
        assert!(!p.is_text && p.text.is_none());
        assert_eq!(p.base64.as_deref(), Some("SURTVC4uLi4="));
        assert_eq!(p.info.kind, FileKind::Model);
        // sem extensão: heurística olha o conteúdo
        assert!(read_mod_file(&cs(&t), "semextensao", 1024).unwrap().is_text);
        // limite
        let p = read_mod_file(&cs(&t), "maps/de_x.txt", 5).unwrap();
        assert!(p.truncated);
        assert_eq!(p.bytes_read, 5);
        assert_eq!(p.info.size, 25);
        // limite zero vira 1, nunca vazio nem panic
        assert_eq!(read_mod_file(&cs(&t), "maps/de_x.txt", 0).unwrap().bytes_read, 1);
        // binário com NUL em extensão de texto não vira texto
        t.put("game/cstrike/bin.txt", b"abc\0def");
        assert!(!read_mod_file(&cs(&t), "bin.txt", 100).unwrap().is_text);
        // arquivo vazio
        t.put("game/cstrike/vazio.txt", b"");
        let p = read_mod_file(&cs(&t), "vazio.txt", 100).unwrap();
        assert!(p.is_text && p.bytes_read == 0 && !p.truncated);
    }

    #[test]
    fn leitura_herda_do_valve_e_de_dentro_do_pak() {
        let t = fixture();
        let p = read_mod_file(&cs(&t), "halflife.wad", 16).unwrap();
        assert_eq!((p.info.layer, p.info.origin), (Layer::Valve, Root::Valve));
        // mod vence quando existe nos dois
        let p = read_mod_file(&cs(&t), "liblist.gam", 100).unwrap();
        assert!(p.text.unwrap().contains("Counter-Strike"));
        let p = read_mod_file_in(&cs(&t), None, "liblist.gam", 100, Some(Root::Valve)).unwrap();
        assert!(p.text.unwrap().contains("Half-Life"));
        assert!(read_mod_file_in(&cs(&t), None, "cstrike.wad", 100, Some(Root::Valve)).is_err());
        // pak: caminho virtual e caminho qualificado
        let p = read_mod_file(&cs(&t), "readme.txt", 100).unwrap();
        assert_eq!((p.info.layer, p.info.pak.as_deref()), (Layer::Pak, Some("pak0.pak")));
        assert_eq!(p.text.as_deref(), Some("dentro do pak"));
        let p = read_mod_file(&cs(&t), "pak0.pak/readme.txt", 100).unwrap();
        assert_eq!(p.text.as_deref(), Some("dentro do pak"));
        let p = read_mod_file(&cs(&t), "SOUND/V/PAK.WAV", 100).unwrap();
        assert_eq!(p.info.origin, Root::Valve);
        let p = read_mod_file(&cs(&t), "readme.txt", 6).unwrap();
        assert!(p.truncated && p.bytes_read == 6);
        assert!(read_mod_file(&cs(&t), "pak0.pak/nao-existe.txt", 100).is_err());
    }

    #[test]
    fn caminhos_maliciosos_sao_recusados() {
        let t = fixture();
        t.put("segredo.txt", b"fora da raiz");
        for rel in ["../../segredo.txt", "../segredo.txt", "/etc/passwd", "maps/../../../segredo.txt", "C:\\Windows\\win.ini", "..", "", "maps\\..\\..\\..\\segredo.txt"] {
            assert!(read_mod_file(&cs(&t), rel, 100).is_err(), "{rel:?}");
            assert!(analyze_mod_file(&cs(&t), rel).is_err(), "{rel:?}");
            assert!(read_mod_audio(&cs(&t), rel).is_err(), "{rel:?}");
        }
        // pasta, não arquivo
        assert!(read_mod_file(&cs(&t), "maps", 100).is_err());
        assert!(read_mod_file(&cs(&t), "nao/existe.txt", 100).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_que_foge_e_recusado() {
        let t = fixture();
        t.put("segredo.txt", b"fora da raiz");
        std::os::unix::fs::symlink(t.path("segredo.txt"), t.path("game/cstrike/atalho.txt")).unwrap();
        std::os::unix::fs::symlink(t.0.clone(), t.path("game/cstrike/pasta_fuga")).unwrap();
        std::os::unix::fs::symlink(t.path("game/cstrike/maps/de_x.txt"), t.path("game/cstrike/atalho_ok.txt")).unwrap();
        assert!(read_mod_file(&cs(&t), "atalho.txt", 100).is_err());
        assert!(read_mod_file(&cs(&t), "pasta_fuga/segredo.txt", 100).is_err());
        // atalho para dentro da raiz funciona
        assert!(read_mod_file(&cs(&t), "atalho_ok.txt", 100).unwrap().is_text);
        let l = list_mod_files(&cs(&t), None, &ListOptions::default()).unwrap();
        assert!(l.entries.iter().all(|e| e.name != "atalho.txt" && !e.path.starts_with("pasta_fuga")));
        assert!(l.entries.iter().any(|e| e.name == "atalho_ok.txt"));
        assert!(l.warnings.iter().any(|w| w.contains("atalho.txt")));
        // ciclo: link para a própria raiz não trava a listagem
        assert!(l.warnings.iter().any(|w| w.contains("pasta_fuga")));
    }

    #[test]
    fn analise_por_formato() {
        let t = fixture();
        let a = analyze_mod_file(&cs(&t), "sound/a.wav").unwrap();
        match a.analysis {
            Analysis::Audio(i) => {
                assert_eq!(i.sample_frames, Some(400));
                assert_eq!(i.envelope.unwrap().len(), 400);
            }
            _ => panic!("esperava áudio"),
        }
        match analyze_mod_file(&cs(&t), "maps/de_x.nav").unwrap().analysis {
            Analysis::Nav(n) => {
                assert_eq!(n.areas.len(), 1);
                assert_eq!(n.bsp_size_matches, Some(true)); // de_x.bsp tem 1000 bytes
            }
            _ => panic!("esperava nav"),
        }
        match analyze_mod_file(&cs(&t), "demos/x.dem").unwrap().analysis {
            Analysis::Demo(d) => assert!((d.duration_secs - 60.0).abs() < 1e-6),
            _ => panic!("esperava demo"),
        }
        match analyze_mod_file(&cs(&t), "pak0.pak").unwrap().analysis {
            Analysis::Pak(p) => assert_eq!(p.entries.len(), 3),
            _ => panic!("esperava pak"),
        }
        match analyze_mod_file(&cs(&t), "liblist.gam").unwrap().analysis {
            Analysis::Liblist(l) => assert_eq!(l.game.as_deref(), Some("Counter-Strike")),
            _ => panic!("esperava liblist"),
        }
        match analyze_mod_file(&cs(&t), "sprites/hud.txt").unwrap().analysis {
            Analysis::HudSprites(h) => assert_eq!(h.entries.len(), 1),
            _ => panic!("esperava hud"),
        }
        match analyze_mod_file(&cs(&t), "maps/de_x_detail.txt").unwrap().analysis {
            Analysis::DetailTextures(d) => assert_eq!(d.entries.len(), 1),
            _ => panic!("esperava detail"),
        }
        match analyze_mod_file(&cs(&t), "mapcycle.txt").unwrap().analysis {
            Analysis::MapCycle(m) => assert_eq!(m.entries.len(), 2),
            _ => panic!("esperava mapcycle"),
        }
        match analyze_mod_file(&cs(&t), "maps/de_x.txt").unwrap().analysis {
            Analysis::Text(d) => assert_eq!(d.line_count, 2),
            _ => panic!("esperava texto"),
        }
        assert!(matches!(analyze_mod_file(&cs(&t), "models/p.mdl").unwrap().analysis, Analysis::None));
        // dentro do pak
        assert!(matches!(analyze_mod_file(&cs(&t), "sound/pak/ping.wav").unwrap().analysis, Analysis::Audio(_)));
    }

    #[test]
    fn nav_com_bsp_de_outro_tamanho_avisa() {
        let t = fixture();
        t.put("game/cstrike/maps/de_x.bsp", &[0u8; 999]);
        match analyze_mod_file(&cs(&t), "maps/de_x.nav").unwrap().analysis {
            Analysis::Nav(n) => {
                assert_eq!(n.bsp_size_matches, Some(false));
                assert!(n.warnings.iter().any(|w| w.contains("bots")));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn json_do_contrato() {
        let t = fixture();
        let v = serde_json::to_value(analyze_mod_file(&cs(&t), "sound/a.wav").unwrap()).unwrap();
        assert_eq!(v["analysis"]["type"], "audio");
        assert_eq!(v["kind"], "sound");
        assert_eq!(v["layer"], "mod");
        assert_eq!(v["origin"], "mod");
        assert!(v["pak"].is_null());
        assert!(v["analysis"]["envelope"][0].is_array());
        let v = serde_json::to_value(analyze_mod_file(&cs(&t), "models/p.mdl").unwrap()).unwrap();
        assert_eq!(v["analysis"]["type"], "none");
        let v = serde_json::to_value(read_mod_file(&cs(&t), "readme.txt", 10).unwrap()).unwrap();
        assert_eq!(v["layer"], "pak");
        assert_eq!(v["pak"], "pak0.pak");
        assert_eq!(v["is_text"], true);
        let v = serde_json::to_value(list_mod_files(&cs(&t), None, &ListOptions::default()).unwrap()).unwrap();
        assert_eq!(v["entries"][0]["layer"], "mod");
        assert!(v["totals"][0]["kind"].is_string());
    }

    #[test]
    fn audio_como_data_url() {
        let t = fixture();
        let d = read_mod_audio(&cs(&t), "sound/a.wav").unwrap();
        assert_eq!(d.mime, "audio/wav");
        assert!(d.data_url.starts_with("data:audio/wav;base64,"));
        assert!(read_mod_audio(&cs(&t), "sound/pak/ping.wav").is_ok());
        // arquivo que não é áudio, mesmo com nome .wav
        t.put("game/cstrike/sound/falso.wav", b"MZ isto nao e um wav");
        assert!(read_mod_audio(&cs(&t), "sound/falso.wav").is_err());
        assert!(analyze_mod_file(&cs(&t), "sound/falso.wav").is_err());
        assert!(read_mod_audio(&cs(&t), "models/p.mdl").is_err());
    }

    #[test]
    fn arquivos_corrompidos_viram_erro_sem_panic() {
        let t = fixture();
        let mut seed = 5u32;
        let mut junk = |n: usize| -> Vec<u8> {
            (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 24) as u8
                })
                .collect()
        };
        for ext in ["nav", "dem", "pak", "wav", "mp3", "txt", "gam", "bsp", "mdl", "cfg"] {
            for n in [0usize, 1, 7, 100, 700, 5000] {
                let name = format!("lixo{n}.{ext}");
                t.put(&format!("game/cstrike/{name}"), &junk(n));
                let _ = analyze_mod_file(&cs(&t), &name);
                let _ = read_mod_file(&cs(&t), &name, 64);
                let _ = read_mod_audio(&cs(&t), &name);
            }
        }
        // um pak corrompido na raiz não derruba a listagem nem a busca
        t.put("game/cstrike/pak9.pak", b"PACK\xff\xff\xff\x7f\x40\0\0\0");
        let l = list_mod_files(&cs(&t), None, &ListOptions::default()).unwrap();
        assert!(l.warnings.iter().any(|w| w.contains("pak9.pak")));
        assert!(read_mod_file(&cs(&t), "readme.txt", 10).is_ok());
        // nav de lixo com magic certo
        let mut nav = build_nav(5, 0, &[], &[]);
        nav.extend_from_slice(&junk(300));
        t.put("game/cstrike/maps/ruim.nav", &nav);
        let _ = analyze_mod_file(&cs(&t), "maps/ruim.nav");
    }

    #[test]
    fn pak_com_nome_malicioso_nao_vaza() {
        let t = fixture();
        t.put("game/cstrike/pak1.pak", &build_pak(&[("../../segredo.txt", b"x"), ("/abs.txt", b"y"), ("ok.txt", b"z")]));
        let l = list_mod_files(&cs(&t), None, &ListOptions::default()).unwrap();
        assert!(l.entries.iter().all(|e| !e.path.contains("..") && !e.path.starts_with('/')));
        assert!(l.entries.iter().any(|e| e.path == "ok.txt"));
        assert!(read_mod_file(&cs(&t), "pak1.pak/../../segredo.txt", 10).is_err());
    }
}
