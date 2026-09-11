//! Central server configuration: one labeled key per knob, single precedence.
//!
//! Precedence per key: **environment → `<exe_dir>/config.toml` → default**.
//! Every key is settable in the file; every key is overridable by env.
//! Access via `get()` (process-wide, loaded once) — vault modules never
//! touch env or the file directly.

use std::path::PathBuf;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

/// All server knobs, with defaults. Field docs are the labels.
#[derive(Debug, Clone)]
pub struct Config {
    /// Port for `serve` HTTP mode.
    pub port: u16,
    /// Vault root directory (spaces live underneath).
    pub vault_root: Option<PathBuf>,
    /// Maintenance scheduler interval (secs). 0 disables.
    pub cron_interval_secs: u64,
    /// Embedding endpoint (OpenAI-compatible). Unset = semantic recall off.
    pub embedding_url: Option<String>,
    /// Embedding model id.
    pub embedding_model: String,
    /// Embedding bearer token (server-side only, never logged).
    pub embedding_token: Option<String>,
    /// Recall links-first gate: vaults above this page count return links.
    /// 0 = always links-first.
    pub recall_links_first_threshold: u64,
    /// Semantic fusion: minimum best-chunk cosine for a page with no lexical
    /// match to be admitted as a candidate. Tuning targets: lower = recall,
    /// higher = precision (fewer junk admissions).
    pub recall_semantic_min_cosine: f32,
    /// Semantic fusion: lexical points a perfect (cosine 1) semantic match is
    /// worth at full weight (with the 0.5 weight) so it reaches the top-N
    /// without outranking a real title match on its own.
    pub recall_semantic_scale: f64,
    /// Semantic fusion: weight of the semantic term when blending with the
    /// lexical score.
    pub recall_semantic_weight: f64,
    /// Git auto-commit tick interval (secs). 0 disables git backing.
    pub git_interval_secs: u64,
    /// Git idle threshold (secs) before a dirty vault commits.
    pub git_idle_secs: u64,
    /// Allow `wiki_delete_page` to remove pages. **Off by default**: deletion
    /// is the only irreversible tool, so it needs an operator opt-in on top of
    /// the caller repeating the page id in `confirm`.
    pub allow_delete: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 8484,
            vault_root: None,
            cron_interval_secs: 3600,
            embedding_url: None,
            embedding_model: "text-embedding-3-small".into(),
            embedding_token: None,
            recall_links_first_threshold: 50,
            recall_semantic_min_cosine: 0.6,
            recall_semantic_scale: 6.0,
            recall_semantic_weight: 0.5,
            git_interval_secs: 60,
            git_idle_secs: 300,
            allow_delete: false,
        }
    }
}

/// File shape: flat TOML, snake_case keys mirroring the env names.
#[derive(Debug, serde::Deserialize, Default)]
struct FileConfig {
    port: Option<u16>,
    vault_root: Option<String>,
    cron_interval_secs: Option<u64>,
    embedding_url: Option<String>,
    embedding_model: Option<String>,
    embedding_token: Option<String>,
    recall_links_first_threshold: Option<u64>,
    recall_semantic_min_cosine: Option<f32>,
    recall_semantic_scale: Option<f64>,
    recall_semantic_weight: Option<f64>,
    git_interval_secs: Option<u64>,
    git_idle_secs: Option<u64>,
    allow_delete: Option<bool>,
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok()?.trim().parse().ok()
}

fn env_f64(name: &str) -> Option<f64> {
    std::env::var(name).ok()?.trim().parse().ok()
}

fn env_str(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

fn env_bool(name: &str) -> Option<bool> {
    parse_bool(&env_str(name)?)
}

/// Accepted spellings for boolean env keys, kept pure so it is testable
/// without mutating the process environment (which races across tests).
fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

impl Config {
    fn from_file(file: &FileConfig) -> Self {
        let d = Self::default();
        Self {
            port: file.port.unwrap_or(d.port),
            vault_root: file.vault_root.clone().map(PathBuf::from).or(d.vault_root),
            cron_interval_secs: file.cron_interval_secs.unwrap_or(d.cron_interval_secs),
            embedding_url: file.embedding_url.clone().or(d.embedding_url),
            embedding_model: file.embedding_model.clone().unwrap_or(d.embedding_model),
            embedding_token: file.embedding_token.clone().or(d.embedding_token),
            recall_links_first_threshold: file
                .recall_links_first_threshold
                .unwrap_or(d.recall_links_first_threshold),
            recall_semantic_min_cosine: file
                .recall_semantic_min_cosine
                .unwrap_or(d.recall_semantic_min_cosine),
            recall_semantic_scale: file
                .recall_semantic_scale
                .unwrap_or(d.recall_semantic_scale),
            recall_semantic_weight: file
                .recall_semantic_weight
                .unwrap_or(d.recall_semantic_weight),
            git_interval_secs: file.git_interval_secs.unwrap_or(d.git_interval_secs),
            git_idle_secs: file.git_idle_secs.unwrap_or(d.git_idle_secs),
            allow_delete: file.allow_delete.unwrap_or(d.allow_delete),
        }
    }

    fn apply_env(&mut self) {
        if let Ok(p) = std::env::var("WIKI_PORT") {
            if let Ok(n) = p.parse() {
                self.port = n;
            }
        }
        if let Some(v) = env_str("WIKI_VAULT_ROOT") {
            self.vault_root = Some(PathBuf::from(v));
        }
        if let Some(v) = env_u64("WIKI_CRON_INTERVAL_SECS") {
            self.cron_interval_secs = v;
        }
        if let Some(v) = env_str("WIKI_EMBEDDING_URL") {
            self.embedding_url = Some(v);
        }
        if let Some(v) = env_str("WIKI_EMBEDDING_MODEL") {
            self.embedding_model = v;
        }
        if let Some(v) = env_str("WIKI_EMBEDDING_TOKEN") {
            self.embedding_token = Some(v);
        }
        if let Some(v) = env_u64("WIKI_RECALL_LINKS_FIRST_THRESHOLD") {
            self.recall_links_first_threshold = v;
        }
        if let Some(v) = env_f32("WIKI_RECALL_SEMANTIC_MIN_COSINE") {
            self.recall_semantic_min_cosine = v;
        }
        if let Some(v) = env_f64("WIKI_RECALL_SEMANTIC_SCALE") {
            self.recall_semantic_scale = v;
        }
        if let Some(v) = env_f64("WIKI_RECALL_SEMANTIC_WEIGHT") {
            self.recall_semantic_weight = v;
        }
        if let Some(v) = env_u64("WIKI_GIT_INTERVAL_SECS") {
            self.git_interval_secs = v;
        }
        if let Some(v) = env_u64("WIKI_GIT_IDLE_SECS") {
            self.git_idle_secs = v;
        }
        if let Some(v) = env_bool("WIKI_ALLOW_DELETE") {
            self.allow_delete = v;
        }
    }

    /// Load from an explicit file path (tests + tooling).
    pub fn load_from_file(path: Option<std::path::PathBuf>) -> Self {
        let mut cfg = path
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|raw| toml::from_str::<FileConfig>(&raw).ok())
            .map(|f| Self::from_file(&f))
            .unwrap_or_default();
        cfg.apply_env();
        cfg
    }

    /// Process-wide load: `<exe_dir>/config.toml`, then env.
    pub fn load() -> Self {
        let path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("config.toml")));
        Self::load_from_file(path)
    }
}

/// Process-wide config, loaded once. Tests hitting this before `main`
/// get file-or-defaults (no config.toml next to the test binary).
pub fn get() -> &'static Config {
    CONFIG.get_or_init(Config::load)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn defaults_without_file() {
        let c = Config::load_from_file(None);
        // env in this process is clean for these keys (CI invariant)
        let _ = &c;
        assert_eq!(Config::default().port, 8484);
        assert_eq!(Config::default().recall_links_first_threshold, 50);
    }

    #[test]
    fn file_keys_parse_and_label() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            f,
            concat!(
                "port = 9999\n",
                "vault_root = \"/data/w\"\n",
                "cron_interval_secs = 60\n",
                "embedding_url = \"https://e/v1/embeddings\"\n",
                "embedding_model = \"m\"\n",
                "embedding_token = \"t\"\n",
                "recall_links_first_threshold = 10\n",
                "git_interval_secs = 30\n",
                "git_idle_secs = 120\n",
                "allow_delete = true\n",
            )
        )
        .unwrap();
        // isolate from ambient env for file-value assertions
        for k in [
            "WIKI_PORT",
            "WIKI_VAULT_ROOT",
            "WIKI_CRON_INTERVAL_SECS",
            "WIKI_EMBEDDING_URL",
            "WIKI_EMBEDDING_MODEL",
            "WIKI_EMBEDDING_TOKEN",
            "WIKI_RECALL_LINKS_FIRST_THRESHOLD",
            "WIKI_GIT_INTERVAL_SECS",
            "WIKI_GIT_IDLE_SECS",
            "WIKI_ALLOW_DELETE",
        ] {
            unsafe { std::env::remove_var(k) };
        }
        let c = Config::load_from_file(Some(f.path().to_path_buf()));
        assert_eq!(c.port, 9999);
        assert_eq!(c.vault_root, Some(PathBuf::from("/data/w")));
        assert_eq!(c.cron_interval_secs, 60);
        assert_eq!(c.embedding_url.as_deref(), Some("https://e/v1/embeddings"));
        assert_eq!(c.embedding_model, "m");
        assert_eq!(c.recall_links_first_threshold, 10);
        assert_eq!(c.git_interval_secs, 30);
        assert_eq!(c.git_idle_secs, 120);
        assert!(c.allow_delete);
    }

    #[test]
    fn allow_delete_defaults_off_and_file_can_enable_it() {
        // Off by default: a server whose operator never asked for deletion
        // must not expose it (the tool itself refuses with permission_denied).
        assert!(!Config::default().allow_delete);

        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "allow_delete = true\n").unwrap();
        unsafe { std::env::remove_var("WIKI_ALLOW_DELETE") };
        assert!(Config::load_from_file(Some(f.path().to_path_buf())).allow_delete);
    }

    #[test]
    fn bool_env_accepts_common_spellings_and_rejects_junk() {
        // Pure parser: no env mutation, so tests stay race-free.
        for yes in ["1", "true", "TRUE", "yes", "on", " On "] {
            assert_eq!(parse_bool(yes), Some(true), "{yes}");
        }
        for no in ["0", "false", "no", "off"] {
            assert_eq!(parse_bool(no), Some(false), "{no}");
        }
        assert_eq!(parse_bool("maybe"), None);
    }
}
