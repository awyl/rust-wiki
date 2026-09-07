//! Vault layout: canonical paths inside one space, and ownership
//! classification used by guardrails (raw/meta immutable, wiki writable).

use std::path::{Path, PathBuf};

/// Reserved cross-project space merged into `wiki_recall`.
pub const SPACE_PERSONAL: &str = "personal";

/// Canonical directories of one space's vault (zosmaai layout).
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

    pub fn wiki(&self) -> PathBuf {
        self.space_root.join(".llm-wiki")
    }
    pub fn config_file(&self) -> PathBuf {
        self.wiki().join("config.json")
    }
    pub fn templates(&self) -> PathBuf {
        self.wiki().join("templates")
    }
    pub fn raw(&self) -> PathBuf {
        self.wiki().join("raw")
    }
    pub fn raw_sources(&self) -> PathBuf {
        self.raw().join("sources")
    }
    pub fn wiki_pages(&self) -> PathBuf {
        self.wiki().join("wiki")
    }
    pub fn meta(&self) -> PathBuf {
        self.wiki().join("meta")
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
        self.wiki().join("outputs")
    }
    pub fn discoveries(&self) -> PathBuf {
        self.wiki().join(".discoveries")
    }

    /// A wiki page path for a folder-qualified id like `concepts/rag`.
    pub fn page_path(&self, page_id: &str) -> PathBuf {
        self.wiki_pages().join(format!("{page_id}.md"))
    }
}

/// Ownership class of a vault-relative path. Guardrails map:
/// - `Raw` and `Meta` are server-owned (immutable / generated)
/// - `Wiki` is agent+user editable
/// - `Other` is anything outside `.llm-wiki/` (treated as server-owned)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Raw,
    Meta,
    Wiki,
    Other,
}

/// Classify a path (absolute or vault-relative) against a vault.
pub fn ownership(vault: &VaultPaths, path: &Path) -> Ownership {
    let rel = match path.strip_prefix(&vault.space_root) {
        Ok(r) => r.to_path_buf(),
        Err(_) => match path.strip_prefix(vault.wiki()) {
            Ok(r) => PathBuf::from(".llm-wiki").join(r),
            Err(_) => return Ownership::Other,
        },
    };
    let mut comps = rel.components();
    // first meaningful component decides
    while let Some(c) = comps.next() {
        let c = c.as_os_str().to_string_lossy().into_owned();
        if c == ".llm-wiki" {
            continue;
        }
        return match c.as_str() {
            "raw" => Ownership::Raw,
            "meta" => Ownership::Meta,
            "wiki" => Ownership::Wiki,
            _ => Ownership::Other,
        };
    }
    Ownership::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> VaultPaths {
        VaultPaths::new(Path::new("/data/vaults"), "test-space")
    }

    #[test]
    fn paths_follow_zosmaai_layout() {
        let v = vault();
        assert_eq!(v.wiki(), PathBuf::from("/data/vaults/test-space/.llm-wiki"));
        assert_eq!(v.raw_sources(), PathBuf::from("/data/vaults/test-space/.llm-wiki/raw/sources"));
        assert_eq!(v.registry_file(), PathBuf::from("/data/vaults/test-space/.llm-wiki/meta/registry.json"));
        assert_eq!(v.page_path("concepts/rag"), PathBuf::from("/data/vaults/test-space/.llm-wiki/wiki/concepts/rag.md"));
    }

    #[test]
    fn ownership_rules() {
        let v = vault();
        let space = v.space_root.clone();
        assert_eq!(ownership(&v, &space.join(".llm-wiki/raw/sources/SRC-1/extracted.md")), Ownership::Raw);
        assert_eq!(ownership(&v, &space.join(".llm-wiki/meta/registry.json")), Ownership::Meta);
        assert_eq!(ownership(&v, &space.join(".llm-wiki/wiki/concepts/rag.md")), Ownership::Wiki);
        assert_eq!(ownership(&v, &space.join("unrelated.txt")), Ownership::Other);
        // absolute path outside the vault entirely
        assert_eq!(ownership(&v, Path::new("/etc/passwd")), Ownership::Other);
        // wiki-relative shorthand (already inside .llm-wiki)
        assert_eq!(ownership(&v, &v.wiki().join("wiki/concepts/x.md")), Ownership::Wiki);
    }
}
