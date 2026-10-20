//! `bsp-museum-mcp`: servidor MCP (stdio, somente leitura) e exportação de contexto do acervo.
//! Em `serve`, stdout carrega só o protocolo; tudo o mais vai para stderr.

use bsp_museum_core::agentctx;
use bsp_museum_core::mcp::{self, cli::Command, Config, Roots, Server};
use std::io::{BufReader, BufWriter};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = |k: &str| std::env::var(k).ok();
    let cmd = match mcp::cli::parse(&args, env("BSP_MUSEUM_ROOTS").as_deref(), env("BSP_MUSEUM_ALLOW_WRITE").as_deref(), env("BSP_MUSEUM_MAX_OUTPUT").as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bsp-museum-mcp: {e}\n\n{}", mcp::cli::HELP);
            return ExitCode::from(2);
        }
    };
    match cmd {
        Command::Help => {
            print!("{}", mcp::cli::HELP);
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("bsp-museum-mcp {}", mcp::VERSION);
            ExitCode::SUCCESS
        }
        Command::Export { dir, out, format, slots } => match agentctx::export_with(&dir, &out, slots, format) {
            Ok(s) => {
                println!(
                    "{} mapa(s) em {}: {} crítico(s), {} aviso(s), {} info; FastDL {} em {} arquivo(s); {} recurso(s) ausente(s)",
                    s.maps,
                    s.out_dir,
                    s.critical,
                    s.warn,
                    s.info,
                    agentctx::human_bytes(s.fastdl_bytes),
                    s.fastdl_files,
                    s.missing_resources
                );
                for f in &s.files {
                    println!("  {f}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("bsp-museum-mcp: {e}");
                ExitCode::from(1)
            }
        },
        Command::Serve { roots, allow_write, max_output } => {
            let roots = match Roots::new(&roots) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("bsp-museum-mcp: {e}");
                    return ExitCode::from(1);
                }
            };
            let mut cfg = Config::new(roots);
            cfg.allow_write = allow_write;
            if let Some(n) = max_output {
                cfg.max_output = n.max(mcp::tools::MIN_MAX_OUTPUT);
            }
            eprintln!(
                "bsp-museum-mcp {} pronto (stdio); raízes: {}{}",
                mcp::VERSION,
                cfg.roots.list().iter().map(|r| mcp::sandbox::display_path(r)).collect::<Vec<_>>().join(", "),
                if cfg.allow_write { "; gravação de radar ligada" } else { "" }
            );
            let mut server = Server::new(cfg);
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            match mcp::stdio::serve(&mut server, BufReader::new(stdin.lock()), BufWriter::new(stdout.lock())) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("bsp-museum-mcp: erro de E/S: {e}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
