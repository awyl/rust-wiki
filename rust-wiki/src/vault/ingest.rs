//! Ingest batching over pending sources (cooperative: the agent synthesizes).

use super::capture;
use super::layout::VaultPaths;

pub const DEFAULT_BATCH: u32 = 3;
pub const MAX_BATCH: u32 = 5;

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct Batch {
    pub items: Vec<Item>,
    pub remaining: u64,
    pub all_ingested: bool,
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct Item {
    pub source_id: String,
    pub title: String,
    pub chars: usize,
    pub extracted_path: String,
    /// Full extracted content — the agent synthesizes from THIS, since
    /// raw/ is not page-readable on a remote server.
    pub extracted: String,
}

/// Next batch of uningested sources (oldest first).
pub fn next_batch(vault: &VaultPaths, source_id: Option<&str>, batch_size: Option<u32>) -> Result<Batch, String> {
    let size = batch_size.unwrap_or(DEFAULT_BATCH).min(MAX_BATCH).max(1) as usize;
    let read_extracted = |sid: &str| {
        std::fs::read_to_string(vault.raw_sources().join(sid).join("extracted.md")).unwrap_or_default()
    };
    let pending = capture::pending(vault)?;
    let items: Vec<Item> = match source_id {
        Some(id) => pending
            .into_iter()
            .filter(|(sid, _, _)| sid == id)
            .take(1)
            .map(|(sid, title, chars)| {
                let extracted = read_extracted(&sid);
                Item {
                    extracted_path: format!("raw/sources/{sid}/extracted.md"),
                    source_id: sid,
                    title,
                    chars,
                    extracted,
                }
            })
            .collect(),
        None => pending
            .into_iter()
            .take(size)
            .map(|(sid, title, chars)| {
                let extracted = read_extracted(&sid);
                Item {
                    extracted_path: format!("raw/sources/{sid}/extracted.md"),
                    source_id: sid,
                    title,
                    chars,
                    extracted,
                }
            })
            .collect(),
    };
    let total_pending = capture::pending(vault)?.len() as u64;
    Ok(Batch {
        remaining: total_pending.saturating_sub(items.len() as u64),
        all_ingested: total_pending == 0,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, VaultPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        super::super::bootstrap::bootstrap(&v, "t").unwrap();
        (tmp, v)
    }

    #[test]
    fn batch_respects_size_and_marks_progress() {
        let (_t, v) = setup();
        for i in 1..=4 {
            capture::capture(&v, "2026-09-07", "t", capture::CaptureInput::Text {
                title: Some(format!("Source {i}")),
                text: format!("body {i}"),
            })
            .unwrap();
        }
        let b1 = next_batch(&v, None, None).unwrap();
        assert_eq!(b1.items.len(), 3);
        assert_eq!(b1.remaining, 1);
        assert!(!b1.all_ingested);
        // batch carries the full extracted content for synthesis
        assert!(b1.items[0].extracted.contains("body 1"));

        capture::mark_ingested(&v, &b1.items.iter().map(|i| i.source_id.clone()).collect::<Vec<_>>(), "t").unwrap();
        let b2 = next_batch(&v, None, None).unwrap();
        assert_eq!(b2.items.len(), 1);

        capture::mark_ingested(&v, &b2.items.iter().map(|i| i.source_id.clone()).collect::<Vec<_>>(), "t").unwrap();
        let b3 = next_batch(&v, None, None).unwrap();
        assert!(b3.all_ingested);
        assert!(b3.items.is_empty());

        // specific-id batch works even when ingested list is long
        let one = next_batch(&v, Some("SRC-2026-09-07-002"), None).unwrap();
        assert!(one.items.is_empty()); // already ingested above
    }
}
