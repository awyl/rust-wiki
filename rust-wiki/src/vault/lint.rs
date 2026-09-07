//! Lint: deterministic health checks — orphans, missing pages,
//! contradiction markers, knowledge gaps; optional auto-fix stubs.

use std::fs;

use serde::Serialize;

use super::layout::VaultPaths;
use super::registry::{rebuild_metadata, Registry};

/// Content marker treated as a contradiction flag (human review required).
pub const CONTRADICTION_MARKER: &str = "⚠️";

#[derive(Debug, Serialize, PartialEq)]
pub struct LintReport {
    pub pages: u64,
    pub orphans: Vec<String>,
    pub missing_pages: Vec<String>,
    pub contradictions: Vec<String>,
    pub auto_fixed: Vec<String>,
}

pub fn run(vault: &VaultPaths, registry: &Registry, auto_fix: bool) -> Result<LintReport, String> {
    let inbound = super::registry::inbound_links(registry);
    let orphans = super::status::orphans_of(registry);
    let mut missing: Vec<String> = inbound
        .keys()
        .filter(|id| !registry.pages.contains_key(*id))
        .cloned()
        .collect();
    missing.sort();

    let mut contradictions: Vec<String> = registry
        .pages
        .values()
        .filter(|p| {
            fs::read_to_string(vault.space_root.join(&p.path))
                .map(|c| c.contains(CONTRADICTION_MARKER) && c.contains("Contradiction"))
                .unwrap_or(false)
        })
        .map(|p| p.id.clone())
        .collect();
    contradictions.sort();

    let mut auto_fixed = Vec::new();
    if auto_fix {
        // stub a missing page when ≥2 distinct pages cite it
        for id in &missing {
            if inbound.get(id).map(|v| v.len()).unwrap_or(0) >= 2 {
                create_stub(vault, id)?;
                auto_fixed.push(id.clone());
            }
        }
        if !auto_fixed.is_empty() {
            rebuild_metadata(vault)?;
        }
    }
    track_gaps(vault, &missing)?;
    Ok(LintReport {
        pages: registry.pages.len() as u64,
        orphans,
        missing_pages: missing,
        contradictions,
        auto_fixed,
    })
}

fn create_stub(vault: &VaultPaths, id: &str) -> Result<(), String> {
    // stubs are concepts by default
    let title = id.rsplit('/').next().unwrap_or(id).replace('-', " ");
    let page_id = format!("concepts/{}", slugify(&title));
    let path = vault.page_path(&page_id);
    if path.exists() {
        return Ok(());
    }
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(
        &path,
        format!(
            "---\ntitle: \"{title}\"\ntype: concept\n---\n\n# {title}\n\nStub created by lint auto_fix — fill in what this concept means and why it is cited.\n"
        ),
    )
    .map_err(|e| e.to_string())
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn track_gaps(vault: &VaultPaths, missing: &[String]) -> Result<(), String> {
    if missing.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(vault.discoveries()).map_err(|e| e.to_string())?;
    let path = vault.discoveries().join("gaps.json");
    let mut gaps: std::collections::BTreeMap<String, u64> = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    for id in missing {
        *gaps.entry(id.clone()).or_default() += 1;
    }
    fs::write(&path, serde_json::to_string_pretty(&gaps).unwrap()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, VaultPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        super::super::bootstrap::bootstrap(&v, "t").unwrap();
        (tmp, v)
    }

    fn page(v: &VaultPaths, id: &str, body: &str) {
        let p = v.page_path(id);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn detects_missing_contradiction_and_autofixes_stubs() {
        let (_t, v) = setup();
        page(&v, "concepts/a", "# A\n\nlinks [[concepts/b]]\n");
        page(
            &v,
            "concepts/c",
            "# C\n\nalso links [[concepts/b]]\n⚠️ **Contradiction:** X vs Y\n",
        );
        let reg = rebuild_metadata(&v).unwrap();
        let r = run(&v, &reg, true).unwrap();
        assert_eq!(r.missing_pages, vec!["concepts/b"]);
        assert_eq!(r.contradictions, vec!["concepts/c"]);
        assert_eq!(r.auto_fixed, vec!["concepts/b"]);
        // stub now exists and registry refreshed
        assert!(v.page_path("concepts/b").exists());
        let reg2 = rebuild_metadata(&v).unwrap();
        assert!(reg2.pages.contains_key("concepts/b"));
        // gap tracked
        let gaps = fs::read_to_string(v.discoveries().join("gaps.json")).unwrap();
        assert!(gaps.contains("concepts/b"));
    }
}
