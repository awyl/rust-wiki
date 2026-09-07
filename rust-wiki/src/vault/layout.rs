//! Vault layout: canonical paths inside one space, and ownership
//! classification used by guardrails (raw/meta immutable, wiki writable).
//! Flat layout: the space dir IS the vault (no .llm-wiki nesting).

use std::path::{Path, PathBuf};

/// Reserved cross-project space merged into `wiki_recall`.
pub const SPACE_PERSONAL: &str = "personal";

/// Canonical directories of one space's vault (flattened zosmaai layout).
#[derive(Debug, Clone)]
pub struct VaultPaths {
    /// Space root, e.g. `<vault_root>/<space>/`
    pub space_root: PathBuf,
}

impl VaultPaths {
    pub fn new(vault_root: &Path, space: &str) -> Self {
        Self {
            space_root: vault_root.join(space),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.space_root.join("config.json")
    }
    pub fn templates(&self) -> PathBuf {
        self.space_root.join("templates")
    }
    pub fn raw(&self) -> PathBuf {
        self.space_root.join("raw")
    }
    pub fn raw_sources(&self) -> PathBuf {
        self.raw().join("sources")
    }
    pub fn wiki_pages(&self) -> PathBuf {
        self.space_root.join("wiki")
    }
    pub fn meta(&self) -> PathBuf {
        self.space_root.join("meta")
    }
    pub fn registry_file(&self) -> PathBuf {
        self.meta().join("registry.json")
    }
    pub fn backlinks_file(&self) -> PathBuf {
        self.meta().join("backlinks.json")
    }
    pub fn index_file(&self) -> PathBuf {
        self.meta().join("index.md")
    }
    pub fn log_file(&self) -> PathBuf {
        self.meta().join("log.md")
    }
    pub fn events_file(&self) -> PathBuf {
        self.meta().join("events.jsonl")
    }
    pub fn outputs(&self) -> PathBuf {
        self.space_root.join("outputs")
    }
    pub fn discoveries(&self) -> PathBuf {
        self.space_root.join(".discoveries")
    }

    /// A wiki page path for a folder-qualified id like `concepts/rag`.
    pub fn page_path(&self, page_id: &str) -> PathBuf {
        self.wiki_pages().join(format!("{page_id}.md"))
    }
}

/// Ownership class of a vault-relative path. Guardrails map:
/// - `Raw` and `Meta` are server-owned (immutable / generated)
/// - `Wiki` is agent+user editable
/// - `Other` is anything outside the standard dirs (treated as server-owned)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Raw,
    Meta,
    Wiki,
    Other,
}

/// Classify a path (absolute or vault-relative) against a vault.
pub fn ownership(vault: &VaultPaths, path: &Path) -> Ownership {
    let Ok(rel) = path.strip_prefix(&vault.space_root) else {
        return Ownership::Other;
    };
    match rel.components().next() {
        Some(c) => match c.as_os_str().to_string_lossy().as_ref() {
            "raw" => Ownership::Raw,
            "meta" => Ownership::Meta,
            "wiki" => Ownership::Wiki,
            _ => Ownership::Other,
        },
        None => Ownership::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> VaultPaths {
        VaultPaths::new(Path::new("/data/vaults"), "test-space")
    }

    #[test]
    fn paths_are_flat_under_space_root() {
        let v = vault();
        assert_eq!(
            v.config_file(),
            PathBuf::from("/data/vaults/test-space/config.json")
        );
        assert_eq!(
            v.raw_sources(),
            PathBuf::from("/data/vaults/test-space/raw/sources")
        );
        assert_eq!(
            v.registry_file(),
            PathBuf::from("/data/vaults/test-space/meta/registry.json")
        );
        assert_eq!(
            v.page_path("concepts/rag"),
            PathBuf::from("/data/vaults/test-space/wiki/concepts/rag.md")
        );
        assert_eq!(
            v.discoveries(),
            PathBuf::from("/data/vaults/test-space/.discoveries")
        );
    }

    #[test]
    fn ownership_rules() {
        let v = vault();
        let space = v.space_root.clone();
        assert_eq!(
            ownership(&v, &space.join("raw/sources/SRC-1/extracted.md")),
            Ownership::Raw
        );
        assert_eq!(
            ownership(&v, &space.join("meta/registry.json")),
            Ownership::Meta
        );
        assert_eq!(
            ownership(&v, &space.join("wiki/concepts/rag.md")),
            Ownership::Wiki
        );
        assert_eq!(
            ownership(&v, &space.join("unrelated.txt")),
            Ownership::Other
        );
        assert_eq!(ownership(&v, Path::new("/etc/passwd")), Ownership::Other);
        assert_eq!(
            ownership(&v, &space.join("templates/concept.md")),
            Ownership::Other
        );
    }
}
