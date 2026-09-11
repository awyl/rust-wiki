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
    /// Fail-closed scan diagnostics (bad frontmatter, identity collisions,
    /// link escapes). Skipped pages never enter `pages` — no partial reads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<ScanDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScanDiagnostic {
    pub path: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub line: usize,
    pub message: String,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

// ---------- markdown parsing ----------

/// Body text after the frontmatter fence (for excerpt/heading extraction).
pub(crate) fn body_of(text: &str) -> &str {
    fm_body(text)
}

/// Body text after the frontmatter fence (lenient: for excerpt/heading
/// extraction on pages that already passed hardened parsing).
fn fm_body(text: &str) -> &str {
    match super::frontmatter::split_block(text) {
        Ok((_, body)) => body,
        Err(_) => text,
    }
}

/// Extract outbound page ids. Markdown links are parsed with a CommonMark
/// engine (pulldown-cmark): code spans/blocks, images, autolinks and raw
/// HTML never produce backlinks; reference links do. Wikilinks are kept
/// for compatibility. Returns (ids, escape_diagnostics).
pub fn extract_links(
    body: &str,
    source_id: &str,
    escapes: &mut Vec<ScanDiagnostic>,
) -> Vec<String> {
    use pulldown_cmark::{Event, Parser, Tag};
    let mut out: Vec<String> = Vec::new();
    let opts = pulldown_cmark::Options::ENABLE_FOOTNOTES;
    for event in Parser::new_ext(body, opts) {
        if let Event::Start(Tag::Link { dest_url, .. }) = event {
            match resolve_link(&dest_url, source_id) {
                Ok(Some(id)) => push_id(&mut out, &id),
                Ok(None) => {} // external scheme
                Err(msg) => escapes.push(ScanDiagnostic {
                    path: source_id.to_string(),
                    code: "link_path_escape".into(),
                    line: 0,
                    message: msg,
                }),
            }
        }
    }
    // [[folder/page]] and [[folder/page|label]] (legacy, readable)
    for caps in wikilink_re().captures_iter(body) {
        let target = caps[1].split('|').next().unwrap_or(&caps[1]);
        push_id(&mut out, target);
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 1 && i + 2 < bytes.len() + 1 {
            let hex = bytes.get(i + 1..i + 3);
            if let Some(h) = hex {
                if let Ok(v) = u8::from_str_radix(std::str::from_utf8(h).unwrap_or(""), 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Normalize a markdown link destination to a page id.
/// Ok(None) = external/uninteresting; Err = escapes the wiki root.
fn resolve_link(dest: &str, source_id: &str) -> Result<Option<String>, String> {
    // strip query + fragment
    let mut target = dest.split(['#', '?']).next().unwrap_or("").to_string();
    if target.is_empty() {
        return Ok(None);
    }
    if ["http://", "https://", "mailto:", "ftp://"]
        .iter()
        .any(|p| target.starts_with(p))
    {
        return Ok(None);
    }
    target = percent_decode(&target);
    let base = source_id.rsplit_once('/').map(|(d, _)| d.to_string());
    // resolve dot segments
    let joined = if target.starts_with('/') {
        target.trim_start_matches('/').to_string()
    } else {
        match &base {
            Some(dir) => format!("{dir}/{target}"),
            None => target.clone(),
        }
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("link '{dest}' escapes the wiki root"));
                }
            }
            other => parts.push(other),
        }
    }
    let id = parts.join("/");
    let id = id.trim_end_matches(".md");
    if id.is_empty() {
        return Ok(None);
    }
    Ok(Some(id.to_string()))
}

fn push_id(out: &mut Vec<String>, raw: &str) {
    let id = raw.trim().trim_start_matches('/').trim_end_matches(".md");
    if !id.is_empty() && !out.contains(&id.to_string()) {
        out.push(id.to_string());
    }
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

/// Point links at a page whose folder was guessed wrong.
///
/// Writers occasionally infer a folder from a page type — `/retros/<slug>.md`
/// for an insight that `pages::retro` actually writes to `sources/` — which
/// lint then reports as a missing page. When such a link's basename matches
/// exactly one page, retarget it; when the basename is ambiguous (the same
/// slug in two folders) leave it dangling, so lint reports it instead of us
/// silently choosing a target.
fn resolve_guessed_folders(registry: &mut Registry) {
    let mut by_slug: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for id in registry.pages.keys() {
        if let Some((_, slug)) = id.rsplit_once('/') {
            by_slug
                .entry(slug.to_string())
                .or_default()
                .push(id.clone());
        }
    }
    // Plan against an immutable borrow, then apply: the target is a page id,
    // and the citing page is the one being mutated.
    let mut remap: Vec<(String, String, String)> = Vec::new();
    for (id, page) in &registry.pages {
        for link in &page.links {
            if registry.pages.contains_key(link) {
                continue;
            }
            let Some((_, slug)) = link.rsplit_once('/') else {
                continue;
            };
            match by_slug.get(slug).map(|v| v.as_slice()) {
                Some([only]) if only != link => {
                    remap.push((id.clone(), link.clone(), only.clone()))
                }
                _ => {}
            }
        }
    }
    for (id, from, to) in remap {
        if let Some(page) = registry.pages.get_mut(&id) {
            for link in page.links.iter_mut() {
                if *link == from {
                    *link = to.clone();
                }
            }
            page.links.sort();
            page.links.dedup();
        }
    }
}

/// Scan `wiki/**/*.md` and rebuild registry.json + backlinks.json + index.md.
pub fn rebuild_metadata(vault: &VaultPaths) -> Result<Registry, String> {
    let mut registry = Registry::default();
    let wiki_dir = vault.wiki_pages();
    collect_pages(vault, &wiki_dir, &mut registry)?;
    resolve_guessed_folders(&mut registry);
    registry.diagnostics.sort();
    registry.diagnostics.dedup();
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
    use unicode_normalization::UnicodeNormalization;
    // identity collision detection: NFC + case-fold keys must be unique
    let mut seen_ids: std::collections::BTreeMap<String, String> = Default::default();
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
            // reserved generated files are never concept pages
            if id == "index" || id == "log" || id.ends_with("/index") {
                continue;
            }
            let text =
                fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            // identity: NFC-normalize, then fail closed on collisions
            let id: String = id.nfc().collect();
            let id_key = id.to_lowercase();
            if let Some(prev) = seen_ids.get(&id_key) {
                registry.diagnostics.push(ScanDiagnostic {
                    path: id.clone(),
                    code: "concept_identity_collision".into(),
                    line: 0,
                    message: format!("collides with '{prev}' after NFC/case folding"),
                });
                continue;
            }
            seen_ids.insert(id_key, id.clone());
            // fail-closed frontmatter: any diagnostic excludes the page
            let fm = match super::frontmatter::parse(&text) {
                Ok(fm) => fm,
                Err(diags) => {
                    for d in diags {
                        registry.diagnostics.push(ScanDiagnostic {
                            path: id.clone(),
                            code: d.code,
                            line: d.line,
                            message: d.message,
                        });
                    }
                    continue;
                }
            };
            let body = fm_body(&text);
            let folder = id.split('/').next().unwrap_or("pages");
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("page")
                .to_string();
            let title = fm
                .scalar("title")
                .unwrap_or_else(|| first_heading_or(body, &file_stem));
            let page_type = fm
                .scalar("type")
                .unwrap_or_else(|| super::pages::type_for_folder(folder));
            let description = fm.scalar("description").unwrap_or_default();
            let source_id = if folder == "sources" {
                Some(file_stem.clone())
            } else {
                None
            };
            let mut escapes: Vec<ScanDiagnostic> = Vec::new();
            let links = extract_links(body, &id, &mut escapes);
            registry.diagnostics.extend(escapes);
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
                    links,
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
    fn a_link_into_a_guessed_folder_finds_the_page() {
        let (_tmp, v) = setup_vault();
        // An insight written by `pages::retro` lives in sources/, but the
        // citing page guessed `retros/` from the type.
        write_page(
            &v,
            "sources/chunk-level-embeddings",
            "---\ntype: retro\ntitle: chunks\n---\n\nInsight.\n",
        );
        write_page(
            &v,
            "concepts/embeddings",
            "---\ntype: concept\ntitle: embeddings\n---\n\nSee [chunks](/retros/chunk-level-embeddings.md).\n",
        );
        let reg = rebuild_metadata(&v).unwrap();
        assert_eq!(
            reg.pages["concepts/embeddings"].links,
            vec!["sources/chunk-level-embeddings"]
        );
        let backlinks: BTreeMap<String, Vec<String>> =
            serde_json::from_str(&fs::read_to_string(v.backlinks_file()).unwrap()).unwrap();
        assert_eq!(
            backlinks["sources/chunk-level-embeddings"],
            vec!["concepts/embeddings"]
        );
        // The guessed id is gone, so lint no longer reports a missing page.
        assert!(!backlinks.contains_key("retros/chunk-level-embeddings"));
    }

    #[test]
    fn an_ambiguous_basename_is_left_dangling() {
        let (_tmp, v) = setup_vault();
        // Same slug in two folders: picking one would be a guess, so the link
        // stays broken and lint keeps reporting it.
        write_page(&v, "sources/dupe", "---\ntype: retro\ntitle: a\n---\n\na\n");
        write_page(
            &v,
            "concepts/dupe",
            "---\ntype: concept\ntitle: b\n---\n\nb\n",
        );
        write_page(
            &v,
            "concepts/citing",
            "---\ntype: concept\ntitle: c\n---\n\nSee [x](/retros/dupe.md).\n",
        );
        let reg = rebuild_metadata(&v).unwrap();
        assert_eq!(reg.pages["concepts/citing"].links, vec!["retros/dupe"]);
    }

    #[test]
    fn type_defaults_follow_the_directory_map() {
        let (_tmp, v) = setup_vault();
        let cases = [
            ("analyses/alpha", "analysis"),
            ("entities/beta", "entity"),
            ("syntheses/gamma", "synthesis"),
            ("concepts/delta", "concept"),
            ("requirements/epsilon", "requirement"),
        ];
        for (id, _) in cases {
            // No `type:` frontmatter — the directory decides.
            write_page(&v, id, "# Page\n\nBody.\n");
        }
        let reg = rebuild_metadata(&v).unwrap();
        for (id, want) in cases {
            assert_eq!(reg.pages[id].page_type, want, "type for {id}");
        }
    }

    #[test]
    fn type_for_folder_handles_irregular_plurals() {
        use super::super::pages::type_for_folder;
        assert_eq!(type_for_folder("analyses"), "analysis");
        assert_eq!(type_for_folder("entities"), "entity");
        assert_eq!(type_for_folder("syntheses"), "synthesis");
        // unknown dirs keep the old naive fallback
        assert_eq!(type_for_folder("customs"), "custom");
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
