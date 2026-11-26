//! The CLI's modules, as a library its binaries share: `takt` (`main.rs`:
//! the commands, the terminal UI and the stdio MCP server) and, with the
//! `remote` feature, `takt-mcp` (`bin/takt-mcp.rs`: the hosted MCP server,
//! `remote/`). Four front ends onto the one tool table in `tools.rs`.

pub mod checkvist;
pub mod cli;
pub mod config;
pub mod error;
pub mod local;
pub mod lock;
pub mod mcp;
#[cfg(feature = "remote")]
pub mod remote;
#[cfg(test)]
mod tests;
pub mod tools;
pub mod tui;
pub mod workspace;
pub mod workspace_tasks;
#[cfg(test)]
mod workspace_tests;
