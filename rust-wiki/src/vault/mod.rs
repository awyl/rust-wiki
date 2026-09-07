//! Vault: layout, bootstrap, guardrails, registry, projections.

pub mod bootstrap;
pub mod capture;
pub mod ingest;
pub mod layout;
pub mod lint;
pub mod pages;
pub mod recall;
pub mod registry;
pub mod status;

pub use bootstrap::{bootstrap, BootstrapError};
pub use layout::{VaultPaths, SPACE_PERSONAL};
pub mod okf;
pub mod watch;
