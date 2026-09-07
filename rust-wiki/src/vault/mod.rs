//! Vault: layout, bootstrap, guardrails, registry, projections.

pub mod layout;
pub mod bootstrap;
pub mod registry;

pub use bootstrap::{bootstrap, BootstrapError};
pub use layout::{VaultPaths, SPACE_PERSONAL};
