//! Vault: layout, bootstrap, guardrails, registry, projections.

pub mod layout;
pub mod bootstrap;
pub mod registry;
pub mod recall;
pub mod status;
pub mod lint;
pub mod pages;
pub mod capture;
pub mod ingest;

pub use bootstrap::{bootstrap, BootstrapError};
pub use layout::{VaultPaths, SPACE_PERSONAL};
