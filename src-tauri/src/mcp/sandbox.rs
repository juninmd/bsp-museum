//! Raízes permitidas do servidor MCP: todo caminho que chega de fora é canonicalizado
//! (segue symlinks) e precisa ficar embaixo de uma delas. É a única porta de entrada
//! para o disco — as ferramentas nunca abrem um caminho que não passou por aqui.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Roots {
    roots: Vec<PathBuf>,
}

/// Tira o prefixo `\\?\` que o `canonicalize` do Windows põe (feio na saída para o agente).
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) => rest.to_string(),
        None => s.to_string(),
    }
}

/// Divide `BSP_MUSEUM_ROOTS`: `:` ou `;` (um `C:\` do Windows não é cortado ao meio).
pub fn split_roots_env(value: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = value.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let drive_colon = c == ':'
            && cur.chars().count() == 1
            && cur.chars().all(|d| d.is_ascii_alphabetic())
            && matches!(chars.get(i + 1), Some('\\') | Some('/'));
        if (c == ':' && !drive_colon) || c == ';' {
            if !cur.trim().is_empty() {
                out.push(PathBuf::from(cur.trim()));
            }
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        out.push(PathBuf::from(cur.trim()));
    }
    out
}

impl Roots {
    /// Canonicaliza as raízes. Lista vazia = só o diretório de trabalho atual.
    pub fn new(paths: &[PathBuf]) -> Result<Self, String> {
        let mut roots: Vec<PathBuf> = Vec::new();
        if paths.is_empty() {
            let cwd = std::env::current_dir().map_err(|e| format!("sem diretório de trabalho: {e}"))?;
            roots.push(cwd.canonicalize().map_err(|e| format!("raiz inválida ({}): {e}", cwd.display()))?);
        }
        for p in paths {
            let c = p.canonicalize().map_err(|e| format!("raiz inválida ({}): {e}", p.display()))?;
            if !c.is_dir() {
                return Err(format!("a raiz não é uma pasta: {}", p.display()));
            }
            if !roots.contains(&c) {
                roots.push(c);
            }
        }
        Ok(Self { roots })
    }

    pub fn list(&self) -> &[PathBuf] {
        &self.roots
    }

    /// `canonical` (já canonicalizado) está em alguma raiz.
    pub fn contains(&self, canonical: &Path) -> bool {
        self.roots.iter().any(|r| canonical.starts_with(r))
    }

    fn denied(&self, input: &str) -> String {
        let roots: Vec<String> = self.roots.iter().map(|r| display_path(r)).collect();
        format!("caminho não encontrado ou fora das raízes permitidas: {input} (raízes: {})", roots.join(", "))
    }

    fn check_raw(input: &str) -> Result<(), String> {
        if input.trim().is_empty() {
            return Err("caminho vazio".to_string());
        }
        if input.contains('\0') {
            return Err("caminho com byte nulo".to_string());
        }
        // recusa `..` antes de qualquer acesso ao disco, mesmo que o destino ficasse dentro da raiz
        let norm = input.replace('\\', "/");
        if norm.split('/').any(|c| c == "..") {
            return Err(format!("`..` não é aceito em caminhos: {input}"));
        }
        Ok(())
    }

    /// Candidatos de um caminho vindo de fora: absoluto vale como está; relativo é procurado
    /// embaixo de cada raiz, na ordem.
    fn candidates(&self, input: &str) -> Vec<PathBuf> {
        let p = Path::new(input);
        if p.is_absolute() {
            vec![p.to_path_buf()]
        } else {
            self.roots.iter().map(|r| r.join(p)).collect()
        }
    }

    /// Caminho existente (arquivo ou pasta), canonicalizado e dentro de uma raiz.
    /// Inexistente e fora da raiz dão a mesma mensagem: não serve de oráculo sobre o disco.
    pub fn resolve(&self, input: &str) -> Result<PathBuf, String> {
        Self::check_raw(input)?;
        for cand in self.candidates(input) {
            if let Ok(canon) = cand.canonicalize() {
                if self.contains(&canon) {
                    return Ok(canon);
                }
            }
        }
        Err(self.denied(input))
    }

    /// Como `resolve`, exigindo uma pasta.
    pub fn resolve_dir(&self, input: &str) -> Result<PathBuf, String> {
        let p = self.resolve(input)?;
        if p.is_dir() {
            Ok(p)
        } else {
            Err(format!("não é uma pasta: {input}"))
        }
    }

    /// Como `resolve`, exigindo um arquivo comum.
    pub fn resolve_file(&self, input: &str) -> Result<PathBuf, String> {
        let p = self.resolve(input)?;
        if p.is_file() {
            Ok(p)
        } else {
            Err(format!("não é um arquivo: {input}"))
        }
    }

    /// Pasta de destino para gravação, que pode ainda não existir: o trecho existente é
    /// canonicalizado e precisa estar numa raiz; o resto só pode ter nomes comuns.
    pub fn resolve_for_write(&self, input: &str) -> Result<PathBuf, String> {
        Self::check_raw(input)?;
        let cand = self.candidates(input).into_iter().next().ok_or_else(|| self.denied(input))?;
        let mut existing = cand.clone();
        let mut rest: Vec<std::ffi::OsString> = Vec::new();
        loop {
            if let Ok(canon) = existing.canonicalize() {
                if !self.contains(&canon) {
                    return Err(self.denied(input));
                }
                let mut out = canon;
                for part in rest.iter().rev() {
                    out.push(part);
                }
                return Ok(out);
            }
            let Some(name) = existing.file_name().map(|n| n.to_os_string()) else { return Err(self.denied(input)) };
            if !matches!(existing.components().next_back(), Some(Component::Normal(_))) {
                return Err(self.denied(input));
            }
            rest.push(name);
            if !existing.pop() {
                return Err(self.denied(input));
            }
        }
    }

    /// Caminho para mostrar ao agente: sempre absoluto (com mais de uma raiz o relativo seria ambíguo).
    pub fn show(&self, p: &Path) -> String {
        display_path(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::testutil::Tmp;

    #[test]
    fn aceita_dentro_e_recusa_fora() {
        let a = Tmp::new();
        let b = Tmp::new();
        std::fs::write(a.0.join("x.bsp"), b"x").unwrap();
        std::fs::write(b.0.join("y.bsp"), b"y").unwrap();
        let roots = Roots::new(std::slice::from_ref(&a.0)).unwrap();
        assert!(roots.resolve(a.0.join("x.bsp").to_str().unwrap()).is_ok());
        assert!(roots.resolve("x.bsp").is_ok(), "relativo é procurado nas raízes");
        let fora = roots.resolve(b.0.join("y.bsp").to_str().unwrap()).unwrap_err();
        assert!(fora.contains("fora das raízes"), "{fora}");
    }

    #[test]
    fn recusa_ponto_ponto_mesmo_quando_cai_dentro() {
        let a = Tmp::new();
        std::fs::create_dir_all(a.0.join("sub")).unwrap();
        std::fs::write(a.0.join("x.bsp"), b"x").unwrap();
        let roots = Roots::new(std::slice::from_ref(&a.0)).unwrap();
        let p = format!("{}/sub/../x.bsp", a.0.display());
        assert!(roots.resolve(&p).unwrap_err().contains(".."));
        assert!(roots.resolve("..\\x.bsp").unwrap_err().contains(".."));
        assert!(roots.resolve("a\0b").is_err());
        assert!(roots.resolve("").is_err());
    }

    #[test]
    fn inexistente_e_fora_dao_a_mesma_mensagem() {
        let a = Tmp::new();
        let b = Tmp::new();
        std::fs::write(b.0.join("y.bsp"), b"y").unwrap();
        let roots = Roots::new(std::slice::from_ref(&a.0)).unwrap();
        let existe_fora = roots.resolve(b.0.join("y.bsp").to_str().unwrap()).unwrap_err();
        let nao_existe = roots.resolve(b.0.join("zzz.bsp").to_str().unwrap()).unwrap_err();
        assert_eq!(existe_fora.replace("y.bsp", "?"), nao_existe.replace("zzz.bsp", "?"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_que_foge_da_raiz_e_recusado() {
        let a = Tmp::new();
        let b = Tmp::new();
        std::fs::write(b.0.join("segredo.bsp"), b"s").unwrap();
        std::os::unix::fs::symlink(b.0.join("segredo.bsp"), a.0.join("atalho.bsp")).unwrap();
        std::os::unix::fs::symlink(&b.0, a.0.join("pasta")).unwrap();
        let roots = Roots::new(std::slice::from_ref(&a.0)).unwrap();
        assert!(roots.resolve(a.0.join("atalho.bsp").to_str().unwrap()).is_err());
        assert!(roots.resolve(a.0.join("pasta").to_str().unwrap()).is_err());
        assert!(roots.resolve(a.0.join("pasta/segredo.bsp").to_str().unwrap()).is_err());
        assert!(roots.resolve_for_write(a.0.join("pasta/novo").to_str().unwrap()).is_err());
    }

    #[test]
    fn gravacao_so_em_subpasta_permitida_mesmo_inexistente() {
        let a = Tmp::new();
        let b = Tmp::new();
        let roots = Roots::new(std::slice::from_ref(&a.0)).unwrap();
        let ok = roots.resolve_for_write(a.0.join("novo/fundo").to_str().unwrap()).unwrap();
        assert_eq!(ok, a.0.join("novo").join("fundo"));
        assert!(roots.resolve_for_write(b.0.join("novo").to_str().unwrap()).is_err());
        assert!(roots.resolve_for_write("novo/../../x").is_err());
        assert_eq!(roots.resolve_for_write("saida").unwrap(), a.0.join("saida"));
    }

    #[test]
    fn sem_raiz_usa_o_diretorio_de_trabalho() {
        let roots = Roots::new(&[]).unwrap();
        assert_eq!(roots.list().len(), 1);
        assert_eq!(roots.list()[0], std::env::current_dir().unwrap().canonicalize().unwrap());
    }

    #[test]
    fn raiz_inexistente_e_erro() {
        assert!(Roots::new(&[PathBuf::from("/nao/existe/mesmo")]).is_err());
    }

    #[test]
    fn env_divide_por_dois_pontos_e_ponto_e_virgula() {
        assert_eq!(split_roots_env("/a:/b;/c"), vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]);
        assert_eq!(split_roots_env("C:\\jogos;D:/x"), vec![PathBuf::from("C:\\jogos"), PathBuf::from("D:/x")]);
        assert!(split_roots_env("  ").is_empty());
    }
}
