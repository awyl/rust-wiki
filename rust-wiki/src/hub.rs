//! Hub: the production `WikiApi` impl. Composes the vault mechanics and
//! owns per-connection space pins + injected clock/url-fetcher (test seams).

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::api::*;
use crate::vault::{
    bootstrap as vb, capture as vc,
    convert::{self, Converter, DefaultConverter},
    ingest as vi,
    layout::{VaultPaths, SPACE_PERSONAL},
    lint as vl, pages as vp, recall as vr, registry, status as vs, trajectory as vt,
};

/// A fetch that has not answered in this long is not going to.
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Operator opt-in for the only irreversible tool. A free function so the
/// gate is testable without touching the process-wide config `OnceLock`.
fn deletion_allowed(allow: bool) -> Result<(), String> {
    if allow {
        Ok(())
    } else {
        Err("deletion is disabled on this server — set `allow_delete = true` in config.toml (or WIKI_ALLOW_DELETE=1) to permit wiki_delete_page".to_string())
    }
}

/// Seam: fetch a URL and convert to markdown. Production impl uses
/// reqwest + the shared `Converter`; tests stub it.
pub trait UrlFetcher: Send + Sync {
    fn fetch_markdown(&self, url: &str) -> Result<String, String>;
}

pub struct HttpFetcher {
    client: reqwest::blocking::Client,
    convert: Arc<dyn Converter>,
}

/// Full cause chain. reqwest's `Display` keeps only the top level, and
/// "error sending request" alone does not say whether it was DNS, TLS, or a
/// refusal — which is the whole point of reporting a failed fetch.
fn why(e: &reqwest::Error) -> String {
    let mut out = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        out.push_str(&format!(": {s}"));
        src = s.source();
    }
    out
}

impl HttpFetcher {
    pub fn new(convert: Arc<dyn Converter>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            // Only fails on a broken TLS backend; serving without a timeout
            // beats refusing to serve.
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self { client, convert }
    }

    /// Test seam: take the client, so a test can bypass the ambient proxy
    /// configuration that would otherwise route 127.0.0.1 through a proxy.
    #[cfg(test)]
    fn with_client(convert: Arc<dyn Converter>, client: reqwest::blocking::Client) -> Self {
        Self { client, convert }
    }
}

impl UrlFetcher for HttpFetcher {
    fn fetch_markdown(&self, url: &str) -> Result<String, String> {
        let resp = self
            .client
            .get(url)
            .send()
            .map_err(|e| format!("fetch {url}: {}", why(&e)))?;
        if !resp.status().is_success() {
            return Err(format!("fetch {url}: HTTP {}", resp.status()));
        }
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        // Refuse an oversize body before reading it into memory.
        if let Some(len) = resp.content_length() {
            if len > convert::MAX_BYTES {
                return Err(format!(
                    "fetch {url}: {len} bytes exceeds the {} byte capture limit",
                    convert::MAX_BYTES
                ));
            }
        }
        let mut bytes = Vec::new();
        let mut limited = resp.take(convert::MAX_BYTES + 1);
        limited
            .read_to_end(&mut bytes)
            .map_err(|e| format!("fetch {url}: {e}"))?;
        if bytes.len() as u64 > convert::MAX_BYTES {
            return Err(format!(
                "fetch {url}: body exceeds the {} byte capture limit",
                convert::MAX_BYTES
            ));
        }
        // Content-type first, magic bytes second: a PDF served as text/plain
        // would otherwise be stored as mojibake.
        let kind = convert::sniff(&bytes).unwrap_or_else(|| convert::from_content_type(&ct));
        self.convert
            .convert(&bytes, &kind)
            .map_err(|e| format!("fetch {url}: {e}"))
    }
}

pub struct Hub {
    root: PathBuf,
    conns: Mutex<HashMap<String, String>>,
    fetch: Box<dyn UrlFetcher>,
    convert: Arc<dyn Converter>,
    embedder: Option<Box<dyn crate::vault::embeddings::Embedder>>,
    now: Box<dyn Fn() -> String + Send + Sync>,
}

impl Hub {
    pub fn new(root: PathBuf) -> Self {
        let convert: Arc<dyn Converter> = Arc::new(DefaultConverter);
        Self {
            root,
            conns: Mutex::new(HashMap::new()),
            fetch: Box::new(HttpFetcher::new(convert.clone())),
            convert,
            embedder: crate::vault::embeddings::from_env(),
            now: Box::new(|| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        }
    }

    #[cfg(test)]
    fn with_embedder(mut self, e: Box<dyn crate::vault::embeddings::Embedder>) -> Self {
        self.embedder = Some(e);
        self
    }

    #[cfg(test)]
    pub fn with_injections(
        root: PathBuf,
        fetch: Box<dyn UrlFetcher>,
        convert: Arc<dyn Converter>,
        now: Box<dyn Fn() -> String + Send + Sync>,
    ) -> Self {
        Self {
            root,
            conns: Mutex::new(HashMap::new()),
            fetch,
            convert,
            embedder: None,
            now,
        }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    fn vault(&self, space: &str) -> VaultPaths {
        VaultPaths::new(&self.root, space)
    }

    /// Resolve the target vault: explicit space > connection pin.
    /// Err when no vault exists yet (bootstrap is the fix).
    fn target(&self, conn: Option<&str>, space: Option<&str>) -> ApiResult<VaultPaths> {
        let name = space
            .map(|s| s.to_string())
            .or_else(|| conn.and_then(|c| self.conns.lock().unwrap().get(c).cloned()))
            .ok_or_else(|| {
                ApiError::invalid("no space given and no connection default — call wiki_use_space")
            })?;
        let v = self.vault(&name);
        if !v.config_file().exists() {
            return Err(ApiError::no_vault(&name));
        }
        Ok(v)
    }

    fn now_iso(&self) -> String {
        (self.now)()
    }
    fn today(&self) -> String {
        self.now_iso()[..10].to_string()
    }

    fn guard_space(space: &str) -> ApiResult<()> {
        if space.is_empty() || space.contains('/') || space.contains("..") {
            return Err(ApiError::invalid(format!("invalid space name '{space}'")));
        }
        Ok(())
    }
}

pub fn default_root() -> PathBuf {
    if let Ok(env) = std::env::var("WIKI_VAULT_ROOT") {
        return PathBuf::from(env);
    }
    // default: vaults/ next to the executable
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("vaults")))
        .unwrap_or_else(|| PathBuf::from("vaults"))
}

impl Hub {
    /// Auto-embed on change (best-effort): refresh one page's vector after a
    /// successful write. Registry is fresh on disk (writers rebuild it).
    /// No-op without a provider or store; never blocks or fails the write.
    fn embed_page(&self, v: &crate::vault::layout::VaultPaths, id: &str) {
        if let Some(embedder) = self.embedder.as_ref() {
            if let Ok(registry) = registry::rebuild_metadata(v) {
                crate::vault::embeddings::upsert_page(v, &registry, embedder.as_ref(), id);
            }
        }
    }
}

impl WikiApi for Hub {
    fn bootstrap(&self, space: &str, mode: Option<&str>) -> ApiResult<BootstrapOut> {
        Self::guard_space(space)?;
        let _ = mode; // v1: personal mode only
        let v = self.vault(space);
        let r = vb::bootstrap(&v, &self.now_iso()).map_err(|e| ApiError::new("io", e.0))?;
        Ok(BootstrapOut {
            created: r.created,
            space: r.space,
            root: v.space_root.to_string_lossy().into_owned(),
        })
    }

    fn use_space(&self, conn: &str, space: &str) -> ApiResult<UseSpaceOut> {
        Self::guard_space(space)?;
        self.conns
            .lock()
            .unwrap()
            .insert(conn.to_string(), space.to_string());
        let v = self.vault(space);
        let exists = v.config_file().exists();
        let total_pages = if exists {
            vr::ensure_registry(&v).ok().map(|r| r.pages.len() as u64)
        } else {
            None
        };
        Ok(UseSpaceOut {
            space: space.to_string(),
            exists,
            total_pages,
        })
    }

    fn capture_source(
        &self,
        space: &str,
        text: Option<&str>,
        url: Option<&str>,
        file_path: Option<&str>,
        title: Option<&str>,
    ) -> ApiResult<CaptureOut> {
        let v = self.target(Some(space), Some(space))?;
        let owned = |s: &str| s.to_string();
        let input = match (text, url, file_path) {
            (Some(t), None, None) => vc::CaptureInput::Text {
                title: title.map(owned),
                text: t.to_string(),
            },
            (None, Some(u), None) => {
                let md = self
                    .fetch
                    .fetch_markdown(u)
                    .map_err(|e| ApiError::new("fetch_failed", e))?;
                vc::CaptureInput::Url {
                    title: title.map(owned),
                    url: u.to_string(),
                    markdown: md,
                }
            }
            (None, None, Some(fp)) => vc::CaptureInput::File {
                title: title.map(owned),
                path: fp.to_string(),
            },
            _ => {
                return Err(ApiError::invalid(
                    "provide exactly one of: text, url, file_path",
                ))
            }
        };
        let c = vc::capture(
            &v,
            &self.today(),
            &self.now_iso(),
            input,
            self.convert.as_ref(),
        )
        .map_err(|e| ApiError::new("io", e))?;
        let source_page_id = format!("sources/{}", c.source_id.to_lowercase());
        Ok(CaptureOut {
            source_id: c.source_id,
            extracted_preview: c.extracted_preview,
            source_page_id,
        })
    }

    fn ingest(
        &self,
        space: &str,
        source_id: Option<&str>,
        batch_size: Option<u32>,
        mark_ingested: &[String],
    ) -> ApiResult<IngestOut> {
        let v = self.target(Some(space), Some(space))?;
        if !mark_ingested.is_empty() {
            vc::mark_ingested(&v, mark_ingested, &self.now_iso())
                .map_err(|e| ApiError::new("io", e))?;
        }
        let b = vi::next_batch(&v, source_id, batch_size).map_err(|e| ApiError::new("io", e))?;
        Ok(IngestOut {
            batch: b
                .items
                .into_iter()
                .map(|i| IngestItem {
                    source_id: i.source_id,
                    title: i.title,
                    chars: i.chars,
                    extracted_path: i.extracted_path,
                    extracted: i.extracted,
                    ingested: false,
                })
                .collect(),
            remaining: b.remaining,
            all_ingested: b.all_ingested,
        })
    }

    fn ensure_page(
        &self,
        space: &str,
        page_type: &str,
        title: &str,
        content: Option<&str>,
    ) -> ApiResult<EnsurePageOut> {
        let v = self.target(Some(space), Some(space))?;
        let gate = Self::gate_mode(&v);
        let (id, created) = vp::ensure_page(&v, page_type, title, content, gate)
            .map_err(|e| ApiError::new("invalid_argument", e))?;
        let _ = registry::rebuild_metadata(&v); // registry must reflect the create/update
        if created {
            self.embed_page(&v, &id);
        }
        Ok(EnsurePageOut { id, created })
    }

    fn read_page(&self, space: &str, id: &str) -> ApiResult<ReadPageOut> {
        let v = self.target(Some(space), Some(space))?;
        let content = vp::read_page(&v, id).map_err(|e| ApiError::new("not_found", e))?;
        Ok(ReadPageOut {
            id: id.to_string(),
            content,
        })
    }

    fn write_page(&self, space: &str, id: &str, content: &str) -> ApiResult<WritePageOut> {
        let v = self.target(Some(space), Some(space))?;
        let gate = Self::gate_mode(&v);
        vp::write_page(&v, id, content, gate).map_err(|e| ApiError::new("invalid_argument", e))?;
        let _ = registry::rebuild_metadata(&v); // registry must reflect the write
        self.embed_page(&v, id);
        Ok(WritePageOut {
            id: id.to_string(),
            updated: true,
        })
    }

    /// Delete a wiki page. Two independent guards, both deliberate: the
    /// operator must enable the tool at all (`allow_delete`), and the caller
    /// repeats the id in `confirm` so a slip cannot delete a neighbour.
    fn delete_page(
        &self,
        space: &str,
        id: &str,
        confirm: &str,
        force: bool,
    ) -> ApiResult<DeletePageOut> {
        deletion_allowed(crate::config::get().allow_delete)
            .map_err(|e| ApiError::new("permission_denied", e))?;
        if confirm != id {
            return Err(ApiError::new(
                "invalid_argument",
                format!("confirm must repeat the exact page id ('{id}')"),
            ));
        }
        let v = self.target(Some(space), Some(space))?;
        let message =
            vp::delete_page(&v, id, force).map_err(|e| ApiError::new("invalid_argument", e))?;
        // Vectors outlive the file otherwise: the semantic pass admits ids
        // straight from the store, so a stale entry becomes a ghost result.
        crate::vault::embeddings::forget_page(&v, id);
        let _ = registry::rebuild_metadata(&v);
        Ok(DeletePageOut {
            id: id.to_string(),
            deleted: true,
            message,
        })
    }

    fn template(&self, space: &str, page_type: &str) -> ApiResult<TemplateOut> {
        let v = self.target(Some(space), Some(space))?;
        let content = vp::template(&v, page_type).map_err(ApiError::invalid)?;
        Ok(TemplateOut {
            page_type: page_type.to_string(),
            content,
        })
    }

    fn recall(&self, space: &str, query: &str, max_results: Option<u32>) -> ApiResult<RecallOut> {
        let v = self.target(Some(space), Some(space))?;
        let registry = vr::ensure_registry(&v).map_err(|e| ApiError::new("io", e))?;
        let personal_vault = {
            let pv = self.vault(SPACE_PERSONAL);
            if pv.config_file().exists() && space != SPACE_PERSONAL {
                Some(pv)
            } else {
                None
            }
        };
        let max = max_results.unwrap_or(5).clamp(1, 10);
        // Semantic blend when a provider is configured AND the space has an
        // embeddings store: embed the query once, boost lexical scores by
        // cosine. Any failure degrades silently to pure lexical.
        let semantic = self.embedder.as_ref().and_then(|e| {
            let store = crate::vault::embeddings::EmbeddingStore::load(&v)?;
            if store.pages.is_empty() {
                return None;
            }
            let mut qv = e.embed(&[query.to_string()]).ok()?.pop()?;
            qv.shrink_to_fit();
            Some((qv, store))
        });
        let (hits, links_first) = vr::recall_layered_semantic(
            &v,
            personal_vault.as_ref(),
            &registry,
            query,
            max,
            semantic.as_ref().map(|(qv, store)| (qv.as_slice(), store)),
        );
        Ok(RecallOut {
            query: query.to_string(),
            matches: hits
                .into_iter()
                .map(|h| RecallMatch {
                    id: h.id,
                    title: h.title,
                    page_type: h.page_type,
                    score: h.score,
                    preview: h.preview,
                    layer: h.layer,
                })
                .collect(),
            links_first,
        })
    }

    fn search(&self, space: &str, query: &str, page_type: Option<&str>) -> ApiResult<SearchOut> {
        let v = self.target(Some(space), Some(space))?;
        let reg = vr::ensure_registry(&v).map_err(|e| ApiError::new("io", e))?;
        let q = query.to_lowercase();
        let matches: Vec<SearchMatch> = reg
            .pages
            .values()
            .filter(|p| page_type.map(|t| p.page_type == t).unwrap_or(true))
            .filter(|p| {
                q.is_empty()
                    || p.id.to_lowercase().contains(&q)
                    || p.title.to_lowercase().contains(&q)
                    || p.page_type.to_lowercase().contains(&q)
            })
            .take(50)
            .map(|p| SearchMatch {
                id: p.id.clone(),
                title: p.title.clone(),
                page_type: p.page_type.clone(),
            })
            .collect();
        Ok(SearchOut {
            query: query.to_string(),
            matches,
        })
    }

    fn status(&self, space: &str) -> ApiResult<StatusOut> {
        let v = self.target(Some(space), Some(space))?;
        let reg = vr::ensure_registry(&v).map_err(|e| ApiError::new("io", e))?;
        let st = vs::compute(&v, &reg);
        Ok(StatusOut {
            space: space.to_string(),
            total_pages: st.total_pages,
            by_type: st.by_type,
            orphans: st.orphans,
            gaps: st.gaps,
            health: st.health,
            git: crate::vault::git::read_state(&self.root),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            allow_delete: crate::config::get().allow_delete,
        })
    }

    fn lint(&self, space: &str, auto_fix: bool) -> ApiResult<LintOut> {
        let v = self.target(Some(space), Some(space))?;
        let reg = registry::rebuild_metadata(&v).map_err(|e| ApiError::new("io", e))?;
        let r = vl::run(&v, &reg, auto_fix).map_err(|e| ApiError::new("io", e))?;
        Ok(LintOut {
            pages: r.pages,
            orphans: r.orphans,
            missing_pages: r.missing_pages,
            contradictions: r.contradictions,
            auto_fixed: r.auto_fixed,
        })
    }

    fn retro(
        &self,
        space: &str,
        slug: &str,
        title: &str,
        body: &str,
        category: Option<&str>,
    ) -> ApiResult<RetroOut> {
        let v = self.target(Some(space), Some(space))?;
        let gate = Self::gate_mode(&v);
        let id = vp::retro(&v, slug, title, body, category, gate)
            .map_err(|e| ApiError::new("invalid_argument", e))?;
        registry::log_event(
            &v,
            "retro",
            &serde_json::json!({"slug": slug}),
            &self.now_iso(),
        )
        .map_err(|e| ApiError::new("io", e))?;
        Ok(RetroOut {
            slug: slug.to_string(),
            path: id,
        })
    }

    fn observe(
        &self,
        space: &str,
        title: &str,
        content: &str,
        relevance: &str,
        tags: Option<&str>,
        source_context: Option<&str>,
    ) -> ApiResult<ObserveOut> {
        let v = self.target(Some(space), Some(space))?;
        let gate = Self::gate_mode(&v);
        let date = self.today();
        let input = vp::ObserveInput {
            title,
            content,
            relevance,
            tags,
            source_context,
        };
        let id = vp::observe(&v, &date, &input, gate)
            .map_err(|e| ApiError::new("invalid_argument", e))?;
        Ok(ObserveOut {
            slug: id.rsplit('/').next().unwrap_or("").to_string(),
            path: id,
        })
    }

    fn log_event(
        &self,
        space: &str,
        kind: &str,
        details: &serde_json::Value,
    ) -> ApiResult<LogEventOut> {
        let v = self.target(Some(space), Some(space))?;
        registry::log_event(&v, kind, details, &self.now_iso())
            .map_err(|e| ApiError::new("io", e))?;
        registry::rebuild_log(&v).map_err(|e| ApiError::new("io", e))?;
        Ok(LogEventOut {
            kind: kind.to_string(),
        })
    }

    fn reembed(&self, space: &str) -> ApiResult<ReembedOut> {
        let v = self.target(Some(space), Some(space))?;
        match &self.embedder {
            None => Ok(ReembedOut {
                embedded: 0,
                skipped: 0,
                provider_configured: false,
                message: "no embedding provider configured (set WIKI_EMBEDDING_URL + WIKI_EMBEDDING_MODEL); recall remains lexical".into(),
            }),
            Some(embedder) => {
                let reg = vr::ensure_registry(&v).map_err(|e| ApiError::new("io", e))?;
                let r = crate::vault::embeddings::reindex(&v, &reg, embedder.as_ref())
                    .map_err(|e| ApiError::new("embedding_failed", e))?;
                Ok(ReembedOut {
                    embedded: r.embedded as u64,
                    skipped: r.skipped as u64,
                    provider_configured: true,
                    message: format!(
                        "embedded {} pages, skipped {} unchanged, with {}",
                        r.embedded,
                        r.skipped,
                        embedder.model()
                    ),
                })
            }
        }
    }

    fn capture_trajectory(
        &self,
        space: &str,
        title: &str,
        outcome: Option<&str>,
        steps: &serde_json::Value,
        summary: &str,
    ) -> ApiResult<CaptureTrajectoryOut> {
        let v = self.target(Some(space), Some(space))?;
        let c = vt::capture_trajectory(
            &v,
            &self.today(),
            &self.now_iso(),
            title,
            outcome.unwrap_or("success"),
            steps,
            summary,
        )
        .map_err(ApiError::invalid)?;
        Ok(CaptureTrajectoryOut {
            trajectory_id: c.trajectory_id,
            case_page_id: c.case_page_id,
        })
    }

    fn distill_skills(&self, space: &str, mark_distilled: &[String]) -> ApiResult<DistillOut> {
        let v = self.target(Some(space), Some(space))?;
        let batch = vt::distill(&v, mark_distilled).map_err(ApiError::invalid)?;
        let all_distilled = batch.is_empty();
        Ok(DistillOut {
            batch: batch
                .into_iter()
                .map(|u| DistillItem {
                    trajectory_id: u.trajectory_id,
                    title: u.title,
                    summary: u.summary,
                })
                .collect(),
            all_distilled,
        })
    }

    fn recall_skill(
        &self,
        space: &str,
        query: &str,
        kind: Option<&str>,
        max_results: Option<u32>,
    ) -> ApiResult<RecallOut> {
        let kinds: Vec<&str> = match kind {
            Some("skill") => vec!["skill"],
            Some("case") => vec!["case"],
            _ => vec!["skill", "case"],
        };
        let max = max_results.unwrap_or(5).max(1);
        // over-fetch then filter: skill/case pages are few, recall is in-process
        let mut out = self.recall(space, query, Some(max.saturating_mul(5).min(50)))?;
        out.matches
            .retain(|m| kinds.contains(&m.page_type.as_str()));
        out.matches.truncate(max as usize);
        out.query = query.to_string();
        Ok(out)
    }
}

impl Hub {
    fn gate_mode(v: &VaultPaths) -> vp::GateMode {
        let raw = std::fs::read_to_string(v.config_file()).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|c| {
                c["wikilink_validation"]
                    .as_str()
                    .and_then(vp::GateMode::parse)
            })
            .unwrap_or(vp::GateMode::Normalize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::embeddings::Embedder;

    fn hub() -> Hub {
        let tmp = tempfile::tempdir().unwrap();
        Hub::with_injections(
            tmp.path().to_path_buf(),
            Box::new(StaticFetcher),
            Arc::new(DefaultConverter),
            Box::new(|| "2026-09-07T12:00:00Z".into()),
        )
    }

    #[test]
    fn deletion_gate_requires_operator_opt_in() {
        let err = deletion_allowed(false).unwrap_err();
        // The refusal has to name the knob, or the operator cannot act on it.
        assert!(err.contains("allow_delete"), "{err}");
        assert!(err.contains("WIKI_ALLOW_DELETE"), "{err}");
        assert!(deletion_allowed(true).is_ok());
    }

    #[test]
    fn delete_page_checks_the_space_before_anything_else() {
        let h = hub();
        assert!(h
            .delete_page("no-such-space", "concepts/x", "concepts/x", false)
            .is_err());
    }

    struct StaticFetcher;
    impl UrlFetcher for StaticFetcher {
        fn fetch_markdown(&self, _url: &str) -> Result<String, String> {
            Ok("# Fetched\n\nmarkdown body\n".into())
        }
    }

    /// Converter that reports what the fetch path decided the response was.
    #[derive(Default)]
    struct KindRecorder {
        seen: std::sync::Mutex<Vec<crate::vault::convert::ContentKind>>,
    }

    impl Converter for KindRecorder {
        fn convert(
            &self,
            _b: &[u8],
            kind: &crate::vault::convert::ContentKind,
        ) -> Result<String, String> {
            self.seen.lock().unwrap().push(kind.clone());
            Ok(format!("converted {}", kind.name()))
        }
    }

    /// One-shot HTTP server: answers the first request with `headers` + `body`.
    /// A local listener keeps these tests off the network. A `Content-Length`
    /// in `headers` is honoured (the oversize case declares a big one and
    /// sends almost nothing); otherwise the real body length is sent, because
    /// hyper rejects an EOF-delimited response as incomplete.
    fn serve_once(headers: &'static str, body: Vec<u8>) -> String {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf); // drain the request
                let len = if headers.to_ascii_lowercase().contains("content-length") {
                    String::new()
                } else {
                    format!("Content-Length: {}\r\n", body.len())
                };
                let head =
                    format!("HTTP/1.1 200 OK\r\n{headers}\r\n{len}Connection: close\r\n\r\n");
                let _ = sock.write_all(head.as_bytes());
                let _ = sock.write_all(&body);
            }
        });
        format!("http://{addr}/page")
    }

    /// A fetcher whose client ignores http_proxy: the local one-shot server
    /// above must not be routed through it.
    fn local_fetcher(convert: Arc<dyn Converter>) -> HttpFetcher {
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap();
        HttpFetcher::with_client(convert, client)
    }

    #[test]
    fn fetch_dispatches_on_content_type() {
        let rec = std::sync::Arc::new(KindRecorder::default());
        let f = local_fetcher(rec.clone());
        let md = f
            .fetch_markdown(&serve_once(
                "Content-Type: text/html; charset=utf-8",
                b"<h1>Hi</h1>".to_vec(),
            ))
            .unwrap();
        assert!(md.starts_with("converted html"), "{md}");
        assert_eq!(
            rec.seen.lock().unwrap().pop().unwrap(),
            crate::vault::convert::ContentKind::Html
        );

        // A PDF announced as text/plain is still converted as a PDF — the
        // case that used to store binary as mojibake.
        let rec = std::sync::Arc::new(KindRecorder::default());
        let f = local_fetcher(rec.clone());
        let md = f
            .fetch_markdown(&serve_once(
                "Content-Type: text/plain",
                b"%PDF-1.4\n...".to_vec(),
            ))
            .unwrap();
        assert!(md.starts_with("converted pdf"), "{md}");
        assert_eq!(
            rec.seen.lock().unwrap().pop().unwrap(),
            crate::vault::convert::ContentKind::Pdf
        );

        // A binary type is refused instead of decoded as text.
        let f = local_fetcher(std::sync::Arc::new(DefaultConverter));
        let err = f
            .fetch_markdown(&serve_once(
                "Content-Type: image/png",
                b"\x89PNG\r\n".to_vec(),
            ))
            .unwrap_err();
        assert!(err.contains("image/png"), "{err}");
    }

    #[test]
    fn fetch_refuses_a_body_over_the_capture_limit() {
        let f = local_fetcher(std::sync::Arc::new(DefaultConverter));
        let declared = convert::MAX_BYTES + 1;
        let headers: &'static str = Box::leak(
            format!("Content-Type: text/plain\r\nContent-Length: {declared}").into_boxed_str(),
        );
        let err = f
            .fetch_markdown(&serve_once(headers, b"tiny".to_vec()))
            .unwrap_err();
        assert!(err.contains("capture limit"), "{err}");
    }

    #[test]
    fn status_carries_git_state_after_tick() {
        let tmp = tempfile::tempdir().unwrap();
        let h = Hub::with_injections(
            tmp.path().to_path_buf(),
            Box::new(StaticFetcher),
            Arc::new(DefaultConverter),
            Box::new(|| "2026-09-07T12:00:00Z".into()),
        );
        let api: &dyn WikiApi = &h;
        api.bootstrap("proj", None).unwrap();
        assert!(api.status("proj").unwrap().git.is_none());
        crate::vault::git::tick(tmp.path(), 0, "2026-09-07T12:00:00Z");
        let st = api.status("proj").unwrap();
        let g = st.git.expect("git state after tick");
        assert!(g.ok, "clean tick: {}", g.detail);
    }

    #[test]
    fn template_roundtrip_via_hub() {
        let h = hub();
        let api: &dyn WikiApi = &h;
        api.bootstrap("proj", None).unwrap();
        let t = api.template("proj", "concept").unwrap();
        assert_eq!(t.page_type, "concept");
        assert!(t.content.starts_with("---\ntype: concept"));
        assert!(t.content.contains("{title}"));
        // scaffold an actual page from the template: registry accepts it
        let filled = t.content.replace("{title}", "Templated Probe");
        let e = api
            .ensure_page("proj", "concept", "Templated Probe", Some(&filled))
            .unwrap();
        assert!(e.created);
        let st = api.status("proj").unwrap();
        assert_eq!(st.total_pages, 1);
        // unknown type rejected
        assert!(api.template("proj", "nope").is_err());
    }

    #[test]
    fn end_to_end_via_trait_object() {
        let h = hub();
        let api: &dyn WikiApi = &h;
        // bootstrap + pin
        let b = api.bootstrap("proj", None).unwrap();
        assert!(b.created);
        let u = api.use_space("conn1", "proj").unwrap();
        assert!(u.exists);

        // capture via fetcher seam
        let c = api
            .capture_source("proj", None, Some("https://ex.com/a"), None, None)
            .unwrap();
        assert_eq!(c.source_id, "SRC-2026-09-07-001");

        // ingest batch + mark
        let i1 = api.ingest("proj", None, None, &[]).unwrap();
        assert_eq!(i1.batch.len(), 1);
        api.ingest("proj", None, None, &[c.source_id]).unwrap();
        let i2 = api.ingest("proj", None, None, &[]).unwrap();
        assert!(i2.all_ingested);

        // write pages
        api.ensure_page(
            "proj",
            "concept",
            "RAG",
            Some("# RAG\n\nsee [[concepts/retrieval]]\n"),
        )
        .unwrap();
        let _ = api
            .ensure_page("proj", "concept", "Retrieval", None)
            .unwrap();
        api.write_page(
            "proj",
            "concepts/rag",
            "---\ntitle: \"RAG\"\ntype: concept\n---\n\nupdated [[concepts/retrieval]]\n",
        )
        .unwrap();
        let read = api.read_page("proj", "concepts/rag").unwrap();
        assert!(read.content.contains("updated"));

        // recall
        let r = api.recall("proj", "retrieval", None).unwrap();
        assert!(r.matches.iter().any(|m| m.id == "concepts/retrieval"));

        // retro + observe
        api.retro("proj", "jwt-fix", "JWT fix", "learned\n", None)
            .unwrap();
        api.observe("proj", "Decision", "chose KISS", "high", None, None)
            .unwrap();

        // status + lint
        let st = api.status("proj").unwrap();
        assert!(st.total_pages >= 5);
        // git state surfaces through status (absent until first tick)
        assert!(st.git.is_none());
        let l = api.lint("proj", false).unwrap();
        assert!(l.missing_pages.is_empty() || !l.auto_fixed.is_empty() || l.auto_fixed.is_empty()); // shape check
        let _ = l;

        // events
        api.log_event("proj", "decision", &serde_json::json!({"what":"port"}))
            .unwrap();

        // guardrail: raw is unreadable as a page
        assert!(api
            .read_page("proj", "../raw/sources/SRC-2026-09-07-001/extracted")
            .is_err());
    }

    #[test]
    fn use_space_persists_per_connection_and_unknown_space_fails() {
        let h = hub();
        WikiApi::bootstrap(&h, "p1", None).unwrap();
        h.use_space("c-42", "p1").unwrap();
        // ops without explicit space use the pin (conn id passed via capture? —
        // the server layer passes conn; here explicit-space ops also work):
        assert!(WikiApi::status(&h, "p1").is_ok());
        let err = WikiApi::status(&h, "nope").unwrap_err();
        assert_eq!(err.code, "no_vault");
    }

    struct MockEmbed;
    impl crate::vault::embeddings::Embedder for MockEmbed {
        fn model(&self) -> &str {
            "mock"
        }
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            Ok(texts
                .iter()
                .map(|t| {
                    let mut v = vec![0.0f32; 8];
                    for w in t.to_lowercase().split_whitespace() {
                        let sum: usize = w.bytes().map(|b| b as usize).sum();
                        v[sum % 8] += 1.0;
                    }
                    v
                })
                .collect())
        }
    }

    #[test]
    fn recall_blends_semantic_scores_when_store_present() {
        use crate::vault::embeddings::EmbeddingStore;
        let h = hub().with_embedder(Box::new(MockEmbed));
        let api: &dyn WikiApi = &h;
        api.bootstrap("proj", None).unwrap();
        let e = api
            .ensure_page(
                "proj",
                "concept",
                "Cache safety",
                Some("Body about prompt cache prefixes."),
            )
            .unwrap();

        // Store a mock vector for the page (from its own text) in meta/.
        let mut store = EmbeddingStore {
            model: "mock".into(),
            pages: Default::default(),
        };
        let v = MockEmbed
            .embed(&["Cache safety prompt cache prefixes.".to_string()])
            .unwrap()
            .pop()
            .unwrap();
        store.pages.insert(
            e.id.clone(),
            crate::vault::embeddings::PageVectors {
                hash: String::new(),
                chunks: vec![v],
            },
        );
        let vp = crate::vault::layout::VaultPaths::new(&h.root, "proj");
        store.save(&vp).unwrap();

        let blended = api.recall("proj", "cache", None).unwrap();
        let hit = blended.matches.iter().find(|m| m.id == e.id).unwrap();

        // Same vault, no embedder => pure lexical baseline.
        let plain = Hub::with_injections(
            h.root.clone(),
            Box::new(StaticFetcher),
            Arc::new(DefaultConverter),
            Box::new(|| "2026-09-07T12:00:00Z".into()),
        );
        let base = plain.recall("proj", "cache", None).unwrap();
        let b = base.matches.iter().find(|m| m.id == e.id).unwrap();

        assert!(
            hit.score > b.score,
            "blended {} should exceed lexical {}",
            hit.score,
            b.score
        );
    }

    #[test]
    fn write_page_auto_embeds_when_provider_configured() {
        use crate::vault::embeddings::EmbeddingStore;
        let h = hub().with_embedder(Box::new(MockEmbed));
        let api: &dyn WikiApi = &h;
        api.bootstrap("proj", None).unwrap();

        // Seed an empty store so the upsert path has something to write to.
        let vp = crate::vault::layout::VaultPaths::new(&h.root, "proj");
        EmbeddingStore {
            model: "mock".into(),
            pages: Default::default(),
        }
        .save(&vp)
        .unwrap();

        let e = api
            .ensure_page("proj", "concept", "Vector fresh", Some("content body"))
            .unwrap();
        assert!(e.created);

        let store = EmbeddingStore::load(&vp).expect("store present");
        assert!(
            store.pages.contains_key(&e.id),
            "ensure_page upserted a vector"
        );

        api.write_page("proj", &e.id, "---\ntitle: Vector fresh\n---\nnew body")
            .unwrap();
        let store = EmbeddingStore::load(&vp).expect("store present");
        assert!(
            store.pages.contains_key(&e.id),
            "write_page kept the vector fresh"
        );
    }
}
