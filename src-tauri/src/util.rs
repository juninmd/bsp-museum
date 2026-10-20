//! Utilidades de I/O compartilhadas pelo app e pelo servidor MCP.

use std::path::Path;

/// Grava por arquivo temporário + renomeação: queda no meio da escrita não deixa o arquivo
/// pela metade (o `settings.json` guarda favoritos e notas do usuário).
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// Grava um arquivo de texto (planta SVG, relatório de auditoria, lista de FastDL).
pub fn export_text(target: &str, content: &str) -> Result<(), String> {
    // o frontend só exporta estes formatos; recusar o resto limita o estrago se algum
    // texto de mapa hostil conseguir injetar script no WebView
    const ALLOWED: [&str; 6] = ["svg", "txt", "csv", "md", "html", "json"];
    let ext = Path::new(target).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if !ext.as_deref().is_some_and(|e| ALLOWED.contains(&e)) {
        return Err(format!("extensão não permitida para exportação: {target}"));
    }
    std::fs::write(target, content).map_err(|e| format!("não gravou {target}: {e}"))
}
