//! Agent working-memory: trajectory packets + skill/case distillation.
//! A completed task is captured as an immutable packet under
//! `raw/trajectories/TRJ-*` (steps + summary); agents generalize packets
//! into `wiki/skills/*` pages (and optional `wiki/cases/*`) via ensure_page.
//! The steps sequence is caller-supplied (the harness owns the live session);
//! the server only stores, never executes.

use std::fs;

use serde::{Deserialize, Serialize};

use super::layout::VaultPaths;
use super::lint::slugify;
use super::registry::rebuild_metadata;

#[derive(Debug, Serialize, Deserialize)]
struct TrajectoryManifest {
    id: String,
    title: String,
    outcome: String,
    captured_at: String,
    distilled: bool,
}

#[derive(Debug)]
pub struct CapturedTrajectory {
    pub trajectory_id: String,
    pub case_page_id: String,
}

#[derive(Debug)]
pub struct Undistilled {
    pub trajectory_id: String,
    pub title: String,
    pub summary: String,
}

fn trajectories_dir(vault: &VaultPaths) -> std::path::PathBuf {
    vault.raw().join("trajectories")
}

fn next_seq(vault: &VaultPaths, date: &str) -> Result<u32, String> {
    let mut max = 0u32;
    let dir = trajectories_dir(vault);
    if dir.exists() {
        for e in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let name = e
                .map_err(|e| e.to_string())?
                .file_name()
                .to_string_lossy()
                .into_owned();
            if let Some(rest) = name.strip_prefix(&format!("TRJ-{date}-")) {
                if let Ok(n) = rest.parse::<u32>() {
                    max = max.max(n);
                }
            }
        }
    }
    Ok(max + 1)
}

fn valid_outcome(outcome: &str) -> bool {
    matches!(outcome, "success" | "failure" | "partial")
}

/// Capture a completed task: immutable packet + skeleton case page.
/// `steps` is the caller-supplied tool-call record (JSON); `summary` is a
/// self-contained prose recap used by distill listings.
#[allow(clippy::too_many_arguments)]
pub fn capture_trajectory(
    vault: &VaultPaths,
    date: &str,
    now_iso: &str,
    title: &str,
    outcome: &str,
    steps: &serde_json::Value,
    summary: &str,
) -> Result<CapturedTrajectory, String> {
    if !valid_outcome(outcome) {
        return Err(format!(
            "invalid outcome '{outcome}' — success|failure|partial"
        ));
    }
    let seq = next_seq(vault, date)?;
    let trajectory_id = format!("TRJ-{date}-{seq:03}");
    let packet = trajectories_dir(vault).join(&trajectory_id);
    fs::create_dir_all(&packet).map_err(|e| e.to_string())?;

    let manifest = TrajectoryManifest {
        id: trajectory_id.clone(),
        title: title.to_string(),
        outcome: outcome.to_string(),
        captured_at: now_iso.to_string(),
        distilled: false,
    };
    fs::write(
        packet.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        packet.join("packet.json"),
        serde_json::to_string_pretty(steps).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    fs::write(packet.join("extracted.md"), summary).map_err(|e| e.to_string())?;

    // skeleton case page (flesh out via write_page; fence mandatory there)
    let slug = slugify(title);
    if slug.is_empty() {
        return Err("title does not slugify".to_string());
    }
    let case_page_id = format!("cases/{slug}");
    let case_path = vault.page_path(&case_page_id);
    if !case_path.exists() {
        if let Some(parent) = case_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(
            &case_path,
            format!(
                "---\ntitle: \"{title}\"\ntype: case\ntrajectory_id: {trajectory_id}\noutcome: {outcome}\ncreated: {date}\nupdated: {date}\n---\n\n# {title}\n\n## Task\n\n\n## Approach\n\n\n## Outcome\n\n"
            ),
        )
        .map_err(|e| e.to_string())?;
    }
    rebuild_metadata(vault)?;
    Ok(CapturedTrajectory {
        trajectory_id,
        case_page_id,
    })
}

/// List undistilled trajectories (oldest first), flipping `mark` to distilled.
/// Cap batch at 5 like ingest.
pub fn distill(vault: &VaultPaths, mark: &[String]) -> Result<Vec<Undistilled>, String> {
    let dir = trajectories_dir(vault);
    for id in mark {
        if id.contains('/') || id.contains("..") {
            return Err(format!("invalid trajectory id '{id}'"));
        }
        let mf = dir.join(id).join("manifest.json");
        let raw = fs::read_to_string(&mf).map_err(|e| e.to_string())?;
        let mut m: TrajectoryManifest = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        m.distilled = true;
        fs::write(&mf, serde_json::to_string_pretty(&m).unwrap()).map_err(|e| e.to_string())?;
    }
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut ids: Vec<String> = Vec::new();
    for e in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        ids.push(
            e.map_err(|e| e.to_string())?
                .file_name()
                .to_string_lossy()
                .into_owned(),
        );
    }
    ids.sort();
    for id in ids {
        if out.len() >= 5 {
            break;
        }
        let packet = dir.join(&id);
        let raw = fs::read_to_string(packet.join("manifest.json")).map_err(|e| e.to_string())?;
        let m: TrajectoryManifest = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if m.distilled {
            continue;
        }
        let summary = fs::read_to_string(packet.join("extracted.md")).unwrap_or_default();
        out.push(Undistilled {
            trajectory_id: id,
            title: m.title,
            summary,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::layout::VaultPaths;

    fn setup() -> (tempfile::TempDir, VaultPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        super::super::bootstrap::bootstrap(&v, "t").unwrap();
        (tmp, v)
    }

    #[test]
    fn capture_distill_roundtrip() {
        let (_t, v) = setup();
        let steps = serde_json::json!([{"tool": "read", "ok": true}]);
        let c = capture_trajectory(
            &v,
            "2026-09-09",
            "2026-09-09T00:00:00Z",
            "Fix flaky test",
            "success",
            &steps,
            "Rerouted fixture setup.",
        )
        .unwrap();
        assert_eq!(c.trajectory_id, "TRJ-2026-09-09-001");
        assert_eq!(c.case_page_id, "cases/fix-flaky-test");
        assert!(v.page_path(&c.case_page_id).exists());

        let batch = distill(&v, &[]).unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].trajectory_id, c.trajectory_id);

        let batch2 = distill(&v, std::slice::from_ref(&c.trajectory_id)).unwrap();
        assert!(batch2.is_empty());

        let bad = capture_trajectory(&v, "2026-09-09", "t", "X", "maybe", &steps, "s");
        assert!(bad.unwrap_err().contains("outcome"));
    }
}
