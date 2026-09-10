//! Status: health verdict from the registry. Mechanical, O(registry).

use std::fs;

use serde::Serialize;

use super::layout::VaultPaths;
use super::registry::Registry;

#[derive(Debug, Serialize, PartialEq)]
pub struct Status {
    pub total_pages: u64,
    pub by_type: std::collections::BTreeMap<String, u64>,
    pub orphans: u64,
    pub gaps: u64,
    /// "empty" | "good" | "warning"
    pub health: String,
}

/// Orphans = pages with no inbound links.
pub fn orphans_of(registry: &Registry) -> Vec<String> {
    let inbound = super::registry::inbound_links(registry);
    let mut out: Vec<String> = registry
        .pages
        .keys()
        .filter(|id| inbound.get(*id).map(|v| v.is_empty()).unwrap_or(true))
        .cloned()
        .collect();
    out.sort();
    out
}

pub fn compute(vault: &VaultPaths, registry: &Registry) -> Status {
    let mut by_type = std::collections::BTreeMap::new();
    for p in registry.pages.values() {
        *by_type.entry(p.page_type.clone()).or_insert(0u64) += 1;
    }
    let orphan_count = orphans_of(registry).len() as u64;
    let gaps = read_gap_count(vault);
    let health = if registry.pages.is_empty() {
        "empty"
    } else if orphan_count > 5 {
        "warning"
    } else {
        "good"
    };
    Status {
        total_pages: registry.pages.len() as u64,
        by_type,
        orphans: orphan_count,
        gaps,
        health: health.to_string(),
    }
}

fn read_gap_count(vault: &VaultPaths) -> u64 {
    let path = vault.discoveries().join("gaps.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return 0;
    };
    #[derive(serde::Deserialize)]
    struct Gaps {
        #[serde(default)]
        gaps: std::collections::BTreeMap<String, serde_json::Value>,
    }
    serde_json::from_str::<Gaps>(&raw)
        .map(|g| g.gaps.len() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::bootstrap::bootstrap;

    #[test]
    fn counts_orphans_and_health() {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        bootstrap(&v, "t").unwrap();
        let mut reg = Registry::default();
        for id in ["concepts/a", "concepts/b", "entities/c"] {
            reg.pages.insert(
                id.into(),
                super::super::registry::PageEntry {
                    id: id.into(),
                    title: id.into(),
                    page_type: super::super::pages::type_for_folder(id.split('/').next().unwrap()),
                    path: format!("wiki/{id}.md"),
                    links: vec![],
                    excerpt: String::new(),
                    description: String::new(),
                    source_id: None,
                },
            );
        }
        // a -> b link; c orphan
        reg.pages.get_mut("concepts/a").unwrap().links = vec!["concepts/b".into()];
        let st = compute(&v, &reg);
        assert_eq!(st.total_pages, 3);
        assert_eq!(st.orphans, 2); // c orphan; a links out but nothing links a
        assert_eq!(st.health, "good");
    }
}
