//! Registro das ferramentas MCP: definição (nome, descrição, `inputSchema`, anotações),
//! leitura tipada dos argumentos e as peças comuns (caminhos confinados, limites).
//! As ferramentas em si ficam em `tools_map` (mapas) e `tools_files` (WAD, modelos, mod…);
//! todas só chamam as funções do núcleo — nenhuma lógica de formato é repetida aqui.

use super::output::{ToolError, ToolOutput};
use super::sandbox::Roots;
use crate::bsp::entities::{self, Entity};
use crate::bsp::Bsp;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// Limites de tamanho por tipo de arquivo (checados antes de abrir).
pub const MAX_BSP_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_MDL_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_NAV_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_DEMO_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Saída máxima padrão (JSON serializado) de uma chamada.
pub const DEFAULT_MAX_OUTPUT: usize = 48_000;
/// Menor teto aceito (abaixo disso nem o aviso de corte cabe).
pub const MIN_MAX_OUTPUT: usize = 2_000;

/// Configuração do servidor: o que ele pode ler e quanto devolve.
#[derive(Debug, Clone)]
pub struct Config {
    pub roots: Roots,
    /// `map_radar` com `out_dir` só grava com isto ligado (`--allow-write`)
    pub allow_write: bool,
    pub max_output: usize,
}

impl Config {
    pub fn new(roots: Roots) -> Self {
        Self { roots, allow_write: false, max_output: DEFAULT_MAX_OUTPUT }
    }
}

pub type Handler = fn(&Config, &Args) -> Result<ToolOutput, ToolError>;

pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: String,
    pub schema: Value,
    /// `false` só para quem pode gravar em disco (e só com `--allow-write`)
    pub read_only: bool,
    pub handler: Handler,
}

impl Tool {
    /// Definição como aparece em `tools/list`.
    pub fn definition(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.schema,
            "annotations": {
                "title": self.title,
                "readOnlyHint": self.read_only,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false,
            },
        })
    }
}

/// Argumentos já validados contra o schema (tipos e faixas conferidos antes).
pub struct Args<'a> {
    map: &'a Map<String, Value>,
}

impl<'a> Args<'a> {
    pub fn new(map: &'a Map<String, Value>) -> Self {
        Self { map }
    }

    pub fn str(&self, key: &str) -> Option<&'a str> {
        self.map.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
    }

    pub fn req_str(&self, key: &str) -> Result<&'a str, ToolError> {
        self.str(key).ok_or_else(|| ToolError::InvalidParams(format!("argumento obrigatório ausente: {key}")))
    }

    pub fn opt_usize(&self, key: &str) -> Option<usize> {
        let v = self.map.get(key)?;
        v.as_u64().map(|n| n as usize).or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as usize))
    }

    pub fn usize_or(&self, key: &str, default: usize) -> usize {
        self.opt_usize(key).unwrap_or(default)
    }

    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.map.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn slots(&self) -> usize {
        self.usize_or("slots", 32)
    }
}

// ---------------------------------------------------------------------------------
// caminhos
// ---------------------------------------------------------------------------------

/// Arquivo dentro das raízes, com a extensão esperada e tamanho limitado.
pub fn file_arg(cfg: &Config, a: &Args, key: &str, exts: &[&str], max: u64) -> Result<PathBuf, ToolError> {
    let p = cfg.roots.resolve_file(a.req_str(key)?)?;
    crate::util::check_file(&p, exts, max)?;
    Ok(p)
}

pub fn dir_arg(cfg: &Config, a: &Args, key: &str) -> Result<PathBuf, ToolError> {
    Ok(cfg.roots.resolve_dir(a.req_str(key)?)?)
}

/// Os `.bsp` de uma pasta que continuam dentro das raízes depois de seguir symlinks, e
/// quantos foram descartados por fugirem delas.
pub fn confined_bsp_files(cfg: &Config, dir: &Path) -> (Vec<PathBuf>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0;
    for f in crate::catalog::find_bsp_files(dir) {
        match f.canonicalize() {
            Ok(c) if cfg.roots.contains(&c) => out.push(f),
            _ => skipped += 1,
        }
    }
    (out, skipped)
}

pub fn load_bsp(path: &Path) -> Result<(Bsp, Vec<Entity>), ToolError> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    let parsed = entities::parse(&bsp.entities_raw);
    Ok((bsp, parsed))
}

pub fn map_name(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Caminho de `p` relativo a `base`, com `/`; o próprio `p` se estiver fora.
pub fn rel(base: &Path, p: &Path) -> String {
    let r = p.strip_prefix(base).unwrap_or(p);
    super::sandbox::display_path(r).replace('\\', "/")
}

// ---------------------------------------------------------------------------------
// registro
// ---------------------------------------------------------------------------------

/// Todas as ferramentas, na ordem em que aparecem em `tools/list`.
pub fn registry(cfg: &Config) -> Vec<Tool> {
    let mut v = vec![Tool {
        name: "list_roots",
        title: "Raízes permitidas",
        description: "Lista as pastas que este servidor pode ler (todo caminho precisa estar embaixo de uma delas), se gravação está ligada e os limites de saída. Comece por aqui para saber de onde partir.".into(),
        schema: super::schema::object(vec![], &[]),
        read_only: true,
        handler: list_roots,
    }];
    v.extend(super::tools_map::tools(cfg));
    v.extend(super::tools_files::tools(cfg));
    v
}

fn list_roots(cfg: &Config, _a: &Args) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::json(roots_value(cfg)))
}

pub fn roots_value(cfg: &Config) -> Value {
    json!({
        "server": {"name": super::SERVER_NAME, "version": super::VERSION},
        "roots": cfg.roots.list().iter().map(|r| cfg.roots.show(r)).collect::<Vec<_>>(),
        "read_only": !cfg.allow_write,
        "allow_write": cfg.allow_write,
        "max_output_bytes": cfg.max_output,
        "note": "Somente leitura. Caminhos relativos são procurados dentro de cada raiz; `..` e symlinks que saem das raízes são recusados.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomes_unicos_e_schemas_validos() {
        let cfg = Config::new(Roots::new(&[]).unwrap());
        let tools = registry(&cfg);
        let mut names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), n, "nome de ferramenta repetido");
        for t in &tools {
            assert!(t.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{} não é snake_case", t.name);
            assert_eq!(t.schema["type"], "object", "{}", t.name);
            assert_eq!(t.schema["additionalProperties"], json!(false), "{}", t.name);
            assert!(!t.description.is_empty(), "{}", t.name);
            // toda chave obrigatória existe em properties
            for r in t.schema["required"].as_array().unwrap() {
                assert!(t.schema["properties"].get(r.as_str().unwrap()).is_some(), "{}: required {r} sem propriedade", t.name);
            }
            assert_eq!(t.definition()["annotations"]["readOnlyHint"], json!(t.read_only));
        }
    }

    #[test]
    fn as_ferramentas_pedidas_existem() {
        let cfg = Config::new(Roots::new(&[]).unwrap());
        let have: Vec<&str> = registry(&cfg).iter().map(|t| t.name).collect();
        for want in [
            "list_maps", "map_summary", "map_diagnostics", "map_entities", "map_resources", "map_textures", "map_scene",
            "map_volumes", "map_hulls_info", "audit_folder", "compare_maps", "map_radar", "wad_list", "wad_texture",
            "wad_usage", "sprite_info", "model_info", "mod_files", "mod_file_analyze", "audio_info", "nav_info",
            "demo_info", "server_checklist",
        ] {
            assert!(have.contains(&want), "falta a ferramenta {want}");
        }
    }
}
