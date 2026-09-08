//! Git backing for the vault root: one repo, shell `git`, best-effort.
//!
//! Cycle (driven by `spawn_git`): commit-if-idle-and-dirty → pull --rebase
//! (abort on any conflict — conflict markers inside pages are worse than a
//! stale remote) → push-if-upstream. Writers are never blocked by this;
//! every failure is a stderr line, never an error surfaced to callers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

pub const DEFAULT_TICK_SECS: u64 = 60;
pub const DEFAULT_IDLE_SECS: u64 = 300;

/// Tick interval from env; 0 disables git backing.
pub fn tick_secs_from_env() -> u64 {
    std::env::var("WIKI_GIT_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TICK_SECS)
}

pub fn idle_secs_from_env() -> u64 {
    std::env::var("WIKI_GIT_IDLE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_IDLE_SECS)
}

const GITIGNORE: &str = "meta/embeddings.json\n.obsidian/workspace*\n*.tmp\n";

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=rust-wiki",
            "-c",
            "user.email=wiki@localhost",
        ])
        .args(args)
        .output()
        .map_err(|e| format!("spawn git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn is_repo(root: &Path) -> bool {
    root.join(".git").exists()
}

/// Init + .gitignore + initial commit when no repo exists. Returns `true`
/// when it created the repo. Never touches an existing repo.
pub fn ensure_repo(root: &Path) -> Result<bool, String> {
    if is_repo(root) {
        return Ok(false);
    }
    git(root, &["init", "-q"])?;
    let gi = root.join(".gitignore");
    if !gi.exists() {
        std::fs::write(&gi, GITIGNORE).map_err(|e| e.to_string())?;
    }
    git(root, &["add", "-A"])?;
    // Empty vault still gets its root commit so pull/push have a base.
    let _ = git(
        root,
        &[
            "commit",
            "-q",
            "-m",
            "wiki: initial commit",
            "--allow-empty",
        ],
    );
    Ok(true)
}

pub fn dirty(root: &Path) -> bool {
    git(root, &["status", "--porcelain"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Seconds since the newest file under root changed (`.git` excluded).
/// Returns `u64::MAX` when nothing is there to measure.
pub fn idle_secs(root: &Path) -> u64 {
    fn newest(dir: &Path, best: &mut Option<SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.file_name().map(|n| n == ".git").unwrap_or(false) {
                continue;
            }
            if p.is_dir() {
                newest(&p, best);
            } else if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                if best.is_none_or(|b| m > b) {
                    *best = Some(m);
                }
            }
        }
    }
    let mut best = None;
    newest(root, &mut best);
    best.and_then(|b| SystemTime::now().duration_since(b).ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX)
}

/// `add -A` + commit when dirty. Returns `true` when it committed.
pub fn commit_all(root: &Path, msg: &str) -> Result<bool, String> {
    git(root, &["add", "-A"])?;
    if git(root, &["diff", "--cached", "--quiet"]).is_ok() {
        return Ok(false); // nothing staged
    }
    git(root, &["commit", "-q", "-m", msg])?;
    Ok(true)
}

pub fn has_upstream(root: &Path) -> bool {
    git(
        root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .is_ok()
}

pub fn push(root: &Path) -> Result<(), String> {
    git(root, &["push", "-q"])?;
    Ok(())
}

pub fn head(root: &Path) -> Option<String> {
    git(root, &["rev-parse", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
}

/// Last-tick outcome, persisted at `<root>/meta/git.json` so `wiki_status`
/// can surface git problems to agents (and humans) instead of burying
/// them in server stderr.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GitState {
    pub last_tick: String,
    pub ok: bool,
    pub detail: String,
    pub head: Option<String>,
}

pub fn state_path(root: &Path) -> PathBuf {
    root.join("meta").join("git.json")
}

pub fn read_state(root: &Path) -> Option<GitState> {
    let raw = std::fs::read_to_string(state_path(root)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_state(root: &Path, state: &GitState) {
    let _ = std::fs::create_dir_all(root.join("meta"));
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(state_path(root), json);
    }
}

#[derive(Debug, PartialEq)]
pub enum PullOutcome {
    UpToDate,
    Moved,
    Conflict,
}

/// Fetch + rebase local work on top. Any conflict aborts the rebase and
/// reports `Conflict` — the working tree is left exactly as it was.
pub fn pull_rebase(root: &Path) -> Result<PullOutcome, String> {
    if !has_upstream(root) {
        return Ok(PullOutcome::UpToDate);
    }
    let before = head(root);
    match git(root, &["pull", "--rebase", "-q"]) {
        Ok(_) => Ok(if head(root) == before {
            PullOutcome::UpToDate
        } else {
            PullOutcome::Moved
        }),
        Err(e) => {
            let _ = git(root, &["rebase", "--abort"]);
            if git(root, &["status", "--porcelain"])
                .map(|s| s.trim().is_empty())
                .unwrap_or(false)
                && head(root) == before
            {
                Ok(PullOutcome::Conflict)
            } else {
                Err(e)
            }
        }
    }
}

/// One full cycle. Returns log lines; rescan needed when HEAD moved.
/// Never fails the caller — errors become lines. Persists the outcome to
/// `meta/git.json` so `wiki_status` surfaces problems to agents.
pub fn tick(root: &Path, idle_want: u64, now_iso: &str) -> (Vec<String>, bool) {
    let mut lines = vec![];
    let mut problems: Vec<String> = vec![];
    if let Err(e) = ensure_repo(root) {
        lines.push(format!("[rust-wiki git] ensure_repo: {e}"));
        problems.push(format!("backing unavailable: {e}"));
        write_state(
            root,
            &GitState {
                last_tick: now_iso.into(),
                ok: false,
                detail: problems.join("; "),
                head: head(root),
            },
        );
        return (lines, false);
    }
    if idle_secs(root) >= idle_want && dirty(root) {
        match commit_all(root, &format!("wiki: auto-commit {now_iso}")) {
            Ok(true) => lines.push("[rust-wiki git] committed".into()),
            Ok(false) => {}
            Err(e) => {
                lines.push(format!("[rust-wiki git] commit: {e}"));
                problems.push(format!("commit failed: {e}"));
            }
        }
    }
    match pull_rebase(root) {
        Ok(PullOutcome::Moved) => lines.push("[rust-wiki git] pulled (HEAD moved)".into()),
        Ok(PullOutcome::Conflict) => {
            lines.push("[rust-wiki git] pull conflict — kept local, needs manual rebase".into());
            problems.push("pull conflict — kept local, needs manual rebase".into());
        }
        Ok(PullOutcome::UpToDate) => {}
        Err(e) => {
            lines.push(format!("[rust-wiki git] pull: {e}"));
            problems.push(format!("pull failed: {e}"));
        }
    }
    if has_upstream(root) {
        if let Err(e) = push(root) {
            lines.push(format!("[rust-wiki git] push: {e}"));
            problems.push(format!("push failed: {e}"));
        }
    }
    let moved = lines.iter().any(|l| l.contains("pulled (HEAD moved)"));
    write_state(
        root,
        &GitState {
            last_tick: now_iso.into(),
            ok: problems.is_empty(),
            detail: if problems.is_empty() {
                "in sync".into()
            } else {
                problems.join("; ")
            },
            head: head(root),
        },
    );
    (lines, moved)
}

/// Spawn the background git thread. No-op when interval is 0.
/// `rescan` runs when a pull moved HEAD (human changes need reindexing).
pub fn spawn_git(
    root: PathBuf,
    rescan: impl Fn() + Send + 'static,
) -> Option<std::thread::JoinHandle<()>> {
    let interval = tick_secs_from_env();
    if interval == 0 {
        return None;
    }
    let idle_want = idle_secs_from_env();
    Some(std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(interval));
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let (lines, moved) = tick(&root, idle_want, &now);
        for line in lines {
            tracing::info!("{line}");
        }
        if moved {
            rescan();
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        if git(tmp.path(), &["init", "-q"]).is_err() {
            panic!("git binary required for git tests");
        }
        tmp
    }

    #[test]
    fn ensure_repo_inits_with_gitignore_and_root_commit() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(ensure_repo(tmp.path()).unwrap());
        assert!(is_repo(tmp.path()));
        let gi = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(gi.contains("embeddings.json"));
        assert!(head(tmp.path()).is_some());
        // Second call is a no-op.
        assert!(!ensure_repo(tmp.path()).unwrap());
    }

    #[test]
    fn commit_roundtrip_and_idle() {
        let tmp = fresh_repo();
        std::fs::write(tmp.path().join("a.md"), "hello\n").unwrap();
        assert!(dirty(tmp.path()));
        assert!(commit_all(tmp.path(), "wiki: auto-commit t").unwrap());
        assert!(!dirty(tmp.path()));
        assert!(!commit_all(tmp.path(), "wiki: auto-commit t2").unwrap());
        assert!(idle_secs(tmp.path()) < 60);
    }

    #[test]
    fn divergent_histories_abort_clean() {
        // Bare upstream + two clones, both commit on top of the same base.
        let base = tempfile::tempdir().unwrap();
        let upstream = base.path().join("up.git");
        assert!(Command::new("git")
            .args(["init", "-q", "--bare"])
            .arg(&upstream)
            .output()
            .unwrap()
            .status
            .success());
        let work = |name: &str| {
            let d = base.path().join(name);
            assert!(Command::new("git")
                .args(["clone", "-q"])
                .arg(&upstream)
                .arg(&d)
                .output()
                .unwrap()
                .status
                .success());
            // Clone of an empty upstream has no HEAD yet — seed via repo A below.
            d
        };
        let a = work("a");
        // Seed: commit + push from A (needs a branch; bare default may be master).
        std::fs::write(a.join("page.md"), "base\n").unwrap();
        assert!(git(&a, &["add", "-A"]).is_ok());
        assert!(git(&a, &["commit", "-q", "-m", "seed"]).is_ok());
        let branch = git(&a, &["branch", "--show-current"])
            .map(|s| s.trim().to_string())
            .unwrap();
        let branch = if branch.is_empty() {
            "master".into()
        } else {
            branch
        };
        assert!(git(&a, &["push", "-q", "-u", "origin", &branch]).is_ok());

        let b = work("b");
        // Divergent same-file edits on both sides.
        std::fs::write(a.join("page.md"), "from A\n").unwrap();
        assert!(commit_all(&a, "A change").unwrap());
        assert!(push(&a).is_ok());
        std::fs::write(b.join("page.md"), "from B\n").unwrap();
        assert!(commit_all(&b, "B change").unwrap());
        // B's pull must abort clean: Conflict, HEAD unmoved, no markers.
        let before = head(&b).unwrap();
        assert_eq!(pull_rebase(&b).unwrap(), PullOutcome::Conflict);
        assert_eq!(head(&b).unwrap(), before);
        let content = std::fs::read_to_string(b.join("page.md")).unwrap();
        assert_eq!(content, "from B\n");
        assert!(!content.contains("<<<<<<<"));
    }
}
