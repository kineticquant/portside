//! Shared core: the desktop app, the CLI, and the MCP server all call this.
//! Tauri `#[command]` functions are plain async fns — directly reusable.
pub mod catalog;
pub mod certs;
pub mod connect;
pub mod docker;
#[cfg(test)]
mod e2e;
pub mod health;
pub mod import;
pub mod prereqs;
pub mod schemas;
pub mod state;
