//! OKF v0.2 mode: knowledge_format resolution + deterministic projections.
//! Adapted subset of zosmaai's OKF Foundation spec — deviations documented
//! in docs/design-spec.md (lenient frontmatter, regex links, ASCII slugs).

use std::collections::BTreeMap;
use std::fs;

use serde::Deserialize;

use super::layout::VaultPaths;
use super::registry::Registry;

pub const OKF_VERSION: &str = "0.2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Legacy,
    Okf,
}

/// Reserved generated files (okf mode): never directly editable.
pub fn is_reserved(vault: &VaultPaths, id: &str) -> bool {
    if mode(vault) != Ok(Mode::Okf) {
        return false;
    }
    id == "index" || id == "log" || id.ends_with("/index")
}

/// Read knowledge_format from vault config. Absent = legacy; unknown = fail closed.
pub fn mode(vault: &VaultPaths) -> Result<Mode, String> {
    #[derive(Deserialize)]
    struct Cfg {
        #[serde(default)]
        knowledge_format: Option<String>,
    }
    let raw = fs::read_to_string(vault.config_file()).map_err(|e| e.to_string())?;
    let cfg: Cfg = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    match cfg.knowledge_format.as_deref() {
        None => Ok(Mode::Legacy),
        Some("legacy") => Ok(Mode::Legacy),
        Some("okf-0.2") => Ok(Mode::Okf),
        Some(other) => Err(format!(
            "config_invalid_knowledge_format: unknown knowledge_format '{other}' (legacy | okf-0.2)"
        )),
    }
}

/// Deterministic directory indexes (root + every concept-bearing dir).
/// Template per OKF Foundation: Directories before Concepts, sorted by
/// relative path, ` — description` only when non-empty.
pub fn write_dir_indexes(
    vault: &VaultPaths,
    registry: &Registry,
    vault_name: &str,
) -> Result<(), String> {
    fn escape_label(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if matches!(c, '\\' | '[' | ']') {
                out.push('\\');
            }
            out.push(c);
        }
        out
    }

    fn render(
        dir: &str,
        entries: &[(&str, &str, &str)],
        subdirs: &[String],
        vault_name: &str,
    ) -> String {
        let mut out = String::new();
        if dir.is_empty() {
            out.push_str(&format!(
                "---\nokf_version: \"{OKF_VERSION}\"\n---\n\n# {vault_name}\n\n"
            ));
        } else {
            let name = dir.rsplit('/').next().unwrap_or(dir);
            out.push_str(&format!("# {name}\n\n"));
        }
        if !subdirs.is_empty() {
            out.push_str("## Directories\n\n");
            for d in subdirs {
                let label = d.rsplit('/').next().unwrap_or(d);
                out.push_str(&format!("- [{label}/]({d}/)\n"));
            }
            out.push('\n');
        }
        if !entries.is_empty() {
            out.push_str("## Concepts\n\n");
            for (id, title, description) in entries {
                let file = id.rsplit('/').next().unwrap_or(id);
                let label = escape_label(if title.is_empty() { file } else { title });
                out.push_str(&format!("- [{label}]({file}.md)"));
                if !description.is_empty() {
                    let collapsed: String =
                        description.split_whitespace().collect::<Vec<_>>().join(" ");
                    out.push_str(&format!(" — {collapsed}"));
                }
                out.push('\n');
            }
            out.push('\n');
        }
        out
    }

    // dir -> immediate subdirs; dir -> concepts (id, title, description)
    let mut dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut concepts: BTreeMap<String, Vec<(&str, &str, &str)>> = BTreeMap::new();
    for p in registry.pages.values() {
        let dir = match p.id.rsplit_once('/') {
            Some((d, _)) => d.to_string(),
            None => String::new(),
        };
        concepts.entry(dir.clone()).or_default().push((
            p.id.as_str(),
            p.title.as_str(),
            p.description.as_str(),
        ));
        dirs.entry(dir.clone()).or_default();
        // ancestors hold transitive concepts
        let mut ancestor = dir.clone();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            ancestor = parent.to_string();
            dirs.entry(ancestor.clone()).or_default();
        }
    }
    // parent -> immediate subdir links
    let parent_links: Vec<(String, String)> = dirs
        .keys()
        .filter(|d| !d.is_empty())
        .map(|d| match d.rsplit_once('/') {
            Some((parent, child)) => (parent.to_string(), child.to_string()),
            None => (String::new(), d.clone()),
        })
        .collect();
    for (parent, child) in parent_links {
        dirs.entry(parent).or_default().push(child);
    }
    for subs in dirs.values_mut() {
        subs.sort();
        subs.dedup();
    }

    let empty: Vec<(&str, &str, &str)> = Vec::new();
    let no_subs: Vec<String> = Vec::new();

    let write_at =
        |dir: &str, entries: &[(&str, &str, &str)], subdirs: &[String]| -> Result<(), String> {
            let target = if dir.is_empty() {
                vault.wiki_pages().join("index.md")
            } else {
                vault.wiki_pages().join(format!("{dir}/index.md"))
            };
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&target, render(dir, entries, subdirs, vault_name)).map_err(|e| e.to_string())
        };

    write_at(
        "",
        concepts.get("").map(|v| v.as_slice()).unwrap_or(&empty),
        dirs.get("").map(|v| v.as_slice()).unwrap_or(&no_subs),
    )?;
    for (dir, subdirs) in &dirs {
        if dir.is_empty() {
            continue;
        }
        let entries = concepts.get(dir).map(|v| v.as_slice()).unwrap_or(&empty);
        write_at(dir, entries, subdirs)?;
    }

    // prune generated dir indexes whose dirs no longer hold concepts
    fn prune(
        wiki_root: &std::path::Path,
        keep: &BTreeMap<String, Vec<String>>,
        rel: &str,
    ) -> Result<(), String> {
        for e in fs::read_dir(wiki_root).map_err(|e| e.to_string())? {
            let p = e.map_err(|e| e.to_string())?.path();
            if p.is_dir() {
                let name = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let child_rel = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{rel}/{name}")
                };
                prune(&p, keep, &child_rel)?;
            }
        }
        // The root index always exists in OKF mode (it carries the
        // okf_version frontmatter); only per-directory indexes are pruned
        // when a directory loses its concepts.
        if !rel.is_empty() && wiki_root.join("index.md").is_file() && !keep.contains_key(rel) {
            fs::remove_file(wiki_root.join("index.md")).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    prune(&vault.wiki_pages(), &dirs, "")
}

/// Deterministic root log from events.jsonl (okf mode projection).
/// Grouped by UTC date, newest first; details as canonical sorted-key JSON.
pub fn write_okf_log(vault: &VaultPaths) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Ev {
        timestamp: String,
        kind: String,
        details: serde_json::Map<String, serde_json::Value>,
    }
    let mut events: Vec<Ev> = Vec::new();
    if vault.events_file().exists() {
        let raw = fs::read_to_string(vault.events_file()).map_err(|e| e.to_string())?;
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Ev>(line) {
                Ok(ev) => events.push(ev),
                Err(_) => continue, // event_invalid_json: omit from projection
            }
        }
    }
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // newest first
    let mut by_date: BTreeMap<String, Vec<&Ev>> = BTreeMap::new();
    for ev in &events {
        let date = ev.timestamp.get(..10).unwrap_or("?").to_string();
        by_date.entry(date).or_default().push(ev);
    }
    let mut out = String::from("# Wiki Update Log\n\n");
    for (date, evs) in by_date.iter().rev() {
        out.push_str(&format!("## {date}\n\n"));
        for ev in evs {
            let canonical = canonical_json(&serde_json::Value::Object(ev.details.clone()));
            if canonical == "{}" {
                out.push_str(&format!("- **{}**\n", ev.kind));
            } else {
                out.push_str(&format!("- **{}**: {canonical}\n", ev.kind));
            }
        }
        out.push('\n');
    }
    fs::write(vault.wiki_pages().join("log.md"), out).map_err(|e| e.to_string())
}

/// Canonical JSON: recursively sorted object keys, no whitespace.
pub fn canonical_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap(),
                        canonical_json(&map[k])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Report from [`migrate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateReport {
    pub changed: bool,
    pub before: Option<String>,
    pub pages: usize,
}

impl std::fmt::Display for MigrateReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.changed {
            write!(
                f,
                "migrated to okf-0.2 (was {}) — {} pages, projections regenerated",
                self.before.as_deref().unwrap_or("legacy"),
                self.pages
            )
        } else {
            write!(f, "already okf-0.2 — {} pages", self.pages)
        }
    }
}

pub fn migrate(vault: &VaultPaths) -> Result<MigrateReport, String> {
    if !vault.config_file().exists() {
        return Err(format!(
            "space is not bootstrapped (missing {}) — run wiki_bootstrap first",
            vault.config_file().display()
        ));
    }
    let raw = fs::read_to_string(vault.config_file()).map_err(|e| e.to_string())?;
    let mut cfg: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&raw).map_err(|e| format!("config.json: {e}"))?;
    let before = cfg
        .get("knowledge_format")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let changed = before.as_deref() != Some("okf-0.2");

    // Only a real upgrade can clobber pages: once the vault is OKF those
    // paths already hold generated projections.
    if changed {
        let wiki = vault.wiki_pages();
        let mut clashes: Vec<String> = Vec::new();
        for (rel, id) in [("index.md", "index"), ("log.md", "log")] {
            if wiki.join(rel).exists() {
                clashes.push(id.to_string());
            }
        }
        for (_, dir) in super::pages::PAGE_TYPES.iter().copied() {
            if wiki.join(dir).join("index.md").exists() {
                clashes.push(format!("{dir}/index"));
            }
        }
        if !clashes.is_empty() {
            return Err(format!(
                "refusing to migrate: generated OKF projections would overwrite existing pages: {}",
                clashes.join(", ")
            ));
        }
        cfg.insert(
            "knowledge_format".to_string(),
            serde_json::Value::String("okf-0.2".to_string()),
        );
        // Temp + rename: boot-time migration can overlap between sessions,
        // and a torn config.json would strand the whole space.
        let tmp = vault.space_root.join("config.json.tmp");
        fs::write(
            &tmp,
            serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(&tmp, vault.config_file()).map_err(|e| e.to_string())?;
    }

    // Rebuild under the new mode: regenerates registry, backlinks, meta/index.md
    // and (OKF) wiki/index.md + per-dir indexes + wiki/log.md.
    let registry = super::registry::rebuild_metadata(vault)?;
    Ok(MigrateReport {
        changed,
        before,
        pages: registry.pages.len(),
    })
}

/// Upgrade every bootstrapped space under `vault_root` to OKF v0.2.
///
/// ponytail: one-time rollout bridge — delete `migrate_all` and its call in
/// `main.rs::boot` once every deployed vault carries `knowledge_format`
/// (harvest with the ponytail-debt skill). Naturally idempotent: OKF spaces
/// are skipped, so after the first boot this is just a directory scan.
/// Spaces that refuse (hand-written projection paths) are reported, not forced.
pub fn migrate_all(vault_root: &std::path::Path) -> Vec<(String, Result<MigrateReport, String>)> {
    let Ok(entries) = fs::read_dir(vault_root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for space in entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("config.json").is_file())
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(str::to_owned))
    {
        let vault = VaultPaths::new(vault_root, &space);
        match mode(&vault) {
            Ok(Mode::Okf) => {}
            Ok(Mode::Legacy) => out.push((space, migrate(&vault))),
            Err(e) => out.push((space, Err(e))),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::bootstrap::bootstrap;
    use crate::vault::registry::{rebuild_metadata, PageEntry};

    fn setup(mode_json: &str) -> (tempfile::TempDir, VaultPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        bootstrap(&v, "t").unwrap();
        {
            let cfg_path = v.config_file();
            let mut cfg: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
            if mode_json.is_empty() {
                cfg.as_object_mut().unwrap().remove("knowledge_format");
            } else {
                cfg["knowledge_format"] = serde_json::from_str(mode_json).unwrap();
            }
            fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
        }
        (tmp, v)
    }

    #[test]
    fn migrate_sets_format_and_regenerates_projections() {
        let (_tmp, v) = setup(""); // legacy: no knowledge_format key
        crate::vault::pages::ensure_page(
            &v,
            "concept",
            "Alpha",
            None,
            crate::vault::pages::GateMode::Off,
        )
        .unwrap();

        let report = migrate(&v).unwrap();
        assert!(report.changed);
        assert_eq!(report.before, None);
        // 1 = the concept page. Generated `wiki/index.md` is skipped by the
        // scanner (reserved), so it never enters the registry.
        assert_eq!(report.pages, 1);

        let cfg: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(v.config_file()).unwrap()).unwrap();
        assert_eq!(cfg["knowledge_format"], "okf-0.2");
        let idx = v.wiki_pages().join("index.md");
        assert!(idx.exists(), "okf root index generated");
        assert!(fs::read_to_string(&idx).unwrap().contains("okf_version"));
        assert!(v.wiki_pages().join("log.md").exists());

        let again = migrate(&v).unwrap();
        assert!(!again.changed, "migration is idempotent");
        assert_eq!(again.before.as_deref(), Some("okf-0.2"));
    }

    #[test]
    fn empty_okf_vault_keeps_the_root_index() {
        let (_tmp, v) = setup("\"okf-0.2\"");
        // No pages at all: the root index must survive projection pruning
        // (it carries the okf_version frontmatter), dir indexes need not.
        let reg = rebuild_metadata(&v).unwrap();
        assert!(reg.pages.is_empty());
        let idx = v.wiki_pages().join("index.md");
        assert!(idx.exists(), "root index survives on an empty vault");
        assert!(fs::read_to_string(&idx).unwrap().contains("okf_version"));
        assert!(!v.wiki_pages().join("concepts/index.md").exists());
    }

    #[test]
    fn migrate_all_upgrades_only_legacy_spaces() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let legacy = VaultPaths::new(root, "legacy");
        let modern = VaultPaths::new(root, "modern");
        bootstrap(&legacy, "t").unwrap();
        bootstrap(&modern, "t").unwrap();
        // legacy: strip the key; modern keeps the bootstrap default (okf-0.2)
        let cfg: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(legacy.config_file()).unwrap()).unwrap();
        let mut cfg = cfg;
        cfg.as_object_mut().unwrap().remove("knowledge_format");
        fs::write(
            legacy.config_file(),
            serde_json::to_string_pretty(&cfg).unwrap(),
        )
        .unwrap();
        // a real page, so the OKF projections have something to index
        let page = legacy.page_path("concepts/alpha");
        fs::create_dir_all(page.parent().unwrap()).unwrap();
        fs::write(&page, "---\ntitle: Alpha\n---\n\nBody.\n").unwrap();

        let results = migrate_all(root);
        assert_eq!(results.len(), 1, "only the legacy space is touched");
        assert_eq!(results[0].0, "legacy");
        let report = results[0].1.as_ref().unwrap();
        assert!(report.changed);
        assert!(legacy.wiki_pages().join("index.md").exists());

        // Second boot: nothing left to migrate.
        assert!(migrate_all(root).is_empty());
    }

    #[test]
    fn migrate_refuses_to_overwrite_a_real_page() {
        let (_tmp, v) = setup("");
        fs::create_dir_all(v.wiki_pages()).unwrap();
        fs::write(v.wiki_pages().join("index.md"), "# hand written\n").unwrap();

        let err = migrate(&v).unwrap_err();
        assert!(err.contains("refusing to migrate"), "{err}");
        let cfg: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(v.config_file()).unwrap()).unwrap();
        assert!(cfg.get("knowledge_format").is_none(), "config untouched");
        assert_eq!(
            fs::read_to_string(v.wiki_pages().join("index.md")).unwrap(),
            "# hand written\n"
        );
    }

    fn entry(id: &str, title: &str, description: &str, links: Vec<String>) -> PageEntry {
        PageEntry {
            id: id.into(),
            title: title.into(),
            page_type: crate::vault::pages::type_for_folder(id.split('/').next().unwrap()),
            path: format!("wiki/{id}.md"),
            links,
            excerpt: String::new(),
            description: description.into(),
            source_id: None,
        }
    }

    #[test]
    fn mode_resolution_fails_closed() {
        let (_t, v) = setup("\"okf-0.2\"");
        assert_eq!(mode(&v).unwrap(), Mode::Okf);
        let (_t2, v2) = setup("\"legacy\"");
        assert_eq!(mode(&v2).unwrap(), Mode::Legacy);
        let (_t3, v3) = setup("\"yaml-matter\"");
        assert!(mode(&v3)
            .unwrap_err()
            .contains("config_invalid_knowledge_format"));
        let (_t4, v4) = setup("");
        assert_eq!(mode(&v4).unwrap(), Mode::Legacy); // absent = legacy
    }

    #[test]
    fn dir_indexes_deterministic_with_dirs_and_descriptions() {
        let (_t, v) = setup("\"okf-0.2\"");
        let mut reg = Registry::default();
        reg.pages.insert(
            "concepts/rag".into(),
            entry(
                "concepts/rag",
                "RAG",
                "Grounds generation using retrieved evidence",
                vec![],
            ),
        );
        reg.pages
            .insert("concepts/b".into(), entry("concepts/b", "", "", vec![]));
        reg.pages.insert(
            "entities/acme".into(),
            entry("entities/acme", "Acme", "", vec![]),
        );
        reg.pages.insert(
            "concepts/deep/nested".into(),
            entry("concepts/deep/nested", "Nested", "", vec![]),
        );
        write_dir_indexes(&v, &reg, "My Vault").unwrap();

        let root = fs::read_to_string(v.wiki_pages().join("index.md")).unwrap();
        assert!(root.contains("okf_version: \"0.2\""));
        assert!(root.contains("# My Vault"));
        assert!(root.contains("## Directories"));
        // dirs sorted: concepts before entities
        assert!(root.find("concepts/").unwrap() < root.find("entities/").unwrap());
        assert!(!root.contains("## Concepts")); // no root-level concepts
        let concepts_idx = fs::read_to_string(v.wiki_pages().join("concepts/index.md")).unwrap();
        assert!(concepts_idx.contains("## Concepts"));
        assert!(
            concepts_idx.contains("[RAG](rag.md) — Grounds generation using retrieved evidence")
        );

        let concepts = fs::read_to_string(v.wiki_pages().join("concepts/index.md")).unwrap();
        assert!(concepts.starts_with("# concepts"));
        assert!(concepts.contains("[deep/](deep/)")); // direct subdir listed
        let deep = fs::read_to_string(v.wiki_pages().join("concepts/deep/index.md")).unwrap();
        assert!(deep.starts_with("# deep"));

        // determinism: same registry -> byte-identical output
        let before = fs::read_to_string(v.wiki_pages().join("index.md")).unwrap();
        write_dir_indexes(&v, &reg, "My Vault").unwrap();
        assert_eq!(
            before,
            fs::read_to_string(v.wiki_pages().join("index.md")).unwrap()
        );
    }

    #[test]
    fn stale_dir_index_pruned() {
        let (_t, v) = setup("\"okf-0.2\"");
        let mut reg = Registry::default();
        reg.pages
            .insert("concepts/a".into(), entry("concepts/a", "A", "", vec![]));
        write_dir_indexes(&v, &reg, "V").unwrap();
        // manually create an empty dir with an index -> pruned on next write
        let ghost = v.wiki_pages().join("ghost");
        fs::create_dir_all(&ghost).unwrap();
        fs::write(ghost.join("index.md"), "# ghost").unwrap();
        write_dir_indexes(&v, &reg, "V").unwrap();
        assert!(!ghost.join("index.md").exists());
    }

    #[test]
    fn okf_log_groups_by_date_newest_first_canonical_json() {
        let (_t, v) = setup("\"okf-0.2\"");
        super::super::registry::log_event(
            &v,
            "capture",
            &serde_json::json!({"source_id":"SRC-1","zeta":"last","alpha":"first"}),
            "2026-09-07T10:00:00Z",
        )
        .unwrap();
        super::super::registry::log_event(
            &v,
            "bootstrap",
            &serde_json::json!({}),
            "2026-09-06T09:00:00Z",
        )
        .unwrap();
        write_okf_log(&v).unwrap();
        let log = fs::read_to_string(v.wiki_pages().join("log.md")).unwrap();
        assert!(log.starts_with("# Wiki Update Log\n\n## 2026-09-07"));
        assert!(log.find("## 2026-09-07").unwrap() < log.find("## 2026-09-06").unwrap());
        // canonical json: keys sorted
        assert!(log.contains("{\"alpha\":\"first\",\"source_id\":\"SRC-1\",\"zeta\":\"last\"}"));
        // no details -> no colon
        assert!(log.contains("- **bootstrap**\n"));
        assert!(log.contains("- **capture**: {"));
    }

    #[test]
    fn malformed_events_omitted() {
        let (_t, v) = setup("\"okf-0.2\"");
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(v.events_file())
            .unwrap();
        writeln!(f, "{{not json").unwrap();
        drop(f);
        write_okf_log(&v).unwrap(); // must not fail
        let log = fs::read_to_string(v.wiki_pages().join("log.md")).unwrap();
        assert!(!log.contains("not json"));
    }

    #[test]
    fn rebuild_metadata_writes_okf_projections_in_okf_mode() {
        let (_t, v) = setup("\"okf-0.2\"");
        let page = v.page_path("concepts/rag");
        fs::create_dir_all(page.parent().unwrap()).unwrap();
        fs::write(&page, "---\ntitle: RAG\n---\n\nbody\n").unwrap();
        rebuild_metadata(&v).unwrap();
        assert!(v.wiki_pages().join("index.md").exists());
        assert!(v.wiki_pages().join("log.md").exists());
    }
}
