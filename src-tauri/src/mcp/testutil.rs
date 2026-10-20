//! Apoio dos testes do servidor MCP: pasta temporária e mapas sintéticos.

use crate::fixture::{museum_bsp, Options};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static N: AtomicU32 = AtomicU32::new(0);

/// Pasta temporária única (canonicalizada), apagada ao sair de escopo.
pub struct Tmp(pub PathBuf);

impl Default for Tmp {
    fn default() -> Self {
        Self::new()
    }
}

impl Tmp {
    pub fn new() -> Self {
        let p = std::env::temp_dir().join(format!("bsp-museum-mcp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p.canonicalize().unwrap())
    }

    pub fn put(&self, rel: &str, data: &[u8]) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, data).unwrap();
        p
    }

    pub fn s(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().to_string()
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Monta `<tmp>/cstrike/maps/de_museu.bsp` e `de_curto.bsp` (1 sala, sem VIS) e devolve a pasta temporária.
pub fn mod_with_maps() -> Tmp {
    let t = Tmp::new();
    t.put("cstrike/maps/de_museu.bsp", &museum_bsp(Options::default()));
    t.put("cstrike/maps/de_curto.bsp", &museum_bsp(Options { rooms: 1, vis: false, ..Options::default() }));
    t
}
