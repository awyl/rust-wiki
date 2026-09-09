//! rust-wiki: remote zosmaai-style wiki MCP server.
//!
//! Mechanical engine only — vaults, registry, projections, recall, lint.
//! Synthesis is the calling agent's job (cooperative `wiki_ingest`).

pub mod api;
pub mod config;
pub mod hub;
pub mod server;
pub mod vault;
