//! Teste do processo real: `bsp-museum-mcp` ligado por stdin/stdout. Os fixtures sintéticos da
//! lib são `#[cfg(test)]` (invisíveis daqui), então o mapa é um BSP mínimo montado à mão —
//! só o cabeçalho e o lump de entidades.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_bsp-museum-mcp");

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("bsp-museum-stdio-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}

/// BSP v30 com os 15 lumps vazios, exceto o de entidades.
fn mini_bsp(entities: &str) -> Vec<u8> {
    let mut ents = entities.as_bytes().to_vec();
    ents.push(0);
    let header: i32 = 4 + 15 * 8;
    let mut out = 30i32.to_le_bytes().to_vec();
    for i in 0..15 {
        let len = if i == 0 { ents.len() } else { 0 };
        out.extend_from_slice(&header.to_le_bytes());
        out.extend_from_slice(&(len as i32).to_le_bytes());
    }
    out.extend_from_slice(&ents);
    out
}

fn mapa(dir: &Path) -> PathBuf {
    let ents = "{\n\"classname\" \"worldspawn\"\n\"message\" \"Mapa de Teste\"\n}\n\
                {\n\"classname\" \"info_player_start\"\n\"origin\" \"0 0 36\"\n}\n\
                {\n\"classname\" \"info_player_deathmatch\"\n\"origin\" \"64 0 36\"\n}\n";
    let path = dir.join("cstrike/maps/de_teste.bsp");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, mini_bsp(ents)).unwrap();
    path
}

/// Roda o servidor com as linhas de entrada e devolve (stdout, stderr).
fn sessao(args: &[&str], envs: &[(&str, &str)], linhas: &[String]) -> (String, String) {
    let mut cmd = Command::new(BIN);
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for l in linhas {
            writeln!(stdin, "{l}").unwrap();
        }
    }
    drop(child.stdin.take()); // EOF: o servidor encerra sozinho
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "saiu com {:?}: {}", out.status, String::from_utf8_lossy(&out.stderr));
    (String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

fn req(id: u32, method: &str, params: serde_json::Value) -> String {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

#[test]
fn sessao_completa_por_stdin_stdout() {
    let root = tmp("sessao");
    let bsp = mapa(&root);
    let linhas = vec![
        req(1, "initialize", serde_json::json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "teste", "version": "1"}})),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        req(2, "tools/list", serde_json::json!({})),
        req(3, "tools/call", serde_json::json!({"name": "list_maps", "arguments": {"dir": root.join("cstrike/maps")}})),
        req(4, "tools/call", serde_json::json!({"name": "map_entities", "arguments": {"path": bsp, "classname": "info_player_*"}})),
        req(5, "tools/call", serde_json::json!({"name": "map_summary", "arguments": {"path": "/etc/passwd"}})),
        req(6, "tools/call", serde_json::json!({"name": "ferramenta_que_nao_existe", "arguments": {}})),
        "isto não é json".to_string(),
        req(7, "resources/read", serde_json::json!({"uri": "bsp-museum://roots"})),
    ];
    let (stdout, stderr) = sessao(&["serve", "--root", root.to_str().unwrap()], &[], &linhas);

    // stdout só tem o protocolo: toda linha é um JSON-RPC 2.0
    let msgs: Vec<serde_json::Value> = stdout.lines().map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("linha fora do protocolo: {l}"))).collect();
    assert_eq!(msgs.len(), 8, "uma resposta por requisição (a notificação não responde): {stdout}");
    assert!(msgs.iter().all(|m| m["jsonrpc"] == "2.0"));
    assert!(stderr.contains("pronto"), "logs vão para stderr: {stderr}");

    assert_eq!(msgs[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(msgs[0]["result"]["serverInfo"]["name"], "bsp-museum");
    let tools = msgs[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "map_summary" && t["annotations"]["readOnlyHint"] == true));

    let lista: serde_json::Value = serde_json::from_str(msgs[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(lista["total"], 1);
    assert_eq!(lista["maps"][0]["name"], "de_teste");
    assert_eq!(lista["maps"][0]["title"], "Mapa de Teste");

    let ents: serde_json::Value = serde_json::from_str(msgs[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(ents["total"], 2);

    // caminho fora da raiz: erro de execução, não de protocolo
    assert_eq!(msgs[4]["result"]["isError"], true);
    assert!(msgs[4]["result"]["content"][0]["text"].as_str().unwrap().contains("fora das raízes"));
    assert_eq!(msgs[5]["error"]["code"], -32602);
    assert_eq!(msgs[6]["error"]["code"], -32700);
    assert!(msgs[7]["result"]["contents"][0]["text"].as_str().unwrap().contains("roots"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn raizes_pela_variavel_de_ambiente_e_padrao_e_o_diretorio_atual() {
    let root = tmp("env");
    mapa(&root);
    let linhas = vec![
        req(1, "initialize", serde_json::json!({"protocolVersion": "2024-11-05", "capabilities": {}})),
        req(2, "tools/call", serde_json::json!({"name": "list_roots", "arguments": {}})),
    ];
    let (stdout, _) = sessao(&["serve"], &[("BSP_MUSEUM_ROOTS", root.to_str().unwrap())], &linhas);
    let msgs: Vec<serde_json::Value> = stdout.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(msgs[0]["result"]["protocolVersion"], "2024-11-05");
    let roots: serde_json::Value = serde_json::from_str(msgs[1]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(roots["roots"][0], root.to_str().unwrap());
    assert!(msgs[1]["result"].get("structuredContent").is_none(), "2024-11-05 não tem structuredContent");

    // sem --root nem variável: só o diretório de trabalho
    let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
    let mut cmd = Command::new(BIN);
    cmd.env_remove("BSP_MUSEUM_ROOTS").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "{}", linhas[0]).unwrap();
        writeln!(stdin, "{}", linhas[1]).unwrap();
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let second: serde_json::Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
    let roots: serde_json::Value = serde_json::from_str(second["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(roots["roots"][0], cwd.to_str().unwrap());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn versao_ajuda_e_erro_de_uso() {
    let out = Command::new(BIN).arg("--version").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("bsp-museum-mcp "));
    let out = Command::new(BIN).arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success() && help.contains("export-context") && help.contains("--root"));
    let out = Command::new(BIN).arg("--opcao-que-nao-existe").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "erro de uso não escreve em stdout");
    let out = Command::new(BIN).args(["serve", "--root", "/pasta/que/nao/existe"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
}

#[test]
fn export_context_gera_os_arquivos() {
    let root = tmp("export");
    mapa(&root);
    let out_dir = root.join("ctx");
    let out = Command::new(BIN)
        .args(["export-context", root.join("cstrike/maps").to_str().unwrap(), "--out", out_dir.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    for f in ["index.json", "REPORT.md", "fastdl.txt", "AGENTS.md"] {
        assert!(out_dir.join(f).is_file(), "falta {f}");
    }
    let idx: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out_dir.join("index.json")).unwrap()).unwrap();
    assert_eq!(idx["maps"].as_array().unwrap().len(), 1);
    assert_eq!(idx["maps"][0]["name"], "de_teste");
    assert!(out_dir.join("maps/de_teste.json").is_file());
    let _ = std::fs::remove_dir_all(&root);
}
