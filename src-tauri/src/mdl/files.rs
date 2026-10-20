//! Arquivos irmãos de um `.mdl` no disco: texturas externas (`nomeT.mdl`) e
//! grupos de sequência (`nome01.mdl`, `nome02.mdl`...), sempre na **mesma pasta**.
//!
//! Convenção conferida em `Mod_LoadStudioModel`/`R_StudioGetAnim` do Xash3D
//! (`engine/common/mod_studio.c`): textura = nome sem extensão + `"T.mdl"`;
//! grupo N = nome + `N/10` + `N%10` + `".mdl"`. Os mods foram feitos no Windows
//! (sem diferença de maiúsculas); no Linux o nome é procurado sem diferenciá-las.
//! LOD não existe no GoldSrc.

use super::{needs_external_textures, parse_with, sequence_frames_with, MdlModel, ParseOptions, SeqFrames, SeqOptions};
use std::path::{Path, PathBuf};

/// `name` dentro de `dir`, exato ou sem diferenciar maiúsculas.
fn find_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .map(|e| e.path())
        .filter(|p| p.is_file())
}

fn sibling(path: &Path, suffix: &str) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("mdl");
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    find_in_dir(dir, &format!("{stem}{suffix}.{ext}"))
}

/// `nomeT.mdl` ao lado de `nome.mdl`, se existir.
pub fn texture_file(path: &Path) -> Option<PathBuf> {
    sibling(path, "T")
}

/// Grupo de sequência `group` (>= 1): `nome01.mdl`, `nome02.mdl`...
pub fn group_file(path: &Path, group: u32) -> Option<PathBuf> {
    if group == 0 {
        return None;
    }
    sibling(path, &format!("{:02}", group))
}

/// Lê o `.mdl` e decodifica, buscando `nomeT.mdl` sozinho quando o arquivo
/// principal não tem texturas.
pub fn parse_file(path: &Path, opts: &ParseOptions) -> Result<MdlModel, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let tex = if needs_external_textures(&bytes) { texture_file(path).and_then(|p| std::fs::read(p).ok()) } else { None };
    parse_with(&bytes, tex.as_deref(), opts).map_err(|e| e.to_string())
}

/// Quadros de uma sequência do arquivo `path`, resolvendo grupos externos pela pasta.
pub fn sequence_file(path: &Path, index: usize, opts: &SeqOptions) -> Result<SeqFrames, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let external = |group: u32| group_file(path, group).and_then(|p| std::fs::read(p).ok());
    sequence_frames_with(&bytes, index, &external, opts).map_err(|e| e.to_string())
}
