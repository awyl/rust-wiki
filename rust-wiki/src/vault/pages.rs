//! Page writes: ensure/read/write, retro, observe, wikilink gate.
//! Only `wiki/**` is writable (guardrail enforced structurally here).

use std::fs;

use super::layout::{ownership, Ownership, VaultPaths};
use super::lint::slugify;
use super::registry::{rebuild_metadata, Registry};

pub const PAGE_TYPES: &[(&str, &str)] = &[
    ("entity", "entities"),
    ("concept", "concepts"),
    ("synthesis", "syntheses"),
    ("analysis", "analyses"),
    ("requirement", "requirements"),
    ("skill", "skills"),
    ("case", "cases"),
    // `sources/` is shared by two types: `source` pages are captured material
    // and `retro` pages are session knowledge — post-task insights written by
    // `pages::retro` and mid-session notes written by `pages::observe`, which
    // stores a retro too (one name per artifact). Both live there, so a folder
    // must never be guessed from a page type — link to the id a tool returned.
    ("source", "sources"),
    ("retro", "sources"),
];

/// Every creatable page type, for error messages that cannot go stale.
pub fn known_types() -> String {
    PAGE_TYPES
        .iter()
        .map(|(t, _)| *t)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Relevance a page may declare (`relevance:` frontmatter). Recall scales a
/// page's score by it (`recall::relevance_multiplier`), so the vocabulary is
/// validated here on write and interpreted there on read.
pub fn is_relevance(r: &str) -> bool {
    matches!(r, "low" | "medium" | "high" | "critical")
}

pub fn folder_for(page_type: &str) -> Option<&'static str> {
    PAGE_TYPES
        .iter()
        .find(|(t, _)| *t == page_type)
        .map(|(_, f)| *f)
}

/// Canonical page type for a wiki directory. Known dirs come from
/// `PAGE_TYPES` (`analyses` -> analysis, not `analyse`); unknown dirs fall
/// back to naive singularization so custom folders still scan.
pub fn type_for_folder(folder: &str) -> String {
    PAGE_TYPES
        .iter()
        .find(|(_, f)| *f == folder)
        .map(|(t, _)| (*t).to_string())
        .unwrap_or_else(|| folder.trim_end_matches('s').to_string())
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 96
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
    Off,
    Validate,
    Normalize,
}

impl GateMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "validate" => Some(Self::Validate),
            "normalize" => Some(Self::Normalize),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Validate => "validate",
            Self::Normalize => "normalize",
        }
    }
}

/// Pre-write wikilink gate over a body. `existing` = registry page ids.
/// Returns the (possibly normalized) body or a diagnostic message.
///
/// Fenced blocks and inline code spans are skipped: a page that documents
/// wikilink syntax (`[[folder/page]]` in a code sample) must not have that
/// sample rewritten into a real link to a page that never existed.
pub fn apply_gate(body: &str, existing: &Registry, mode: GateMode) -> Result<String, String> {
    if mode == GateMode::Off {
        return Ok(body.to_string());
    }
    let re = super::registry::wikilinks();
    let mut out = String::with_capacity(body.len());
    let mut fenced = false;
    for line in body.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            out.push_str(line);
        } else if fenced {
            out.push_str(line);
        } else {
            for (segment, is_code) in code_segments(line) {
                if is_code {
                    out.push_str(segment);
                } else {
                    out.push_str(&gate_segment(segment, re, existing, mode)?);
                }
            }
        }
    }
    Ok(out)
}

/// Split one line into alternating prose / inline-code segments, keeping the
/// backticks in the returned slices so the line round-trips unchanged. An
/// unbalanced trailing backtick leaves the rest of the line as code.
fn code_segments(line: &str) -> Vec<(&str, bool)> {
    let mut segments = Vec::new();
    let mut is_code = false;
    let mut start = 0;
    for (idx, ch) in line.char_indices() {
        if ch != '`' {
            continue;
        }
        let end = if is_code { idx + 1 } else { idx };
        if end > start {
            segments.push((&line[start..end], is_code));
        }
        start = end;
        is_code = !is_code;
    }
    if start < line.len() {
        segments.push((&line[start..], is_code));
    }
    segments
}

fn gate_segment(
    segment: &str,
    re: &regex::Regex,
    existing: &Registry,
    mode: GateMode,
) -> Result<String, String> {
    let mut out = segment.to_string();
    for caps in re.captures_iter(segment) {
        let raw = &caps[1];
        let id = raw.trim();
        if id.is_empty() {
            return Err("empty wikilink [[]]".to_string());
        }
        if mode == GateMode::Validate && !existing.pages.contains_key(id) {
            return Err(format!(
                "wikilink [[{id}]] does not resolve to an existing page"
            ));
        }
        if mode == GateMode::Normalize {
            let canonical = format!("[{id}](/{id}.md)");
            out = out.replace(&format!("[[{raw}]]"), &canonical);
        }
    }
    Ok(out)
}

fn template_body(vault: &VaultPaths, page_type: &str, title: &str) -> String {
    let path = vault.templates().join(format!("{page_type}.md"));
    let raw = fs::read_to_string(path).unwrap_or_default();
    if raw.is_empty() {
        format!("# {title}\n\n")
    } else {
        fill_template(&raw, title)
    }
}

/// Substitute `{title}` and `{date}` (today UTC) in a template body.
fn fill_template(raw: &str, title: &str) -> String {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    raw.replace("{title}", title).replace("{date}", &date)
}

/// Authoritative page-template read: returns `templates/{page_type}.md`
/// with `{date}` filled and `{title}` left as a placeholder for the caller.
/// Lets agents always scaffold from the server's current templates.
pub fn template(vault: &VaultPaths, page_type: &str) -> Result<String, String> {
    if folder_for(page_type).is_none() {
        return Err(format!(
            "unknown page type '{page_type}' — expected one of: {}",
            known_types()
        ));
    }
    let path = vault.templates().join(format!("{page_type}.md"));
    let raw = fs::read_to_string(&path)
        .map_err(|_| format!("no template for page type '{page_type}' — re-bootstrap the space"))?;
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(raw.replace("{date}", &date))
}

/// A page to create. `relevance` is the optional `relevance:` declaration
/// recall scales a page's score by; the plain `ensure_page` leaves it unset.
pub struct PageSpec<'a> {
    pub page_type: &'a str,
    pub title: &'a str,
    pub content: Option<&'a str>,
    pub relevance: Option<&'a str>,
}

/// Create `folder/slug.md` if absent. Returns (id, created).
pub fn ensure_page(
    vault: &VaultPaths,
    page_type: &str,
    title: &str,
    content: Option<&str>,
    gate: GateMode,
) -> Result<(String, bool), String> {
    ensure_page_with(
        vault,
        PageSpec {
            page_type,
            title,
            content,
            relevance: None,
        },
        gate,
    )
}

/// `ensure_page` for callers that declare a `relevance`
/// (`low|medium|high|critical`) for the page they are creating.
pub fn ensure_page_with(
    vault: &VaultPaths,
    spec: PageSpec<'_>,
    gate: GateMode,
) -> Result<(String, bool), String> {
    let PageSpec {
        page_type,
        title,
        content,
        relevance,
    } = spec;
    if let Some(r) = relevance {
        if !is_relevance(r) {
            return Err(format!(
                "invalid relevance '{r}' — expected one of: low, medium, high, critical"
            ));
        }
    }
    let Some(folder) = folder_for(page_type) else {
        return Err(format!(
            "unknown page type '{page_type}' — expected one of: {}",
            known_types()
        ));
    };
    let slug = slugify(title);
    if !valid_slug(&slug) {
        return Err(format!(
            "title '{title}' does not slugify to a valid kebab-case id"
        ));
    }
    let id = format!("{folder}/{slug}");
    let path = vault.page_path(&id);
    if path.exists() {
        return Ok((id, false));
    }
    // A body that already carries a fence keeps it verbatim when it came from
    // the caller (never nest a second one, and refuse a `relevance` we cannot
    // merge into someone else's fence). A template body is ours, so a declared
    // claim goes in as the first field there too.
    let (from_template, body) = match content {
        Some(c) => (false, apply_gate(c, &read_registry(vault)?, gate)?),
        None => (true, template_body(vault, page_type, title)),
    };
    let fenced = body.trim_start().starts_with("---");
    let doc =
        match (relevance, fenced) {
            (Some(_), true) if !from_template => return Err(
                "content carries its own frontmatter — declare `relevance:` in that fence instead"
                    .into(),
            ),
            (Some(r), true) => {
                let t = body.trim_start();
                let rest = t.split_once('\n').map(|(_, rest)| rest).unwrap_or_default();
                format!("---\nrelevance: {r}\n{rest}")
            }
            (Some(r), false) => {
                format!("---\ntitle: \"{title}\"\ntype: {page_type}\nrelevance: {r}\n---\n\n{body}")
            }
            (None, true) => body.trim_start().to_string(),
            (None, false) => format!("---\ntitle: \"{title}\"\ntype: {page_type}\n---\n\n{body}"),
        };
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&path, doc).map_err(|e| e.to_string())?;
    rebuild_metadata(vault)?;
    Ok((id, true))
}

/// Guarded read of a wiki page by id.
pub fn read_page(vault: &VaultPaths, id: &str) -> Result<String, String> {
    if id.contains("..") || id.starts_with('/') {
        return Err(format!("invalid page id '{id}'"));
    }
    let path = vault.page_path(id);
    if ownership(vault, &path) != Ownership::Wiki {
        return Err(format!("'{id}' is not a wiki page"));
    }
    fs::read_to_string(&path).map_err(|_| format!("page '{id}' not found"))
}

/// Guarded update of an EXISTING wiki page. Never creates.
pub fn write_page(
    vault: &VaultPaths,
    id: &str,
    content: &str,
    gate: GateMode,
) -> Result<(), String> {
    let path = vault.page_path(id);
    if ownership(vault, &path) != Ownership::Wiki {
        return Err(format!("'{id}' is not a writable wiki page"));
    }
    if !path.exists() {
        return Err(format!(
            "page '{id}' does not exist — use wiki_ensure_page to create"
        ));
    }
    // Fail closed on fenceless writes: bare bodies bypass frontmatter and
    // silently drop out of the registry. Read the page first, keep its fence.
    if !content.trim_start().starts_with("---") {
        return Err(format!(
            "refusing fenceless write to '{id}' — read the page with wiki_read_page, keep its frontmatter fence, and write the complete file"
        ));
    }
    let gated = apply_gate(content, &read_registry(vault)?, gate)?;
    // preserve frontmatter: caller passes full doc; we only guardrail-check type unchanged
    fs::write(&path, gated).map_err(|e| e.to_string())?;
    rebuild_metadata(vault)?;
    Ok(())
}

fn read_registry(vault: &VaultPaths) -> Result<Registry, String> {
    let raw = fs::read_to_string(vault.registry_file()).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// Remove a wiki page and rebuild the metadata so the registry, index and
/// backlinks stop advertising it.
///
/// Refuses while any other page still links to the id (`force` overrides) —
/// deleting a linked page turns a tidy vault into a lint report. Deletion is
/// also the only irreversible operation in the tool set, so the caller must
/// name the page again in `confirm` and the operator must opt in via
/// `Config::allow_delete` (checked in `hub::delete_page`).
pub fn delete_page(vault: &VaultPaths, id: &str, force: bool) -> Result<String, String> {
    if id.contains("..") || id.starts_with('/') {
        return Err(format!("invalid page id '{id}'"));
    }
    let path = vault.page_path(id);
    if ownership(vault, &path) != Ownership::Wiki {
        return Err(format!("'{id}' is not a wiki page"));
    }
    if !path.exists() {
        return Err(format!("page '{id}' does not exist"));
    }
    if !force {
        let registry = read_registry(vault)?;
        let inbound = super::registry::inbound_links(&registry);
        if let Some(citers) = inbound.get(id).filter(|c| !c.is_empty()) {
            return Err(format!(
                "refusing to delete '{id}' — still linked from: {}. Re-point those links first, or pass force: true.",
                citers.join(", ")
            ));
        }
    }
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    // Only succeeds when the folder is empty; a leftover empty dir is harmless.
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent);
    }
    rebuild_metadata(vault)?;
    Ok(format!("deleted '{id}'"))
}

/// Atomic insight file: wiki/sources/<slug>.md, searchable immediately.
pub fn retro(
    vault: &VaultPaths,
    slug: &str,
    title: &str,
    body: &str,
    category: Option<&str>,
    relevance: Option<&str>,
    gate: GateMode,
) -> Result<String, String> {
    if !valid_slug(slug) {
        return Err(format!("invalid slug '{slug}' — use kebab-case"));
    }
    let id = format!("sources/{slug}");
    let path = vault.page_path(&id);
    if path.exists() {
        return Err(format!("insight '{slug}' already exists"));
    }
    let gated = apply_gate(body, &read_registry(vault)?, gate)?;
    let cat = match category {
        Some(c) => format!("category: {c}\n"),
        None => String::new(),
    };
    let rel = match relevance {
        Some(r) if is_relevance(r) => format!("relevance: {r}\n"),
        Some(r) => {
            return Err(format!(
                "invalid relevance '{r}' — low|medium|high|critical"
            ))
        }
        None => String::new(),
    };
    let doc = format!("---\ntitle: \"{title}\"\ntype: retro\n{cat}{rel}---\n\n{gated}");
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&path, doc).map_err(|e| e.to_string())?;
    rebuild_metadata(vault)?;
    Ok(id)
}

/// Timestamped observation input (keeps the fn under the arg limit).
#[derive(Debug, Clone)]
pub struct ObserveInput<'a> {
    pub title: &'a str,
    pub content: &'a str,
    pub relevance: &'a str,
    pub tags: Option<&'a str>,
    pub source_context: Option<&'a str>,
}

/// Timestamped mid-session note: wiki/sources/obs-<date>-<slug>.md, stored as
/// a `retro` page (there is no separate `observation` type) rated by relevance.
pub fn observe(
    vault: &VaultPaths,
    date: &str,
    input: &ObserveInput<'_>,
    gate: GateMode,
) -> Result<String, String> {
    let ObserveInput {
        title,
        content,
        relevance,
        tags,
        source_context,
    } = *input;
    if !is_relevance(relevance) {
        return Err(format!(
            "invalid relevance '{relevance}' — low|medium|high|critical"
        ));
    }
    let slug = format!("obs-{date}-{}", slugify(title));
    let body_slug = slugify(title);
    if !valid_slug(&body_slug) {
        return Err(format!("title '{title}' does not slugify"));
    }
    let id = format!("sources/{slug}");
    let path = vault.page_path(&id);
    if path.exists() {
        return Err(format!("observation '{slug}' already exists"));
    }
    let gated = apply_gate(content, &read_registry(vault)?, gate)?;
    let mut fm = format!("---\ntitle: \"{title}\"\ntype: retro\nrelevance: {relevance}\n");
    if let Some(t) = tags {
        fm.push_str(&format!("tags: {t}\n"));
    }
    if let Some(sc) = source_context {
        fm.push_str(&format!("source_context: \"{sc}\"\n"));
    }
    fm.push_str("---\n\n");
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&path, format!("{fm}{gated}")).map_err(|e| e.to_string())?;
    rebuild_metadata(vault)?;
    Ok(id)
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

    #[test]
    fn delete_refuses_while_other_pages_link_to_it() {
        let (_t, v) = setup();
        let (target, _) = ensure_page(&v, "concept", "Target", None, GateMode::Off).unwrap();
        ensure_page(
            &v,
            "concept",
            "Citer",
            Some(&format!("see [{target}](/{target}.md)\n")),
            GateMode::Off,
        )
        .unwrap();

        let err = delete_page(&v, &target, false).unwrap_err();
        assert!(err.contains("still linked from"), "{err}");
        assert!(err.contains("concepts/citer"), "{err}");

        // force is the documented override — and it really removes it.
        assert!(delete_page(&v, &target, true).is_ok());
        assert!(!v.page_path(&target).exists());
        let registry = read_registry(&v).unwrap();
        assert!(!registry.pages.contains_key(&target));
    }

    #[test]
    fn delete_reports_unknown_and_traversal_ids() {
        let (_t, v) = setup();
        ensure_page(&v, "concept", "Keeper", None, GateMode::Off).unwrap();
        assert!(delete_page(&v, "concepts/never-existed", false)
            .unwrap_err()
            .contains("does not exist"));
        assert!(delete_page(&v, "../escape", false)
            .unwrap_err()
            .contains("invalid page id"));
        assert!(delete_page(&v, "/etc/passwd", false)
            .unwrap_err()
            .contains("invalid page id"));
    }

    #[test]
    fn gate_validate_rejects_dangling_wikilink() {
        let (_t, v) = setup();
        let err = ensure_page(
            &v,
            "concept",
            "X",
            Some("see [[concepts/missing]]\n"),
            GateMode::Validate,
        )
        .unwrap_err();
        assert!(err.contains("does not resolve"));
    }

    #[test]
    fn gate_leaves_code_samples_alone() {
        let (_t, v) = setup();
        let (id, _) = ensure_page(&v, "concept", "Rag", None, GateMode::Off).unwrap();
        let body = "---\ntitle: \"Rag\"\ntype: concept\n---\n\nprose see [[concepts/rag]] ok\n\ninline `[[concepts/missing]]` sample\n\n```\n[[concepts/fenced-missing]]\n```\n";
        write_page(&v, &id, body, GateMode::Normalize).unwrap();
        let content = read_page(&v, &id).unwrap();
        assert!(content.contains("prose see [concepts/rag](/concepts/rag.md) ok"));
        assert!(
            content.contains("`[[concepts/missing]]`"),
            "inline code rewritten: {content}"
        );
        assert!(
            content.contains("[[concepts/fenced-missing]]"),
            "fenced block rewritten: {content}"
        );
    }

    #[test]
    fn gate_validate_ignores_wikilinks_in_code() {
        let (_t, v) = setup();
        let body = "documents the syntax `[[concepts/never-was]]`\n\n```\n[[concepts/nope]]\n```\n";
        ensure_page(&v, "concept", "Docs", Some(body), GateMode::Validate).unwrap();
    }

    #[test]
    fn ensure_write_read_roundtrip() {
        let (_t, v) = setup();
        // seed a page so validate passes
        let (id, created) = ensure_page(&v, "concept", "Rag Note", None, GateMode::Off).unwrap();
        assert!(created);
        assert_eq!(id, "concepts/rag-note");
        // ensure again: no overwrite
        let (id2, created2) = ensure_page(&v, "concept", "rag note", None, GateMode::Off).unwrap();
        assert!(!created2);
        assert_eq!(id2, id);

        write_page(&v, &id, "---\ntitle: \"Rag Note\"\ntype: concept\n---\n\nupdated body [[concepts/rag-note]] self\n", GateMode::Off).unwrap();
        let content = read_page(&v, &id).unwrap();
        assert!(content.contains("updated body"));

        // guardrail: cannot create via write_page
        let err = write_page(&v, "concepts/nope", "x", GateMode::Off).unwrap_err();
        assert!(err.contains("wiki_ensure_page"));

        // normalize gate rewrites wikilinks on write
        write_page(
            &v,
            &id,
            "---\ntitle: \"Rag Note\"\ntype: concept\n---\n\nsee [[concepts/rag-note]] ok\n",
            GateMode::Normalize,
        )
        .unwrap();
        let content = read_page(&v, &id).unwrap();
        assert!(content.contains("[concepts/rag-note](/concepts/rag-note.md)"));
    }

    #[test]
    fn write_page_rejects_fenceless_content() {
        let (_t, v) = setup();
        let (id, _) = ensure_page(&v, "concept", "Fence Guard", None, GateMode::Off).unwrap();
        let err = write_page(&v, &id, "bare body without fence\n", GateMode::Off).unwrap_err();
        assert!(err.contains("fenceless"));
        // page unchanged
        let content = read_page(&v, &id).unwrap();
        assert!(!content.contains("bare body"));
    }

    #[test]
    fn template_returns_filled_scaffold() {
        let (_t, v) = setup();
        let body = template(&v, "concept").unwrap();
        assert!(body.starts_with("---\ntype: concept"));
        assert!(body.contains("{title}"));
        assert!(!body.contains("{date}")); // filled with today
        let src = template(&v, "source").unwrap();
        assert!(src.contains("format: article"));
        // `retro` shares the `sources/` folder; the scaffold exists so
        // ensure_page can create one on demand.
        let ret = template(&v, "retro").unwrap();
        assert!(ret.starts_with("---\ntype: retro"));
        let (id, created) =
            ensure_page(&v, "retro", "Made Up Folders", None, GateMode::Off).unwrap();
        assert!(created);
        assert_eq!(id, "sources/made-up-folders");
        let err = template(&v, "nope").unwrap_err();
        assert!(err.contains("unknown page type"));
        // The message is derived from PAGE_TYPES, so it cannot go stale.
        assert!(err.contains("requirement") && err.contains("retro"));
    }

    #[test]
    fn templated_pages_pass_registry_scan() {
        // Every bootstrap template must survive the fail-closed frontmatter
        // scan: created pages enter the registry with zero diagnostics.
        let (_t, v) = setup();
        for t in [
            "concept",
            "entity",
            "source",
            "analysis",
            "synthesis",
            "requirement",
            "retro",
        ] {
            let scaffold = template(&v, t).unwrap().replace("{title}", "Probe");
            let folder = match t {
                "source" | "retro" => "sources",
                "requirement" => "requirements",
                _ => "concepts",
            };
            let path = v.wiki_pages().join(format!("{folder}/probe-{t}.md"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &scaffold).unwrap();
        }
        let reg = super::super::registry::rebuild_metadata(&v).unwrap();
        assert!(reg.diagnostics.is_empty(), "{:?}", reg.diagnostics);
        assert_eq!(reg.pages.len(), 7);
    }

    #[test]
    fn retro_and_observe_write_sources() {
        let (_t, v) = setup();
        let id = retro(
            &v,
            "jwt-fix",
            "JWT revocation fix",
            "learned [[concepts/x]]\n",
            Some("bugfix"),
            Some("high"),
            GateMode::Off,
        )
        .unwrap();
        assert_eq!(id, "sources/jwt-fix");
        let text = fs::read_to_string(v.page_path(&id)).unwrap();
        assert!(text.contains("type: retro\n"), "{text}");
        assert!(text.contains("relevance: high\n"), "{text}");
        assert!(text.contains("category: bugfix\n"), "{text}");

        let dup = retro(&v, "jwt-fix", "dup", "b", None, None, GateMode::Off).unwrap_err();
        assert!(dup.contains("already exists"));
        let bad_rel =
            retro(&v, "other", "t", "b", None, Some("urgent"), GateMode::Off).unwrap_err();
        assert!(bad_rel.contains("relevance"), "{bad_rel}");

        let obs = observe(
            &v,
            "2026-09-07",
            &ObserveInput {
                title: "Decided KISS port",
                content: "we port scoring not tantivy",
                relevance: "high",
                tags: Some("rust wiki"),
                source_context: Some("rust-wiki build"),
            },
            GateMode::Off,
        )
        .unwrap();
        assert!(obs.starts_with("sources/obs-2026-09-07-"));
        // A mid-session note is a retro: one name per artifact.
        let text = fs::read_to_string(v.page_path(&obs)).unwrap();
        assert!(text.contains("type: retro\n"), "{text}");
        assert!(!text.contains("observation"), "{text}");
        assert!(text.contains("relevance: high\n"), "{text}");
        assert!(text.contains("tags: rust wiki\n"), "{text}");
        assert!(
            text.contains("source_context: \"rust-wiki build\"\n"),
            "{text}"
        );

        let bad = observe(
            &v,
            "2026-09-07",
            &ObserveInput {
                title: "Bad",
                content: "c",
                relevance: "urgent",
                tags: None,
                source_context: None,
            },
            GateMode::Off,
        )
        .unwrap_err();
        assert!(bad.contains("relevance"));

        // Both writers land as retros carrying their relevance claim.
        let reg = rebuild_metadata(&v).unwrap();
        let entry = reg.pages.get(&id).unwrap();
        assert_eq!(entry.page_type, "retro");
        assert_eq!(entry.relevance.as_deref(), Some("high"));
        let entry = reg.pages.get(&obs).unwrap();
        assert_eq!(entry.page_type, "retro");
        assert_eq!(entry.relevance.as_deref(), Some("high"));
    }

    #[test]
    fn ensure_page_with_records_relevance() {
        let (_t, v) = setup();
        let (id, _) = ensure_page_with(
            &v,
            PageSpec {
                page_type: "concept",
                title: "Weighed",
                content: None,
                relevance: Some("high"),
            },
            GateMode::Off,
        )
        .unwrap();
        let reg = rebuild_metadata(&v).unwrap();
        assert_eq!(reg.pages[&id].relevance.as_deref(), Some("high"));

        // The declared vocabulary is enforced at the write door.
        let err = ensure_page_with(
            &v,
            PageSpec {
                page_type: "concept",
                title: "Bad",
                content: None,
                relevance: Some("urgent"),
            },
            GateMode::Off,
        )
        .unwrap_err();
        assert!(err.contains("invalid relevance"), "{err}");

        // A caller-supplied fence cannot be merged with the argument — refuse
        // loudly rather than silently drop the claim.
        let err = ensure_page_with(
            &v,
            PageSpec {
                page_type: "concept",
                title: "Own Fence",
                content: Some("---\ntitle: \"Own\"\ntype: concept\n---\n\nbody\n"),
                relevance: Some("high"),
            },
            GateMode::Off,
        )
        .unwrap_err();
        assert!(err.contains("own frontmatter"), "{err}");
    }
}
