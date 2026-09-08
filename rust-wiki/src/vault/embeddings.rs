//! Optional semantic layer: page embeddings via an OpenAI-compatible
//! endpoint. Server config is env-only (WIKI_EMBEDDING_URL /
//! WIKI_EMBEDDING_MODEL / WIKI_EMBEDDING_TOKEN). Without a provider the
//! feature is a clean no-op and recall stays purely lexical.

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
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
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

/// Build the provider from env. None = feature off.
pub fn from_env() -> Option<Box<dyn Embedder>> {
    let url = std::env::var("WIKI_EMBEDDING_URL").ok()?;
    let model =
        std::env::var("WIKI_EMBEDDING_MODEL").unwrap_or_else(|_| "text-embedding-3-small".into());
    let token = std::env::var("WIKI_EMBEDDING_TOKEN").ok();
    Some(Box::new(HttpEmbedder { url, model, token }))
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct EmbeddingStore {
    pub model: String,
    pub pages: BTreeMap<String, Vec<f32>>,
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

/// Best-effort single-page upsert after a write (option A: auto-embed on
/// change). Uses the same title/id/excerpt text shape as `reindex` so
/// vectors stay comparable. Lazy-creates the store (recording the live
/// model) when absent — single-page cost only, so the write path never
/// pays a backfill. Silent no-op when: stored model differs from the
/// embedder, page unknown to the registry, or embedding fails. Writes
/// are never blocked by this.
pub fn upsert_page(vault: &VaultPaths, registry: &Registry, embedder: &dyn Embedder, id: &str) {
    let mut store = match EmbeddingStore::load(vault) {
        Some(s) => s,
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
    let text = format!("{}\n{}\n{}", p.title, p.id, p.excerpt);
    let Ok(mut vectors) = embedder.embed(&[text]) else {
        return;
    };
    let Some(vector) = vectors.pop() else {
        return;
    };
    store.pages.insert(id.to_string(), vector);
    let _ = store.save(vault);
}

pub fn embeddings_path(vault: &VaultPaths) -> std::path::PathBuf {
    vault.meta().join("embeddings.json")
}

/// Re-embed every page in the registry. Returns count embedded.
pub fn reindex(
    vault: &VaultPaths,
    registry: &Registry,
    embedder: &dyn Embedder,
) -> Result<usize, String> {
    let ids: Vec<&str> = registry.pages.keys().map(|s| s.as_str()).collect();
    if ids.is_empty() {
        // still persist an empty store so recall knows embeddings exist
        EmbeddingStore {
            model: embedder.model().to_string(),
            pages: Default::default(),
        }
        .save(vault)?;
        return Ok(0);
    }
    let texts: Vec<String> = ids
        .iter()
        .map(|id| {
            let p = registry.pages.get(*id).expect("id from registry");
            format!("{}\n{}\n{}", p.title, p.id, p.excerpt)
        })
        .collect();
    let vectors = embedder.embed(&texts)?;
    let mut store = EmbeddingStore {
        model: embedder.model().to_string(),
        pages: Default::default(),
    };
    for (id, vec) in ids.into_iter().zip(vectors) {
        store.pages.insert(id.to_string(), vec);
    }
    store.save(vault)?;
    Ok(store.pages.len())
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
    fn reindex_persists_all_pages_and_reloads() {
        let (_t, v, reg) = setup();
        let n = reindex(&v, &reg, &MockEmbedder).unwrap();
        assert_eq!(n, 2);
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
}
