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

/// Confere um caminho vindo da interface antes de abri-lo: sem `..`, é arquivo comum
/// (não pasta, não dispositivo), tem uma das extensões esperadas e não passa de `max_bytes`.
/// Devolve o tamanho. Cada visualizador (sprite, WAD, imagem) só abre o formato que declara.
pub fn check_file(path: &Path, exts: &[&str], max_bytes: u64) -> Result<u64, String> {
    use std::path::Component;
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("caminho com `..` não é aceito: {}", path.display()));
    }
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if !ext.as_deref().is_some_and(|e| exts.contains(&e)) {
        return Err(format!("extensão não suportada (esperado: {}): {}", exts.join(", "), path.display()));
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("não abriu {}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("não é um arquivo: {}", path.display()));
    }
    if meta.len() > max_bytes {
        return Err(format!("arquivo grande demais ({} bytes, máximo {max_bytes}): {}", meta.len(), path.display()));
    }
    Ok(meta.len())
}

/// `check_file` + leitura inteira do arquivo.
pub fn read_checked(path: &Path, exts: &[&str], max_bytes: u64) -> Result<Vec<u8>, String> {
    check_file(path, exts, max_bytes)?;
    std::fs::read(path).map_err(|e| format!("não leu {}: {e}", path.display()))
}

/// Aplica `f` a cada item em até 8 threads e devolve os resultados na ordem de entrada.
/// Serve às varreduras de pasta (auditoria, exportação de contexto): um mapa por vez no pior caso.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    if items.len() < 2 || workers < 2 {
        return items.iter().map(&f).collect();
    }
    let chunk = items.len().div_ceil(workers);
    let f = &f;
    let mut parts: Vec<Vec<R>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = items.chunks(chunk).map(|slice| scope.spawn(move || slice.iter().map(f).collect::<Vec<_>>())).collect();
        for h in handles {
            // thread que entrou em pânico: o trecho some; quem chama confere o tamanho se importar
            parts.push(h.join().unwrap_or_default());
        }
    });
    parts.into_iter().flatten().collect()
}
