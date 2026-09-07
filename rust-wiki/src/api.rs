//! The seam: `WikiApi` is the ONLY surface the MCP layer (server.rs) knows.
//! Value types here are the wire DTOs (tool args/returns). The Hub is the
//! sole production impl; tests may stub the trait.

use serde::{Deserialize, Serialize};

pub type ApiResult<T> = Result<T, ApiError>;

/// Errors across the API. `code` is stable and surfaced to MCP clients.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn no_vault(space: &str) -> Self {
        Self::new("no_vault", format!("no vault for space '{space}' — call wiki_bootstrap first"))
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::new("invalid_argument", msg)
    }
    pub fn guarded(msg: impl Into<String>) -> Self {
        Self::new("guardrail", msg)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("serialize", e.to_string())
    }
}



// ---------- DTOs ----------

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct BootstrapOut {
    pub created: bool,
    pub space: String,
    pub root: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct UseSpaceOut {
    pub space: String,
    pub exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CaptureOut {
    pub source_id: String,
    pub extracted_preview: String,
    pub source_page_id: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct IngestItem {
    pub source_id: String,
    pub title: String,
    pub chars: usize,
    /// Vault-relative path of extracted.md for reading.
    pub extracted_path: String,
    pub ingested: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct IngestOut {
    pub batch: Vec<IngestItem>,
    pub remaining: u64,
    pub all_ingested: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct EnsurePageOut {
    pub id: String,
    pub created: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ReadPageOut {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct WritePageOut {
    pub id: String,
    pub updated: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RecallMatch {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: String,
    pub score: f64,
    pub preview: String,
    /// "personal" when hit came from the personal layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct RecallOut {
    pub query: String,
    pub matches: Vec<RecallMatch>,
    /// true when the vault exceeded the links-first threshold (previews trimmed).
    pub links_first: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SearchMatch {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SearchOut {
    pub query: String,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct StatusOut {
    pub space: String,
    pub total_pages: u64,
    pub by_type: std::collections::BTreeMap<String, u64>,
    pub orphans: u64,
    pub gaps: u64,
    /// "empty" | "good" | "warning"
    pub health: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct LintOut {
    pub pages: u64,
    pub orphans: Vec<String>,
    pub missing_pages: Vec<String>,
    pub contradictions: Vec<String>,
    pub auto_fixed: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct RetroOut {
    pub slug: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ObserveOut {
    pub slug: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct LogEventOut {
    pub kind: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OkOut {
    pub ok: bool,
}

/// The seam trait. All ops are space-aware; `space` explicitly provided
/// overrides the connection default held by the impl.
pub trait WikiApi: Send + Sync {
    fn bootstrap(&self, space: &str, mode: Option<&str>) -> ApiResult<BootstrapOut>;
    /// Pin `space` as the default for connection `conn`. Returns summary.
    fn use_space(&self, conn: &str, space: &str) -> ApiResult<UseSpaceOut>;
    fn capture_source(&self, space: &str, text: Option<&str>, url: Option<&str>, file_path: Option<&str>, title: Option<&str>) -> ApiResult<CaptureOut>;
    /// `mark_ingested`: source ids to flip into ingested state (post-synthesis).
    fn ingest(&self, space: &str, source_id: Option<&str>, batch_size: Option<u32>, mark_ingested: &[String]) -> ApiResult<IngestOut>;
    fn ensure_page(&self, space: &str, page_type: &str, title: &str, content: Option<&str>) -> ApiResult<EnsurePageOut>;
    fn read_page(&self, space: &str, id: &str) -> ApiResult<ReadPageOut>;
    fn write_page(&self, space: &str, id: &str, content: &str) -> ApiResult<WritePageOut>;
    fn recall(&self, space: &str, query: &str, max_results: Option<u32>) -> ApiResult<RecallOut>;
    fn search(&self, space: &str, query: &str, page_type: Option<&str>) -> ApiResult<SearchOut>;
    fn status(&self, space: &str) -> ApiResult<StatusOut>;
    fn lint(&self, space: &str, auto_fix: bool) -> ApiResult<LintOut>;
    fn retro(&self, space: &str, slug: &str, title: &str, body: &str, category: Option<&str>) -> ApiResult<RetroOut>;
    fn observe(&self, space: &str, title: &str, content: &str, relevance: &str, tags: Option<&str>, source_context: Option<&str>) -> ApiResult<ObserveOut>;
    fn log_event(&self, space: &str, kind: &str, details: &serde_json::Value) -> ApiResult<LogEventOut>;
}
