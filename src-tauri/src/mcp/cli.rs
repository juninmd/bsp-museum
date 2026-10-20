//! Linha de comando do `bsp-museum-mcp`: `serve` (padrão), `export-context`, `--version`, `--help`.
//! Separado do binário para ser testado sem processo.

use std::path::PathBuf;

pub const HELP: &str = "\
bsp-museum-mcp — servidor MCP (somente leitura) e exportação de contexto do acervo de mapas GoldSrc

USO
  bsp-museum-mcp [serve] [--root <pasta>]... [--allow-write] [--max-output <bytes>]
  bsp-museum-mcp export-context <pasta-de-mapas> --out <pasta> [--format json|md|jsonl] [--slots <n>]
  bsp-museum-mcp --version | --help

SERVE (padrão)
  Fala MCP (JSON-RPC 2.0, uma mensagem por linha) em stdin/stdout; logs vão para stderr.
  --root <pasta>        raiz permitida (repetível). Também lê BSP_MUSEUM_ROOTS (separado por ':' ou ';').
                        Sem nenhuma raiz, só o diretório de trabalho atual.
  --allow-write         deixa map_radar gravar overviews/ dentro de uma raiz (env BSP_MUSEUM_ALLOW_WRITE=1)
  --max-output <bytes>  teto do JSON de cada resposta (padrão 48000; env BSP_MUSEUM_MAX_OUTPUT)

EXPORT-CONTEXT
  Gera index.json, maps/<nome>.json, REPORT.md, fastdl.txt e AGENTS.md em --out.
  --format json|md|jsonl   formato por mapa (padrão json)
  --slots <n>              slots do servidor para o diagnóstico (padrão 32)

EXEMPLO
  claude mcp add bsp-museum -- /caminho/bsp-museum-mcp serve --root /caminho/cstrike
";

#[derive(Debug, PartialEq)]
pub enum Command {
    Serve { roots: Vec<PathBuf>, allow_write: bool, max_output: Option<usize> },
    Export { dir: PathBuf, out: PathBuf, format: crate::agentctx::Format, slots: usize },
    Version,
    Help,
}

/// `args` sem o nome do programa; `env_roots`, `env_write` e `env_max` são as variáveis de ambiente já lidas.
pub fn parse(args: &[String], env_roots: Option<&str>, env_write: Option<&str>, env_max: Option<&str>) -> Result<Command, String> {
    let mut it = args.iter().peekable();
    // subcomando opcional
    let sub = match it.peek().map(|s| s.as_str()) {
        Some("serve") | Some("export-context") => it.next().map(|s| s.as_str()),
        _ => None,
    };
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut allow_write = matches!(env_write, Some("1") | Some("true") | Some("yes"));
    let mut max_output: Option<usize> = match env_max {
        Some(v) if !v.trim().is_empty() => Some(v.trim().parse().map_err(|_| format!("BSP_MUSEUM_MAX_OUTPUT inválido: {v}"))?),
        _ => None,
    };
    let (mut out, mut format, mut slots) = (None::<PathBuf>, crate::agentctx::Format::Json, 32usize);
    let mut positional: Vec<String> = Vec::new();

    // `--flag valor` ou `--flag=valor`
    fn value(it: &mut std::iter::Peekable<std::slice::Iter<String>>, inline: Option<String>, flag: &str) -> Result<String, String> {
        match inline {
            Some(v) => Ok(v),
            None => it.next().cloned().ok_or_else(|| format!("{flag} precisa de um valor")),
        }
    }

    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        match flag.as_str() {
            "--help" | "-h" => return Ok(Command::Help),
            "--version" | "-V" => return Ok(Command::Version),
            "--root" => roots.push(PathBuf::from(value(&mut it, inline, "--root")?)),
            "--allow-write" => allow_write = true,
            "--max-output" => {
                let v = value(&mut it, inline, "--max-output")?;
                max_output = Some(v.parse().map_err(|_| format!("--max-output inválido: {v}"))?);
            }
            "--out" => out = Some(PathBuf::from(value(&mut it, inline, "--out")?)),
            "--format" => format = crate::agentctx::Format::parse(&value(&mut it, inline, "--format")?)?,
            "--slots" => {
                let v = value(&mut it, inline, "--slots")?;
                slots = v.parse().map_err(|_| format!("--slots inválido: {v}"))?;
            }
            f if f.starts_with('-') && f != "-" => return Err(format!("opção desconhecida: {f}")),
            _ => positional.push(arg.clone()),
        }
    }

    match sub {
        Some("export-context") => {
            let [dir] = &positional[..] else {
                return Err("export-context precisa de exatamente uma <pasta-de-mapas>".into());
            };
            let out = out.ok_or("export-context precisa de --out <pasta>")?;
            Ok(Command::Export { dir: PathBuf::from(dir), out, format, slots })
        }
        _ => {
            if let Some(extra) = positional.first() {
                return Err(format!("argumento inesperado: {extra}"));
            }
            if out.is_some() {
                return Err("--out só vale para export-context".into());
            }
            if let Some(env) = env_roots {
                roots.extend(super::sandbox::split_roots_env(env));
            }
            Ok(Command::Serve { roots, allow_write, max_output })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentctx::Format;

    fn p(args: &[&str]) -> Result<Command, String> {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), None, None, None)
    }

    #[test]
    fn padrao_e_serve_sem_raiz() {
        assert_eq!(p(&[]).unwrap(), Command::Serve { roots: vec![], allow_write: false, max_output: None });
        assert_eq!(p(&["serve"]).unwrap(), p(&[]).unwrap());
    }

    #[test]
    fn serve_com_raizes_repetidas_e_igual() {
        let c = p(&["serve", "--root", "/a", "--root=/b", "--allow-write", "--max-output", "1000"]).unwrap();
        assert_eq!(c, Command::Serve { roots: vec!["/a".into(), "/b".into()], allow_write: true, max_output: Some(1000) });
        assert_eq!(p(&["--root", "/a"]).unwrap(), Command::Serve { roots: vec!["/a".into()], allow_write: false, max_output: None });
    }

    #[test]
    fn env_soma_raizes_e_liga_gravacao() {
        let c = parse(&["--root".into(), "/a".into()], Some("/b:/c"), Some("1"), Some("5000")).unwrap();
        assert_eq!(c, Command::Serve { roots: vec!["/a".into(), "/b".into(), "/c".into()], allow_write: true, max_output: Some(5000) });
        assert!(parse(&[], None, None, Some("abc")).is_err());
    }

    #[test]
    fn export_context() {
        let c = p(&["export-context", "maps", "--out", "ctx", "--format", "md", "--slots=16"]).unwrap();
        assert_eq!(c, Command::Export { dir: "maps".into(), out: "ctx".into(), format: Format::Md, slots: 16 });
        assert!(p(&["export-context", "maps"]).is_err(), "falta --out");
        assert!(p(&["export-context", "--out", "x"]).is_err(), "falta a pasta");
        assert!(p(&["export-context", "a", "b", "--out", "x"]).is_err());
        assert!(p(&["export-context", "a", "--out", "x", "--format", "xml"]).is_err());
    }

    #[test]
    fn versao_ajuda_e_erros() {
        assert_eq!(p(&["--version"]).unwrap(), Command::Version);
        assert_eq!(p(&["-h"]).unwrap(), Command::Help);
        assert!(p(&["--nao-existe"]).is_err());
        assert!(p(&["serve", "sobrando"]).is_err());
        assert!(p(&["--root"]).is_err());
        assert!(p(&["serve", "--out", "x"]).is_err());
    }
}
