//! Recursos MCP (`resources/*`): o mesmo conteúdo das ferramentas, endereçável por URI para
//! clientes que anexam recursos ao contexto.
//!
//! - `bsp-museum://roots` — raízes permitidas e limites
//! - `bsp-museum://maps` — mapas de todas as raízes
//! - `bsp-museum://map/<nome>/{summary|diagnostics|resources|textures|entities}` — por mapa
//!   (`<nome>` é o nome do arquivo sem `.bsp`, com %-codificação)

use super::protocol::{run_tool, RpcError, RESOURCE_NOT_FOUND};
use super::sandbox::display_path;
use super::tools::{roots_value, Config};
use crate::catalog;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const SCHEME: &str = "bsp-museum://";

/// Parte do URI -> ferramenta que a produz.
const MAP_VIEWS: [(&str, &str, &str); 5] = [
    ("summary", "map_summary", "Resumo do mapa"),
    ("diagnostics", "map_diagnostics", "Diagnóstico do mapa"),
    ("resources", "map_resources", "Recursos / FastDL do mapa"),
    ("textures", "map_textures", "Texturas do mapa"),
    ("entities", "map_entities", "Entidades do mapa"),
];

/// Quantos mapas aparecem explicitamente em `resources/list` (os demais vêm pelo template).
const LIST_MAPS: usize = 100;

fn encode(name: &str) -> String {
    let mut out = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Todos os `.bsp` das raízes que continuam dentro delas.
fn all_maps(cfg: &Config) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in cfg.roots.list() {
        for f in catalog::find_bsp_files(root) {
            if f.canonicalize().is_ok_and(|c| cfg.roots.contains(&c)) {
                out.push(f);
            }
        }
    }
    out
}

pub fn list(cfg: &Config) -> Value {
    let mut resources = vec![
        json!({"uri": format!("{SCHEME}roots"), "name": "roots", "title": "Raízes permitidas", "description": "Pastas que o servidor pode ler e seus limites.", "mimeType": "application/json"}),
        json!({"uri": format!("{SCHEME}maps"), "name": "maps", "title": "Mapas", "description": "Mapas encontrados nas raízes, com resumo barato.", "mimeType": "application/json"}),
    ];
    for f in all_maps(cfg).into_iter().take(LIST_MAPS) {
        let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        resources.push(json!({
            "uri": format!("{SCHEME}map/{}/summary", encode(&stem)),
            "name": format!("{stem}/summary"),
            "title": format!("Resumo de {stem}"),
            "mimeType": "application/json",
        }));
    }
    json!({"resources": resources})
}

pub fn templates() -> Value {
    let templates: Vec<Value> = MAP_VIEWS
        .iter()
        .map(|(part, _, title)| {
            json!({
                "uriTemplate": format!("{SCHEME}map/{{name}}/{part}"),
                "name": format!("map-{part}"),
                "title": title,
                "description": format!("{title}; `name` é o nome do .bsp sem a extensão."),
                "mimeType": "application/json",
            })
        })
        .collect();
    json!({"resourceTemplates": templates})
}

fn not_found(uri: &str, why: &str) -> RpcError {
    RpcError { code: RESOURCE_NOT_FOUND, message: format!("recurso não encontrado: {why}"), data: Some(json!({"uri": uri})) }
}

fn find_map(cfg: &Config, uri: &str, name: &str) -> Result<PathBuf, RpcError> {
    let bad = name.is_empty() || name.contains(['/', '\\', '\0']) || name == "." || name == "..";
    if bad {
        return Err(RpcError::invalid_params(format!("nome de mapa inválido: {name}")));
    }
    let lower = name.to_lowercase();
    let hits: Vec<PathBuf> = all_maps(cfg)
        .into_iter()
        .filter(|f| f.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == lower))
        .collect();
    match hits.len() {
        0 => Err(not_found(uri, &format!("nenhum mapa chamado {name} nas raízes permitidas"))),
        1 => Ok(hits[0].clone()),
        _ => Err(RpcError::invalid_params(format!(
            "nome ambíguo ({name}): {}. Use as ferramentas com o caminho completo.",
            hits.iter().map(|h| display_path(h)).collect::<Vec<_>>().join(", ")
        ))),
    }
}

fn contents(uri: &str, value: &Value) -> Value {
    json!({"contents": [{"uri": uri, "mimeType": "application/json", "text": value.to_string()}]})
}

pub fn read(cfg: &Config, uri: &str) -> Result<Value, RpcError> {
    let Some(rest) = uri.strip_prefix(SCHEME) else {
        return Err(RpcError::invalid_params(format!("URI desconhecido (esperado {SCHEME}…): {uri}")));
    };
    let tool_err = |e: super::output::ToolError| match e {
        super::output::ToolError::InvalidParams(m) => RpcError::invalid_params(m),
        super::output::ToolError::Failed(m) => RpcError::new(super::protocol::INTERNAL_ERROR, m),
    };
    match rest {
        "roots" => Ok(contents(uri, &roots_value(cfg))),
        "maps" => {
            let mut rows = Vec::new();
            for root in cfg.roots.list() {
                let v = run_tool(cfg, "list_maps", json!({"dir": display_path(root), "limit": 200})).map_err(tool_err)?;
                rows.push(v);
            }
            Ok(contents(uri, &json!({"roots": rows})))
        }
        _ => {
            let parts: Vec<&str> = rest.split('/').collect();
            let ["map", name, view] = parts[..] else {
                return Err(not_found(uri, "URI não reconhecido"));
            };
            let name = decode(name).ok_or_else(|| RpcError::invalid_params("nome com %-codificação inválida"))?;
            let Some((_, tool, _)) = MAP_VIEWS.iter().find(|(v, _, _)| *v == view) else {
                return Err(not_found(uri, &format!("visão desconhecida: {view}")));
            };
            let path = find_map(cfg, uri, &name)?;
            let v = run_tool(cfg, tool, json!({"path": display_path(&path)})).map_err(tool_err)?;
            Ok(contents(uri, &v))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encoding_vai_e_volta() {
        assert_eq!(encode("de dust#2"), "de%20dust%232");
        assert_eq!(decode("de%20dust%232").unwrap(), "de dust#2");
        assert!(decode("%zz").is_none());
        assert!(decode("%2").is_none());
    }
}
