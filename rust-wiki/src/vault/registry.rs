//! Registry: the master page catalog + generated projections.
//!
//! `meta/registry.json` is rebuilt by scanning `wiki/**/*.md`. Frontmatter
//! carries `title`/`type` (defaults derived from folder/heading). Links:
//! standard markdown `[label](/folder/page.md)` and legacy `[[folder/page]]`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::layout::VaultPaths;

/// One registered page (folder-qualified id -> entry).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageEntry {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: String,
    /// Vault-relative path, e.g. `wiki/concepts/rag.md`
    pub path: String,
    /// Page ids this page links to (outbound).
    pub links: Vec<String>,
    /// First ~200 chars of body text (preview for recall/search).
    pub excerpt: String,
    /// OKF `description`: canonical one-sentence preview (empty when unknown).
    #[serde(default)]
    pub description: String,
    /// For `sources/` pages: ingest state (absent = not applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Registry {
    pub pages: BTreeMap<String, PageEntry>,
}

// ---------- markdown parsing ----------

/// Split `---\n...\n---` frontmatter from body. Returns (fm_yaml, body).
pub(crate) fn split_frontmatter(text: &str) -> (Option<&str>, &str) {
    let t = text.trim_start();
    if let Some(rest) = t.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let body_start = end + 4; // "\n---"
            let body = rest[body_start..]
                .strip_prefix('\n')
                .unwrap_or(&rest[body_start..]);
            return (Some(fm), body);
        }
    }
    (None, text)
}

/// Minimal frontmatter scalar extraction: `title:` and `type:` values.
fn fm_scalar(fm: &str, key: &str) -> Option<String> {
    for line in fm.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix(&format!("{key}:")) {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Extract outbound page ids: markdown links to /folder/page.md and [[wikilinks]].
pub fn extract_links(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    // [label](/folder/page.md) — link target starting with "/" and ending .md
    for caps in markdown_link_re().captures_iter(body) {
        push_id(&mut out, &caps[1]);
    }
    // [[folder/page]] (legacy, readable)
    for caps in wikilink_re().captures_iter(body) {
        push_id(&mut out, &caps[1]);
    }
    out
}

fn push_id(out: &mut Vec<String>, raw: &str) {
    let id = raw.trim().trim_start_matches('/').trim_end_matches(".md");
    if !id.is_empty() && !out.contains(&id.to_string()) {
        out.push(id.to_string());
    }
}

fn markdown_link_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\[[^\]]*\]\(/([^)\s]+\.md)\)").unwrap())
}

fn wikilink_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\[\[([^\]|]+)\]\]").unwrap())
}

/// Public accessor for the wikilink pattern (used by the write gate).
pub fn wikilinks() -> &'static regex::Regex {
    wikilink_re()
}

fn first_heading_or(body: &str, fallback: &str) -> String {
    for line in body.lines() {
        if let Some(h) = line.trim().strip_prefix("# ") {
            let h = h.trim();
            if !h.is_empty() {
                return h.to_string();
            }
        }
    }
    fallback.to_string()
}

fn excerpt_of(body: &str, max: usize) -> String {
    let (_, body) = split_frontmatter(body);
    let text: String = body
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let squeezed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squeezed.chars().count() <= max {
        squeezed
    } else {
        let cut: String = squeezed.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// Derive page id (e.g. `concepts/rag`) from a file under `wiki/`.
fn page_id_of(vault: &VaultPaths, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(vault.wiki_pages()).ok()?;
    let rel = rel.to_string_lossy().replace('\\', "/");
    let id = rel.strip_suffix(".md")?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

// ---------- scan + projections ----------

/// Scan `wiki/**/*.md` and rebuild registry.json + backlinks.json + index.md.
pub fn rebuild_metadata(vault: &VaultPaths) -> Result<Registry, String> {
    let mut registry = Registry::default();
    let wiki_dir = vault.wiki_pages();
    collect_pages(vault, &wiki_dir, &mut registry)?;
    // sort links for deterministic output
    for p in registry.pages.values_mut() {
        p.links.sort();
    }

    let inbound = inbound_links(&registry);
    write_json(&vault.registry_file(), &registry)?;
    write_json(&vault.backlinks_file(), &inbound)?;
    write_index(vault, &registry)?;
    // OKF mode: deterministic wiki/ projections (reserved generated files)
    if super::okf::mode(vault)? == super::okf::Mode::Okf {
        let name = vault
            .space_root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Wiki".into());
        super::okf::write_dir_indexes(vault, &registry, &name)?;
        super::okf::write_okf_log(vault)?;
    }
    Ok(registry)
}

/// Inbound-link map (page id -> citing ids), the single source of truth
/// for backlinks.json, status orphans, and lint. Sorted for determinism.
pub fn inbound_links(registry: &Registry) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut inbound: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for page in registry.pages.values() {
        for target in &page.links {
            let v = inbound.entry(target.clone()).or_default();
            if !v.contains(&page.id) {
                v.push(page.id.clone());
            }
        }
    }
    for v in inbound.values_mut() {
        v.sort();
    }
    inbound
}

fn collect_pages(vault: &VaultPaths, dir: &Path, registry: &mut Registry) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_pages(vault, &path, registry)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let Some(id) = page_id_of(vault, &path) else {
                continue;
            };
            let text =
                fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            let (fm, body) = split_frontmatter(&text);
            let folder = id.split('/').next().unwrap_or("pages");
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("page")
                .to_string();
            let title = fm
                .and_then(|f| fm_scalar(f, "title"))
                .unwrap_or_else(|| first_heading_or(body, &file_stem));
            let page_type = fm
                .and_then(|f| fm_scalar(f, "type"))
                .unwrap_or_else(|| folder.trim_end_matches('s').to_string());
            let description = fm
                .and_then(|f| fm_scalar(f, "description"))
                .unwrap_or_default();
            let source_id = if folder == "sources" {
                Some(file_stem.clone())
            } else {
                None
            };
            registry.pages.insert(
                id.clone(),
                PageEntry {
                    id,
                    title,
                    page_type,
                    path: path
                        .strip_prefix(&vault.space_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    links: extract_links(body),
                    excerpt: excerpt_of(body, 200),
                    description,
                    source_id,
                },
            );
        }
    }
    Ok(())
}

fn write_index(vault: &VaultPaths, registry: &Registry) -> Result<(), String> {
    let mut by_type: BTreeMap<&str, Vec<&PageEntry>> = BTreeMap::new();
    for p in registry.pages.values() {
        by_type.entry(p.page_type.as_str()).or_default().push(p);
    }
    let mut out = String::from("# Index\n\n");
    for (ptype, pages) in by_type {
        out.push_str(&format!("## {ptype}\n\n"));
        for p in pages {
            out.push_str(&format!("- [{}](/{}) — {}\n", p.title, p.id, p.id));
        }
        out.push('\n');
    }
    fs::write(vault.index_file(), out).map_err(|e| format!("write index: {e}"))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value).map_err(|e| format!("serialize: {e}"))?;
    fs::write(path, json + "\n").map_err(|e| format!("write {}: {e}", path.display()))
}

// ---------- events ----------

/// Append a structured event to the authoritative events.jsonl stream.
pub fn log_event(
    vault: &VaultPaths,
    kind: &str,
    details: &serde_json::Value,
    now_iso: &str,
) -> Result<(), String> {
    use std::io::Write;
    let event = serde_json::json!({ "timestamp": now_iso, "kind": kind, "details": details });
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(vault.events_file())
        .map_err(|e| format!("open events: {e}"))?;
    writeln!(f, "{event}").map_err(|e| format!("append event: {e}"))
}

/// Rebuild log.md from events.jsonl (one-way projection).
pub fn rebuild_log(vault: &VaultPaths) -> Result<(), String> {
    let mut out = String::from("# Log\n\n");
    if vault.events_file().exists() {
        let raw =
            fs::read_to_string(vault.events_file()).map_err(|e| format!("read events: {e}"))?;
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: serde_json::Value =
                serde_json::from_str(line).map_err(|e| format!("parse event: {e}"))?;
            let ts = v["timestamp"].as_str().unwrap_or("?");
            let kind = v["kind"].as_str().unwrap_or("?");
            out.push_str(&format!("- `{ts}` {kind} {}\n", v["details"]));
        }
    }
    fs::write(vault.log_file(), out).map_err(|e| format!("write log: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_vault() -> (tempfile::TempDir, VaultPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        super::super::bootstrap::bootstrap(&v, "2026-09-06T00:00:00Z").unwrap();
        (tmp, v)
    }

    fn write_page(v: &VaultPaths, id: &str, body: &str) {
        let p = v.page_path(id);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn registry_links_and_projections() {
        let (_tmp, v) = setup_vault();
        write_page(
            &v,
            "concepts/rag",
            "---\ntitle: RAG\ntype: concept\n---\n\nRetrieval augmented generation. See [[entities/acme]] and [guide](/analyses/how-rag.md).\n\nBody text for the excerpt.",
        );
        write_page(
            &v,
            "entities/acme",
            "# Acme\n\nVendor. Referenced by [[concepts/rag]].\n",
        );
        let reg = rebuild_metadata(&v).unwrap();
        assert_eq!(reg.pages.len(), 2);
        let rag = &reg.pages["concepts/rag"];
        assert_eq!(rag.title, "RAG");
        assert_eq!(rag.page_type, "concept");
        assert_eq!(rag.links, vec!["analyses/how-rag", "entities/acme"]);
        assert!(rag.excerpt.contains("Retrieval augmented"));
        assert_eq!(rag.source_id, None);
        assert_eq!(reg.pages["entities/acme"].source_id, None);

        let backlinks: BTreeMap<String, Vec<String>> =
            serde_json::from_str(&fs::read_to_string(v.backlinks_file()).unwrap()).unwrap();
        assert_eq!(backlinks["entities/acme"], vec!["concepts/rag"]);
        // missing page still recorded as a backlink target
        assert_eq!(backlinks["analyses/how-rag"], vec!["concepts/rag"]);

        let index = fs::read_to_string(v.index_file()).unwrap();
        assert!(index.contains("## concept"));
        assert!(index.contains("[RAG](/concepts/rag)"));

        // source pages carry source_id
        write_page(&v, "sources/SRC-2026-09-06-001", "# A source\n\nclaims\n");
        let reg2 = rebuild_metadata(&v).unwrap();
        assert_eq!(
            reg2.pages["sources/SRC-2026-09-06-001"]
                .source_id
                .as_deref(),
            Some("SRC-2026-09-06-001")
        );
    }

    #[test]
    fn events_append_and_log_projection() {
        let (_tmp, v) = setup_vault();
        log_event(
            &v,
            "ingest",
            &serde_json::json!({"source":"SRC-1"}),
            "2026-09-06T01:00:00Z",
        )
        .unwrap();
        log_event(
            &v,
            "retro",
            &serde_json::json!({"slug":"jwt-fix"}),
            "2026-09-06T02:00:00Z",
        )
        .unwrap();
        rebuild_log(&v).unwrap();
        let log = fs::read_to_string(v.log_file()).unwrap();
        assert!(log.contains("`2026-09-06T01:00:00Z` ingest"));
        assert!(log.contains("jwt-fix"));
    }

    #[test]
    fn title_falls_back_to_heading_then_stem() {
        let (_tmp, v) = setup_vault();
        write_page(&v, "concepts/no-title", "# Heading Title\n\nbody\n");
        write_page(&v, "concepts/bare", "just text\n");
        let reg = rebuild_metadata(&v).unwrap();
        assert_eq!(reg.pages["concepts/no-title"].title, "Heading Title");
        assert_eq!(reg.pages["concepts/bare"].title, "bare");
        assert_eq!(reg.pages["concepts/bare"].page_type, "concept");
    }
}
