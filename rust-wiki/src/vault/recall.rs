//! Recall: chunk-level scoring over registry + on-demand page chunks,
//! pseudo-relevance feedback, links-first gate, personal-layer merge.
//! A faithful-but-KISS port of zosmaai's recall intent.

use std::fs;
use std::path::Path;

use super::layout::VaultPaths;
use super::registry::{rebuild_metadata, Registry};

/// Chunk size for body scanning (~ chars).
const CHUNK: usize = 600;
/// Pages above this count => links-first (no full previews).
pub const LINKS_FIRST_THRESHOLD: u64 = 50;
/// Field weights (KISS: title/id dominate, type assists).
const W_TITLE: f64 = 3.0;
const W_ID: f64 = 2.0;
const W_TYPE: f64 = 1.5;
const W_BODY: f64 = 1.0;
/// Pseudo-relevance feedback: top docs + their top terms, one round.
const PRF_DOCS: usize = 3;
const PRF_TERMS: usize = 4;

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(|t| t.to_string())
        .collect()
}

/// Score of a token set against a field. Simple overlap with saturation.
fn field_score(tokens: &[String], field: &str, weight: f64) -> f64 {
    if tokens.is_empty() {
        return 0.0;
    }
    let hay = tokenize(field);
    if hay.is_empty() {
        return 0.0;
    }
    let hits = tokens.iter().filter(|t| hay.contains(t)).count() as f64;
    // saturating: hits/all_query * weight
    (hits / tokens.len() as f64) * weight
}

fn chunks_of(text: &str) -> Vec<String> {
    let body: Vec<&str> = text.lines().filter(|l| !l.trim_start().starts_with('#')).collect();
    let joined = body.join("\n");
    if joined.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    let chars: Vec<char> = joined.chars().collect();
    while start < chars.len() {
        let end = (start + CHUNK).min(chars.len());
        out.push(chars[start..end].iter().collect());
        start = end;
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecallHit {
    pub id: String,
    pub title: String,
    pub page_type: String,
    pub score: f64,
    pub preview: String,
    pub layer: Option<String>,
}

/// Recall within ONE vault directory (space or personal).
fn recall_one(vault: &VaultPaths, query: &str, max_results: u32, layer: Option<&str>) -> Vec<RecallHit> {
    if !vault.registry_file().exists() {
        return vec![];
    }
    let Ok(raw) = fs::read_to_string(vault.registry_file()) else { return vec![] };
    let Ok(registry) = serde_json::from_str::<Registry>(&raw) else { return vec![] };
    recall_registry(vault, &registry, query, max_results, layer)
}

pub fn recall_registry(vault: &VaultPaths, registry: &Registry, query: &str, max_results: u32, layer: Option<&str>) -> Vec<RecallHit> {
    let mut tokens = tokenize(query);
    if tokens.is_empty() {
        return vec![];
    }
    // Pass 1 over lightweight fields.
    let mut scored: Vec<(f64, &super::registry::PageEntry)> = registry
        .pages
        .values()
        .map(|p| {
            let s = field_score(&tokens, &p.title, W_TITLE)
                + field_score(&tokens, &p.id, W_ID)
                + field_score(&tokens, &p.page_type, W_TYPE)
                + field_score(&tokens, &p.excerpt, W_BODY);
            (s, p)
        })
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // PRF: only when the first pass has enough evidence (>= PRF_DOCS hits);
    // pull top terms from top docs' bodies, expand query, rescore.
    if scored.len() >= PRF_DOCS {
        let mut extra: Vec<String> = Vec::new();
        for (_, p) in scored.iter().take(PRF_DOCS) {
            let Ok(text) = fs::read_to_string(vault.space_root.join(&p.path)) else { continue };
            let mut doc = tokenize(&text);
            doc.retain(|t| !tokens.contains(t));
            doc.sort();
            doc.dedup();
            extra.extend(doc.into_iter().take(PRF_TERMS));
        }
        if !extra.is_empty() {
            tokens.extend(extra);
            tokens.dedup();
            scored = registry
                .pages
                .values()
                .map(|p| {
                    let s = field_score(&tokens, &p.title, W_TITLE)
                        + field_score(&tokens, &p.id, W_ID)
                        + field_score(&tokens, &p.page_type, W_TYPE)
                        + field_score(&tokens, &p.excerpt, W_BODY);
                    (s, p)
                })
                .filter(|(s, _)| *s > 0.0)
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    // Best chunk preview: rescan the body of the top hits for the query terms.
    let mut hits = Vec::new();
    for (score, p) in scored.into_iter().take(max_results as usize) {
        let body = fs::read_to_string(vault.space_root.join(&p.path)).unwrap_or_default();
        let best = best_chunk(&chunks_of(&body), &tokens);
        hits.push(RecallHit {
            id: p.id.clone(),
            title: p.title.clone(),
            page_type: p.page_type.clone(),
            score: score * (1.0 + best.1 * 0.25), // chunk proximity bonus
            preview: best.0.unwrap_or_else(|| p.excerpt.clone()),
            layer: layer.map(|s| s.to_string()),
        });
    }
    hits
}

fn best_chunk(chunks: &[String], tokens: &[String]) -> (Option<String>, f64) {
    let mut best: Option<(f64, &String)> = None;
    for c in chunks {
        let hay = tokenize(c);
        let hits = tokens.iter().filter(|t| hay.contains(t)).count() as f64;
        if hits > 0.0 && best.map(|(s, _)| hits > s).unwrap_or(true) {
            best = Some((hits, c));
        }
    }
    match best {
        Some((_, c)) => {
            let snippet: String = c.chars().take(200).collect();
            (Some(snippet), 1.0)
        }
        None => (None, 0.0),
    }
}

/// Layered recall: active space first, then `personal`, dedup by id.
pub fn recall_layered(
    space_vault: &VaultPaths,
    personal_vault: Option<&VaultPaths>,
    registry: &Registry,
    query: &str,
    max_results: u32,
) -> (Vec<RecallHit>, bool) {
    let space_hits = recall_registry(space_vault, registry, query, max_results, None);
    let mut hits = space_hits;
    if let Some(pv) = personal_vault {
        let personal = recall_one(pv, query, max_results, Some("personal"));
        for h in personal {
            if !hits.iter().any(|x| x.id == h.id) {
                hits.push(h);
            }
        }
    }
    // links-first: previews trimmed when vault is big
    let links_first = registry.pages.len() as u64 > LINKS_FIRST_THRESHOLD;
    if links_first {
        for h in &mut hits {
            h.preview = h.preview.chars().take(80).collect();
        }
    }
    hits.truncate(max_results as usize);
    (hits, links_first)
}

/// Ensure the registry exists + is fresh enough for recall; rebuild when absent.
pub fn ensure_registry(vault: &VaultPaths) -> Result<Registry, String> {
    if vault.registry_file().exists() {
        let raw = fs::read_to_string(vault.registry_file()).map_err(|e| e.to_string())?;
        if let Ok(reg) = serde_json::from_str::<Registry>(&raw) {
            return Ok(reg);
        }
    }
    rebuild_metadata(vault)
}

/// Path guard: only serve pages inside wiki/**.
pub fn readable_page(vault: &VaultPaths, id: &str) -> Result<std::path::PathBuf, String> {
    let p = vault.page_path(id);
    let canon_id = super::super::vault::layout::Ownership::Wiki; // marker only, see ownership()
    let _ = canon_id;
    if id.contains("..") {
        return Err(format!("invalid page id '{id}'"));
    }
    if !p.exists() {
        return Err(format!("page '{id}' not found"));
    }
    let _ = Path::new(&p).is_file();
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::bootstrap::bootstrap;

    fn setup() -> (tempfile::TempDir, VaultPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        bootstrap(&v, "t0").unwrap();
        (tmp, v)
    }

    fn page(v: &VaultPaths, id: &str, body: &str) {
        let p = v.page_path(id);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn title_hit_beats_body_hit_and_layering_dedups() {
        let (_t, v) = setup();
        page(&v, "concepts/rag", "# RAG\n\nretrieval augmented generation overview\n");
        page(&v, "concepts/llm-basics", "# LLM Basics\n\nmentions rag once in body\n");
        page(&v, "entities/unrelated", "# Vendor\n\nnothing here\n");
        let reg = rebuild_metadata(&v).unwrap();

        let hits = recall_registry(&v, &reg, "rag", 5, None);
        assert_eq!(hits[0].id, "concepts/rag");
        assert!(hits[0].score > hits[1].score);
        assert!(hits.iter().all(|h| h.id != "entities/unrelated"));

        // layering: personal hits appended, labeled, deduped by id
        let tmp2 = tempfile::tempdir().unwrap();
        let pv = VaultPaths::new(tmp2.path(), "personal");
        bootstrap(&pv, "t0").unwrap();
        page(&pv, "concepts/rag", "# RAG\n\nduplicate id in personal\n");
        page(&pv, "concepts/private-note", "# Private\n\nrag notes personal only\n");
        let reg_p = rebuild_metadata(&pv).unwrap();
        let _ = reg_p;
        let (merged, links_first) = recall_layered(&v, Some(&pv), &reg, "rag", 5);
        assert!(!links_first);
        assert_eq!(merged[0].id, "concepts/rag");
        assert!(merged[0].layer.is_none()); // space layer wins
        let personal = merged.iter().find(|h| h.layer.as_deref() == Some("personal")).expect("personal hit present");
        assert_eq!(personal.id, "concepts/private-note"); // dup id dropped
    }

    #[test]
    fn links_first_gate_trims_previews() {
        let (_t, v) = setup();
        for i in 0..60 {
            page(&v, &format!("concepts/page-{i}"), &format!("# P{i}\n\ncommon rag token {i}\n"));
        }
        let reg = rebuild_metadata(&v).unwrap();
        let (hits, links_first) = recall_layered(&v, None, &reg, "rag", 5);
        assert!(links_first);
        assert!(hits.iter().all(|h| h.preview.chars().count() <= 80));
    }
}
