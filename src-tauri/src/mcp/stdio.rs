//! Laço de entrada/saída do transporte stdio: uma mensagem JSON por linha, só o protocolo
//! em `output` (logs vão para stderr, em quem chama).

use super::protocol::{Server, INVALID_REQUEST};
use std::io::{BufRead, Write};

/// Maior mensagem aceita (uma linha). Acima disso a linha é descartada com erro JSON-RPC.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// Lê linhas de `input` até o fim (EOF = cliente fechou) e escreve as respostas em `output`.
pub fn serve<R: BufRead, W: Write>(server: &mut Server, mut input: R, mut output: W) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let n = std::io::Read::take(&mut input, MAX_LINE_BYTES as u64 + 1).read_until(b'\n', &mut buf)?;
        if n == 0 {
            return Ok(());
        }
        let reply = if buf.len() > MAX_LINE_BYTES && buf.last() != Some(&b'\n') {
            // descarta o resto da linha gigante sem guardá-la
            let mut sink = Vec::new();
            loop {
                sink.clear();
                let m = std::io::Read::take(&mut input, 1 << 20).read_until(b'\n', &mut sink)?;
                if m == 0 || sink.last() == Some(&b'\n') {
                    break;
                }
            }
            Some(
                serde_json::json!({"jsonrpc": "2.0", "id": null, "error": {"code": INVALID_REQUEST, "message": format!("mensagem maior que {MAX_LINE_BYTES} bytes")}})
                    .to_string(),
            )
        } else {
            match std::str::from_utf8(&buf) {
                Ok(line) => server.handle_line(line),
                Err(_) => Some(
                    serde_json::json!({"jsonrpc": "2.0", "id": null, "error": {"code": super::protocol::PARSE_ERROR, "message": "a mensagem não é UTF-8 válido"}})
                        .to_string(),
                ),
            }
        };
        if let Some(r) = reply {
            output.write_all(r.as_bytes())?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}
