//! Servidor MCP (Model Context Protocol) do bsp-museum: expõe o acervo GoldSrc a *agent
//! harnesses* por JSON-RPC 2.0 sobre stdio, somente leitura.
//!
//! - `protocol`: mensagens, handshake e despacho — sem I/O, testável em memória;
//! - `tools` (+ `tools_map`, `tools_files`): registro das ferramentas, que só chamam o núcleo;
//! - `resources`, `prompts`: recursos e receitas MCP;
//! - `sandbox`: raízes permitidas — única porta para o disco;
//! - `output`: paginação e corte de respostas grandes;
//! - `stdio`: o laço de linhas que o binário `bsp-museum-mcp` liga a stdin/stdout.

pub mod cli;
pub mod output;
pub mod prompts;
pub mod protocol;
pub mod resources;
pub mod sandbox;
pub mod schema;
pub mod stdio;
pub mod tools;
mod tools_files;
mod tools_map;
#[cfg(test)]
pub mod testutil;
#[cfg(test)]
mod tests;

pub use protocol::Server;
pub use sandbox::Roots;
pub use tools::Config;

pub const SERVER_NAME: &str = "bsp-museum";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
