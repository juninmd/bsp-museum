//! Núcleo do bsp-museum: leitura dos formatos do GoldSrc e análise dos mapas.
//! Sem dependência de interface — é usado pelo app (Tauri) e pelo servidor MCP.

pub mod audit;
pub mod bsp;
pub mod catalog;
pub mod compare;
pub mod diagnostics;
pub mod entity_list;
pub mod hulls;
pub mod mdl;
pub mod radar;
pub mod resources;
pub mod scene;
pub mod util;
pub mod volumes;
#[cfg(test)]
mod fixture;
#[cfg(test)]
mod fixture_extra;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_extra;
#[cfg(test)]
mod tests_novos;
