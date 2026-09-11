//! Optional semantic layer: page embeddings via an OpenAI-compatible
//! endpoint. Server config is env-only (WIKI_EMBEDDING_URL /
//! WIKI_EMBEDDING_MODEL / WIKI_EMBEDDING_TOKEN). Without a provider the
//! feature is a clean no-op and recall stays purely lexical.
//!
//! Vectors are per **chunk**, not per page: a long page's score is its
//! best-matching chunk, so unrelated sections cannot dilute it.

use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use super::layout::VaultPaths;
use super::registry::Registry;

/// Provider seam: embed a batch of texts. Test-stubbable.
pub trait Embedder: Send + Sync {
    fn model(&self) -> &str;
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

/// OpenAI-compatible /embeddings client (blocking).
pub struct HttpEmbedder {
    pub url: String,
    pub model: String,
    pub token: Option<String>,
}

impl Embedder for HttpEmbedder {
    fn model(&self) -> &str {
        &self.model
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        #[derive(Deserialize)]
        struct Resp {
            data: Vec<Data>,
        }
        #[derive(Deserialize)]
        struct Data {
            embedding: Vec<f32>,
        }
        // Cold-start on a self-hosted embedding model measured at ~54s
        // (2026-09-10), too close to a 60s ceiling. 180s leaves headroom
        // for the first call; warm calls are ~12ms.
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .map_err(|e| e.to_string())?;
        let mut req = client.post(&self.url).json(&serde_json::json!({
            "model": self.model,
            "input": texts
        }));
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .map_err(|e| format!("embedding request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "embedding endpoint returned HTTP {}",
                resp.status()
            ));
        }
        let parsed: Resp = resp
            .json()
            .map_err(|e| format!("bad embedding response: {e}"))?;
        if parsed.data.len() != texts.len() {
            return Err(format!(
                "embedding count mismatch: sent {} got {}",
                texts.len(),
                parsed.data.len()
            ));
        }
        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }
}

/// Build the provider from central config. None = feature off.
pub fn from_env() -> Option<Box<dyn Embedder>> {
    let cfg = crate::config::get();
    let url = cfg.embedding_url.clone()?;
    let model = cfg.embedding_model.clone();
    let token = cfg.embedding_token.clone();
    Some(Box::new(HttpEmbedder { url, model, token }))
}

/// Target chunk size in chars: a few paragraphs. Large enough to carry one
/// complete idea, small enough that sections do not average each other out.
pub const CHUNK_CHARS: usize = 800;
/// Per-page chunk ceiling: bounds the embedding cost of a very long page.
/// ponytail: flat cap — raise it (or add a vector index) only if long pages
/// start losing real content.
pub const MAX_CHUNKS: usize = 24;
/// Bounded provider batch: a full vault must not become one huge request.
pub const EMBED_BATCH: usize = 64;

/// Split a page body into embedding chunks: paragraphs packed up to
/// `CHUNK_CHARS`, each prefixed with the page title so a bare chunk still
/// carries context. An empty body yields no chunks.
pub fn chunk_text(title: &str, body: &str) -> Vec<String> {
    let mut packed: Vec<String> = Vec::new();
    let mut buf = String::new();
    for para in body.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        for piece in hard_split(para, CHUNK_CHARS) {
            let over =
                !buf.is_empty() && buf.chars().count() + piece.chars().count() + 2 > CHUNK_CHARS;
            if over {
                packed.push(std::mem::take(&mut buf));
                if packed.len() >= MAX_CHUNKS {
                    break;
                }
            }
            if !buf.is_empty() {
                buf.push_str("\n\n");
            }
            buf.push_str(&piece);
        }
        if packed.len() >= MAX_CHUNKS {
            break;
        }
    }
    if !buf.is_empty() && packed.len() < MAX_CHUNKS {
        packed.push(buf);
    }
    packed
        .into_iter()
        .map(|c| format!("{title}\n\n{c}"))
        .collect()
}

/// Split an oversized paragraph at char boundaries so no chunk exceeds `max`.
fn hard_split(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut n = 0;
    for ch in text.chars() {
        cur.push(ch);
        n += 1;
        if n >= max {
            out.push(std::mem::take(&mut cur));
            n = 0;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Page body with the frontmatter block removed (the whole text when absent).
fn body_of(raw: &str) -> &str {
    super::frontmatter::split_block(raw)
        .map(|(_, body)| body)
        .unwrap_or(raw)
}

/// Chunks for one page: its body from disk (frontmatter stripped), falling
/// back to the registry excerpt when the file cannot be read. The semantic
/// layer is best-effort — it never fails a write or a reindex.
pub fn page_chunks(vault: &VaultPaths, id: &str, title: &str, excerpt: &str) -> Vec<String> {
    let raw = fs::read_to_string(vault.page_path(id)).ok();
    let body = match &raw {
        Some(raw) => body_of(raw),
        None => excerpt,
    };
    chunk_text(title, body)
}

/// One page's vectors plus the hash of the text they were built from.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PageVectors {
    /// Hash of the embedded chunk text. Same hash under the same model = skip:
    /// no provider call, no write.
    pub hash: String,
    /// One vector per chunk of the page body.
    pub chunks: Vec<Vec<f32>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct EmbeddingStore {
    pub model: String,
    pub pages: BTreeMap<String, PageVectors>,
}

/// Stable 64-bit FNV-1a over the embedded text. Change detection only — not a
/// security hash — but deliberately hand-rolled: `DefaultHasher` is explicitly
/// not stable across releases, and this value is persisted.
fn content_hash(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

impl EmbeddingStore {
    pub fn load(vault: &VaultPaths) -> Option<EmbeddingStore> {
        let raw = fs::read_to_string(vault.meta().join("embeddings.json")).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self, vault: &VaultPaths) -> Result<(), String> {
        let path = vault.meta().join("embeddings.json");
        let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }
}

/// Drop a page's vectors after it is deleted.
///
/// The semantic candidate pass admits ids straight from this store, so a
/// deleted page left in here resurfaces in recall as a result that no longer
/// exists. Best-effort like the upsert: a failed save never blocks the delete.
pub fn forget_page(vault: &VaultPaths, id: &str) -> bool {
    let Some(mut store) = EmbeddingStore::load(vault) else {
        return false;
    };
    if store.pages.remove(id).is_none() {
        return false;
    }
    store.save(vault).is_ok()
}

/// Best-effort single-page upsert after a write (option A: auto-embed on
/// change). Reads the page body from disk, so vectors always reflect what is
/// actually stored. Lazy-creates the store (recording the live model) when
/// absent — single-page cost only, so the write path never pays a backfill.
/// Silent no-op when: the store exists but cannot be read as the current
/// format (skipping beats replacing every other page's vectors with a
/// one-page store), the stored model differs from the embedder, the page is
/// unknown to the registry, the chunks are unchanged, or embedding fails.
/// Writes are never blocked by this.
pub fn upsert_page(vault: &VaultPaths, registry: &Registry, embedder: &dyn Embedder, id: &str) {
    let store_file = vault.meta().join("embeddings.json");
    let mut store = match EmbeddingStore::load(vault) {
        Some(s) => s,
        None if store_file.exists() => return,
        None => EmbeddingStore {
            model: embedder.model().to_string(),
            pages: Default::default(),
        },
    };
    if store.model != embedder.model() {
        return;
    }
    let Some(p) = registry.pages.get(id) else {
        return;
    };
    let chunks = page_chunks(vault, id, &p.title, &p.excerpt);
    if chunks.is_empty() {
        return;
    }
    let hash = content_hash(&chunks.join("\n"));
    // Unchanged text under the same model: skip the provider call entirely.
    if store
        .pages
        .get(id)
        .is_some_and(|pv| pv.hash == hash && !pv.chunks.is_empty())
    {
        return;
    }
    let Ok(vectors) = embedder.embed(&chunks) else {
        return;
    };
    store.pages.insert(
        id.to_string(),
        PageVectors {
            hash,
            chunks: vectors,
        },
    );
    let _ = store.save(vault);
}

pub fn embeddings_path(vault: &VaultPaths) -> std::path::PathBuf {
    vault.meta().join("embeddings.json")
}

/// Outcome of a reindex.
#[derive(Debug, Serialize, Default, PartialEq)]
pub struct ReindexReport {
    /// Pages whose vectors were rebuilt and persisted.
    pub embedded: usize,
    /// Pages skipped because text and model were already current.
    pub skipped: usize,
}

/// Re-embed every page in the registry, chunk by chunk. Pages whose text hash
/// and model are unchanged are skipped — a reindex over an unchanged vault
/// costs no provider calls.
pub fn reindex(
    vault: &VaultPaths,
    registry: &Registry,
    embedder: &dyn Embedder,
) -> Result<ReindexReport, String> {
    let previous = EmbeddingStore::load(vault);
    let same_model = previous
        .as_ref()
        .is_some_and(|s| s.model == embedder.model());
    let mut store = EmbeddingStore {
        model: embedder.model().to_string(),
        pages: Default::default(),
    };
    let mut skipped = 0usize;
    // One flat batch across every page (BTreeMap order = deterministic),
    // embedded in bounded windows so a large vault cannot send one enormous
    // request. `owners` maps each text back to its page id.
    let mut owners: Vec<&str> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut hashes: Vec<(&str, String)> = Vec::new();
    for (id, page) in &registry.pages {
        let chunks = page_chunks(vault, id, &page.title, &page.excerpt);
        if chunks.is_empty() {
            continue;
        }
        let hash = content_hash(&chunks.join("\n"));
        if same_model {
            if let Some(prev) = previous.as_ref().and_then(|s| s.pages.get(id)) {
                if prev.hash == hash && !prev.chunks.is_empty() {
                    store.pages.insert(
                        id.clone(),
                        PageVectors {
                            hash,
                            chunks: prev.chunks.clone(),
                        },
                    );
                    skipped += 1;
                    continue;
                }
            }
        }
        hashes.push((id.as_str(), hash));
        for chunk in chunks {
            owners.push(id.as_str());
            texts.push(chunk);
        }
    }
    for start in (0..texts.len()).step_by(EMBED_BATCH) {
        let end = usize::min(start + EMBED_BATCH, texts.len());
        let vectors = embedder.embed(&texts[start..end])?;
        for (owner, vector) in owners[start..end].iter().zip(vectors) {
            store
                .pages
                .entry((*owner).to_string())
                .or_default()
                .chunks
                .push(vector);
        }
    }
    // Record the text hash once per page, and only for pages that actually
    // got vectors — a page the provider skipped stays unstamped and is
    // re-embedded next time.
    for (id, hash) in hashes {
        if let Some(p) = store.pages.get_mut(id) {
            p.hash = hash;
        }
    }
    // Persist even when nothing was embeddable, so recall can tell
    // "semantic ran, found nothing" apart from "no store at all".
    store.save(vault)?;
    Ok(ReindexReport {
        embedded: store.pages.len() - skipped,
        skipped,
    })
}

/// Cosine similarity; 0 when either vector is empty.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// A page's semantic score is its best-matching chunk: one strongly relevant
/// section beats an average over the whole page.
pub fn best_similarity(query: &[f32], chunks: &[Vec<f32>]) -> f32 {
    chunks
        .iter()
        .map(|c| cosine(query, c))
        .fold(0.0f32, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::bootstrap::bootstrap;

    pub struct MockEmbedder;
    impl Embedder for MockEmbedder {
        fn model(&self) -> &str {
            "mock"
        }
        // toy embedding: character n-gram bag hashed into 8 dims
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            Ok(texts
                .iter()
                .map(|t| {
                    let mut v = vec![0.0f32; 8];
                    for w in t.to_lowercase().split_whitespace() {
                        let h = w.bytes().map(|b| b as usize).sum::<usize>();
                        v[h % 8] += 1.0;
                    }
                    v
                })
                .collect())
        }
    }

    fn setup() -> (tempfile::TempDir, VaultPaths, Registry) {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        bootstrap(&v, "t").unwrap();
        let mut reg = Registry::default();
        let mut mk = |id: &str, title: &str, excerpt: &str| {
            reg.pages.insert(
                id.into(),
                super::super::registry::PageEntry {
                    id: id.into(),
                    title: title.into(),
                    page_type: "concept".into(),
                    path: format!("wiki/{id}.md"),
                    links: vec![],
                    excerpt: excerpt.into(),
                    description: String::new(),
                    relevance: None,
                    source_id: None,
                },
            );
        };
        mk(
            "concepts/retrieval",
            "retrieval",
            "retrieval systems ground answers",
        );
        mk("concepts/quota", "quota", "multi user quota limits");
        (tmp, v, reg)
    }

    #[test]
    fn forget_page_drops_only_the_deleted_pages_vectors() {
        let (_t, v, reg) = setup();
        reindex(&v, &reg, &MockEmbedder).unwrap();
        assert!(forget_page(&v, "concepts/quota"));
        let store = EmbeddingStore::load(&v).unwrap();
        assert!(!store.pages.contains_key("concepts/quota"));
        assert!(store.pages.contains_key("concepts/retrieval"));
        // Unknown id and missing store are both no-ops, not errors.
        assert!(!forget_page(&v, "concepts/quota"));
        assert!(!forget_page(&v, "concepts/never-existed"));
        let (_t2, v2, _r2) = setup();
        assert!(!forget_page(&v2, "concepts/quota"));
    }

    #[test]
    fn reindex_persists_all_pages_and_reloads() {
        let (_t, v, reg) = setup();
        let r = reindex(&v, &reg, &MockEmbedder).unwrap();
        assert_eq!(
            r,
            ReindexReport {
                embedded: 2,
                skipped: 0
            }
        );
        let store = EmbeddingStore::load(&v).unwrap();
        assert_eq!(store.model, "mock");
        assert_eq!(store.pages.len(), 2);
        assert!(store.pages.contains_key("concepts/retrieval"));
    }

    #[test]
    fn upsert_creates_store_when_absent() {
        let (_t, v, reg) = setup();
        assert!(EmbeddingStore::load(&v).is_none());
        upsert_page(&v, &reg, &MockEmbedder, "concepts/retrieval");
        let store = EmbeddingStore::load(&v).unwrap();
        assert_eq!(store.model, "mock");
        assert_eq!(store.pages.len(), 1);
        assert!(store.pages.contains_key("concepts/retrieval"));
    }

    #[test]
    fn cosine_direction_and_edge_cases() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
        assert_eq!(cosine(&[], &[1.0]), 0.0);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn long_bodies_split_and_stay_capped() {
        let para = "lorem ipsum dolor sit amet ".repeat(20);
        let body = (0..40)
            .map(|_| para.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let chunks = chunk_text("Big page", &body);
        assert!(chunks.len() > 1, "a long body must split");
        assert_eq!(chunks.len(), MAX_CHUNKS, "stops at the per-page ceiling");
        for c in &chunks {
            assert!(c.starts_with("Big page"), "every chunk carries the title");
            assert!(
                c.chars().count() <= CHUNK_CHARS + "Big page\n\n".len() + 2,
                "chunk exceeded the target size: {}",
                c.chars().count()
            );
        }
        assert!(
            chunk_text("Empty", "   \n\n  ").is_empty(),
            "no body, no chunks"
        );
    }

    #[test]
    fn page_chunks_strip_frontmatter_and_fall_back_to_the_excerpt() {
        let (_t, v, reg) = setup();

        // No file on disk -> the excerpt keeps the semantic layer working.
        let chunks = page_chunks(&v, "concepts/retrieval", "retrieval", "excerpt fallback");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("excerpt fallback"));

        // A real file -> frontmatter is never embedded, body is chunked.
        let path = v.page_path("concepts/retrieval");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
                "---\ntitle: retrieval\ntype: concept\n---\n\n{}",
                "grounding ".repeat(300)
            ),
        )
        .unwrap();
        let chunks = page_chunks(&v, "concepts/retrieval", "retrieval", "excerpt fallback");
        assert!(chunks.len() > 1, "a long body yields several chunks");
        assert!(
            !chunks[0].contains("title: retrieval"),
            "frontmatter must not be embedded"
        );
        assert_eq!(reg.pages.len(), 2);
    }

    #[test]
    fn reindex_stores_one_vector_per_chunk() {
        let (_t, v, reg) = setup();
        let path = v.page_path("concepts/retrieval");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!("---\ntitle: retrieval\n---\n\n{}", "grounding ".repeat(300)),
        )
        .unwrap();

        let r = reindex(&v, &reg, &MockEmbedder).unwrap();
        assert_eq!(
            r,
            ReindexReport {
                embedded: 2,
                skipped: 0
            },
            "both pages got vectors"
        );
        let store = EmbeddingStore::load(&v).unwrap();
        assert!(
            store.pages["concepts/retrieval"].chunks.len() > 1,
            "long page -> several chunk vectors"
        );
        assert_eq!(
            store.pages["concepts/quota"].chunks.len(),
            1,
            "short page -> a single chunk"
        );
    }

    #[test]
    fn reindex_skips_pages_whose_text_is_unchanged() {
        let (_t, v, reg) = setup();
        let first = reindex(&v, &reg, &MockEmbedder).unwrap();
        assert_eq!(first.embedded, 2);

        // Same text, same model -> nothing re-embedded, vectors preserved.
        let again = reindex(&v, &reg, &MockEmbedder).unwrap();
        assert_eq!(
            again,
            ReindexReport {
                embedded: 0,
                skipped: 2
            }
        );
        let store = EmbeddingStore::load(&v).unwrap();
        assert_eq!(store.pages.len(), 2, "skipped pages keep their vectors");

        // Editing one page's body re-embeds that page only.
        let path = v.page_path("concepts/quota");
        fs::write(&path, "---\ntitle: quota\n---\n\nbrand new body\n").unwrap();
        let after_edit = reindex(&v, &reg, &MockEmbedder).unwrap();
        assert_eq!(
            after_edit,
            ReindexReport {
                embedded: 1,
                skipped: 1
            }
        );

        // A different model invalidates every stored vector.
        let store = EmbeddingStore::load(&v).unwrap();
        let stale = EmbeddingStore {
            model: "other".into(),
            ..store
        };
        stale.save(&v).unwrap();
        let on_new_model = reindex(&v, &reg, &MockEmbedder).unwrap();
        assert_eq!(
            on_new_model,
            ReindexReport {
                embedded: 2,
                skipped: 0
            }
        );
    }

    /// MockEmbedder plus a call counter, to prove the skip really avoids the
    /// provider.
    struct CountingEmbedder(std::sync::atomic::AtomicUsize);
    impl Embedder for CountingEmbedder {
        fn model(&self) -> &str {
            "mock"
        }
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            MockEmbedder.embed(texts)
        }
    }

    #[test]
    fn upsert_skips_an_unchanged_page() {
        use std::sync::atomic::Ordering;
        let (_t, v, reg) = setup();
        let path = v.page_path("concepts/retrieval");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "---\ntitle: retrieval\n---\n\nbody text here\n").unwrap();

        let counter = CountingEmbedder(Default::default());
        upsert_page(&v, &reg, &counter, "concepts/retrieval");
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);

        // Same text, same model -> no provider call at all.
        upsert_page(&v, &reg, &counter, "concepts/retrieval");
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            1,
            "unchanged text skipped"
        );

        // Edited text -> embedded again.
        fs::write(&path, "---\ntitle: retrieval\n---\n\ndifferent body\n").unwrap();
        upsert_page(&v, &reg, &counter, "concepts/retrieval");
        assert_eq!(counter.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn upsert_never_replaces_an_unreadable_store() {
        let (_t, v, reg) = setup();
        fs::create_dir_all(v.meta()).unwrap();
        let file = v.meta().join("embeddings.json");
        fs::write(&file, "{\"older\": \"format\"}").unwrap();

        upsert_page(&v, &reg, &MockEmbedder, "concepts/retrieval");

        // Skipping beats overwriting every other page's vectors with a
        // one-page store; `wiki_reindex_embeddings` rebuilds on request.
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "{\"older\": \"format\"}"
        );
    }

    #[test]
    fn best_similarity_takes_the_best_chunk() {
        let query = vec![1.0f32, 0.0];
        let chunks = vec![vec![0.0f32, 1.0], vec![1.0f32, 0.0]];
        assert!((best_similarity(&query, &chunks) - 1.0).abs() < 1e-6);
        assert_eq!(best_similarity(&query, &[]), 0.0);
    }
}
