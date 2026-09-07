//! wiki_watch: scheduled maintenance. No daemon — cron owns the clock.
//! `cron_cycle` = one mechanical pass (lint + auto_fix + status);
//! `crontab_line` renders the cron entry that runs it.

use crate::api::WikiApi;
use crate::hub::Hub;

pub const SCHEDULES: &[(&str, &str)] = &[
    ("hourly", "0 * * * *"),
    ("daily", "0 3 * * *"),
    ("weekly", "0 3 * * 0"),
];

pub fn crontab_line(space: &str, schedule: &str, exe: &str) -> Result<String, String> {
    let cron_field = SCHEDULES
        .iter()
        .find(|(name, _)| *name == schedule)
        .map(|(_, field)| *field)
        .ok_or_else(|| format!("unknown schedule '{schedule}' — hourly | daily | weekly"))?;
    Ok(format!(
        "{cron_field} {exe} cron --space {space} >> /tmp/rust-wiki-cron-{space}.log 2>&1"
    ))
}

/// One mechanical maintenance cycle. No LLM, no fetching.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crontab_lines_per_schedule() {
        let line = crontab_line("proj-x", "daily", "/usr/local/bin/rust-wiki").unwrap();
        assert!(line.starts_with("0 3 * * * /usr/local/bin/rust-wiki cron --space proj-x"));
        assert!(line.contains(">> /tmp/rust-wiki-cron-proj-x.log"));
        assert!(crontab_line("s", "hourly", "rust-wiki")
            .unwrap()
            .starts_with("0 * * * *"));
        assert!(crontab_line("s", "weekly", "rust-wiki")
            .unwrap()
            .starts_with("0 3 * * 0"));
        let err = crontab_line("s", "minutely", "rust-wiki").unwrap_err();
        assert!(err.contains("unknown schedule"));
    }

    #[test]
    fn cron_cycle_reports_and_autofixes() {
        let tmp = tempfile::tempdir().unwrap();
        let hub = Hub::new(tmp.path().to_path_buf());
        hub.bootstrap("s", None).unwrap();
        let vp = crate::vault::layout::VaultPaths::new(tmp.path(), "s");
        // one page citing a missing target twice -> lint auto_fix stubs it
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
}
