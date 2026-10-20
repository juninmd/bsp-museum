//! Núcleo do bsp-museum: leitura dos formatos do GoldSrc e análise dos mapas.
//! Sem dependência de interface — é usado pelo app (Tauri) e pelo servidor MCP.

pub mod audio;
pub mod audit;
pub mod bsp;
pub mod catalog;
pub mod compare;
pub mod demo;
pub mod diagnostics;
pub mod entity_list;
pub mod mdl;
pub mod modfiles;
pub mod nav;
pub mod pak;
pub mod radar;
pub mod resources;
pub mod textfiles;
pub mod util;
pub mod spr; pub mod wadlist; pub mod image;
#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_novos;
