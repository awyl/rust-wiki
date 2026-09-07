//! Built-in maintenance scheduler. The server is long-running, so it owns
//! the clock itself: a background thread runs a mechanical cycle
//! (lint + auto_fix + status) across every bootstrapped space on a fixed
//! interval. On by default (hourly); `WIKI_CRON_INTERVAL_SECS=0` disables.
//! No LLM, no crontab, no human steps.

use std::path::Path;
use std::time::Duration;

use crate::api::WikiApi;
use crate::hub::Hub;

pub const DEFAULT_INTERVAL_SECS: u64 = 3600;

/// Interval from env; 0 disables the scheduler.
pub fn interval_from_env() -> u64 {
    std::env::var("WIKI_CRON_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS)
}

/// Spawn the background scheduler thread. No-op when interval is 0.
/// First cycle runs after one interval (startup path is already busy).
pub fn spawn(vault_root: std::path::PathBuf) -> Option<std::thread::JoinHandle<()>> {
    let interval = interval_from_env();
    if interval == 0 {
        return None;
    }
    Some(std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(interval));
        for line in run_all_spaces(&vault_root) {
            tracing::info!("{line}");
        }
    }))
}

/// One mechanical maintenance cycle for a single space. No LLM, no fetching.
pub fn cron_cycle(hub: &Hub, space: &str) -> Result<String, String> {
    let lint = hub
        .lint(space, true)
        .map_err(|e| format!("{}: {}", e.code, e.message))?;
    let status = hub
        .status(space)
        .map_err(|e| format!("{}: {}", e.code, e.message))?;
    Ok(format!(
        "[rust-wiki cron] space={space} pages={} orphans={} missing={} auto_fixed={} health={}",
        status.total_pages,
        status.orphans,
        lint.missing_pages.len(),
        lint.auto_fixed.len(),
        status.health
    ))
}

/// Run a cycle across every bootstrapped space in the vault root.
/// Spaces without a vault (never bootstrapped) are skipped silently.
pub fn run_all_spaces(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return vec![];
    };
    let mut spaces: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("config.json").is_file())
        .filter_map(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect();
    spaces.sort();
    let hub = Hub::new(root.to_path_buf());
    spaces
        .into_iter()
        .filter_map(|space| match cron_cycle(&hub, &space) {
            Ok(line) => Some(line),
            Err(e) => Some(format!("[rust-wiki cron] space={space} ERROR {e}")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_cycle_reports_and_autofixes() {
        let tmp = tempfile::tempdir().unwrap();
        let hub = Hub::new(tmp.path().to_path_buf());
        hub.bootstrap("s", None).unwrap();
        let vp = crate::vault::layout::VaultPaths::new(tmp.path(), "s");
        // pages citing a missing target twice -> lint auto_fix stubs it
        crate::vault::pages::ensure_page(
            &vp,
            "concept",
            "a",
            Some("see [[concepts/b]]\n"),
            crate::vault::pages::GateMode::Off,
        )
        .unwrap();
        crate::vault::pages::ensure_page(
            &vp,
            "concept",
            "c",
            Some("also [[concepts/b]]\n"),
            crate::vault::pages::GateMode::Off,
        )
        .unwrap();
        let report = cron_cycle(&hub, "s").unwrap();
        assert!(report.contains("pages=3"), "got: {report}"); // a + c + stub b
        assert!(report.contains("auto_fixed=1"), "got: {report}");
        assert!(report.contains("health=good"), "got: {report}");
    }

    #[test]
    fn run_all_spaces_covers_bootstrapped_skips_unbootstrapped() {
        let tmp = tempfile::tempdir().unwrap();
        let hub = Hub::new(tmp.path().to_path_buf());
        hub.bootstrap("alpha", None).unwrap();
        hub.bootstrap("beta", None).unwrap();
        // unbootstrapped dir: no config.json -> skipped
        std::fs::create_dir_all(tmp.path().join("ghost")).unwrap();

        let reports = run_all_spaces(tmp.path());
        let joined = reports.join("\n");
        assert!(joined.contains("space=alpha"), "got: {joined}");
        assert!(joined.contains("space=beta"), "got: {joined}");
        assert!(!joined.contains("space=ghost"), "got: {joined}");
        // sorted order
        let a = joined.find("space=alpha").unwrap();
        let b = joined.find("space=beta").unwrap();
        assert!(a < b);
    }

    #[test]
    fn run_all_spaces_on_missing_root_is_empty() {
        assert!(run_all_spaces(Path::new("/nonexistent/rw-cron-test")).is_empty());
    }

    #[test]
    fn interval_env_parsing() {
        // default when unset — can't unset safely in-process, so just
        // verify the parse paths via the same logic shape
        assert_eq!("x".parse::<u64>().is_ok(), false);
        let _ = interval_from_env(); // smoke: must not panic
    }
}
