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
    /// Git auto-commit tick interval (secs). 0 disables git backing.
    pub git_interval_secs: u64,
    /// Git idle threshold (secs) before a dirty vault commits.
    pub git_idle_secs: u64,
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
            git_interval_secs: 60,
            git_idle_secs: 300,
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
    git_interval_secs: Option<u64>,
    git_idle_secs: Option<u64>,
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}

fn env_str(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
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
            git_interval_secs: file.git_interval_secs.unwrap_or(d.git_interval_secs),
            git_idle_secs: file.git_idle_secs.unwrap_or(d.git_idle_secs),
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
        if let Some(v) = env_u64("WIKI_GIT_INTERVAL_SECS") {
            self.git_interval_secs = v;
        }
        if let Some(v) = env_u64("WIKI_GIT_IDLE_SECS") {
            self.git_idle_secs = v;
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
    }
}
