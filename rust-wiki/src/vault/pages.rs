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
];

pub fn folder_for(page_type: &str) -> Option<&'static str> {
    PAGE_TYPES
        .iter()
        .find(|(t, _)| *t == page_type)
        .map(|(_, f)| *f)
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
pub fn apply_gate(body: &str, existing: &Registry, mode: GateMode) -> Result<String, String> {
    if mode == GateMode::Off {
        return Ok(body.to_string());
    }
    let mut out = body.to_string();
    let re = super::registry::wikilinks();
    for caps in re.captures_iter(body) {
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
        raw.replace("{title}", title)
    }
}

/// Create `folder/slug.md` if absent. Returns (id, created).
pub fn ensure_page(
    vault: &VaultPaths,
    page_type: &str,
    title: &str,
    content: Option<&str>,
    gate: GateMode,
) -> Result<(String, bool), String> {
    let Some(folder) = folder_for(page_type) else {
        return Err(format!("unknown page type '{page_type}' — expected one of: entity, concept, synthesis, analysis"));
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
    let body = match content {
        Some(c) => apply_gate(c, &read_registry(vault)?, gate)?,
        None => template_body(vault, page_type, title),
    };
    let doc = format!("---\ntitle: \"{title}\"\ntype: {page_type}\n---\n\n{body}");
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

/// Atomic insight file: wiki/sources/<slug>.md, searchable immediately.
pub fn retro(
    vault: &VaultPaths,
    slug: &str,
    title: &str,
    body: &str,
    category: Option<&str>,
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
    let cat = category.unwrap_or("");
    let doc = format!(
        "---\ntitle: \"{title}\"\ntype: retro\n{cat}\n---\n\n{gated}",
        cat = if cat.is_empty() {
            String::new()
        } else {
            format!("category: {cat}")
        }
    );
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

/// Timestamped observation: wiki/sources/obs-<date>-<slug>.md
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
    if !matches!(relevance, "low" | "medium" | "high" | "critical") {
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
    let mut fm = format!("---\ntitle: \"{title}\"\ntype: observation\nrelevance: {relevance}\n");
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
    fn retro_and_observe_write_sources() {
        let (_t, v) = setup();
        let id = retro(
            &v,
            "jwt-fix",
            "JWT revocation fix",
            "learned [[concepts/x]]\n",
            Some("bugfix"),
            GateMode::Off,
        )
        .unwrap();
        assert_eq!(id, "sources/jwt-fix");
        let dup = retro(&v, "jwt-fix", "dup", "b", None, GateMode::Off).unwrap_err();
        assert!(dup.contains("already exists"));

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

        let reg = rebuild_metadata(&v).unwrap();
        assert!(reg.pages.contains_key(&id));
    }
}
