//! Source capture: immutable packets under raw/sources/SRC-*.
//! `text` and `url` are the agent-facing inputs; `file_path` resolves
//! server-side only (documents a local file on the SERVER).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::convert::{self, Converter};
use super::layout::VaultPaths;
use super::pages;
use super::registry::rebuild_metadata;

#[derive(Debug, Serialize, PartialEq)]
pub struct Captured {
    pub source_id: String,
    pub extracted_chars: usize,
    pub extracted_preview: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Manifest {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_path: Option<String>,
    captured_at: String,
    ingested: bool,
}

/// Highest NNN used for `SRC-<date>-` so ids stay unique per day.
fn next_seq(vault: &VaultPaths, date: &str) -> Result<u32, String> {
    let mut max = 0u32;
    let dir = vault.raw_sources();
    if dir.exists() {
        for e in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let name = e
                .map_err(|e| e.to_string())?
                .file_name()
                .to_string_lossy()
                .into_owned();
            if let Some(rest) = name.strip_prefix(&format!("SRC-{date}-")) {
                if let Ok(n) = rest.parse::<u32>() {
                    max = max.max(n);
                }
            }
        }
    }
    Ok(max + 1)
}

fn title_of(text: &str, fallback: &str) -> String {
    for line in text.lines() {
        if let Some(h) = line.trim().strip_prefix("# ") {
            let h = h.trim();
            if !h.is_empty() {
                return h.chars().take(80).collect();
            }
        }
    }
    fallback.to_string()
}

/// Capture text/url/file into a packet + skeleton source page.
/// `url_body` is pre-fetched markdown (the Hub does the HTTP), and
/// `converter` turns local file bytes into markdown, so this stays pure fs —
/// easy to test, no network in the engine.
pub fn capture(
    vault: &VaultPaths,
    date: &str,
    now_iso: &str,
    input: CaptureInput,
    converter: &dyn Converter,
) -> Result<Captured, String> {
    let seq = next_seq(vault, date)?;
    let source_id = format!("SRC-{date}-{seq:03}");
    let packet = vault.raw_sources().join(&source_id);
    fs::create_dir_all(packet.join("original")).map_err(|e| e.to_string())?;

    let title: String;
    let extracted: String;
    let original_name: String;
    let original: Vec<u8>;
    let url: Option<String>;
    let file_path: Option<String>;
    match input {
        CaptureInput::Text {
            title: title_opt,
            text,
        } => {
            title = title_opt.unwrap_or_else(|| title_of(&text, &source_id));
            extracted = text.clone();
            original_name = "text.md".into();
            original = text.into_bytes();
            url = None;
            file_path = None;
        }
        CaptureInput::Url {
            title: title_opt,
            url: u,
            markdown,
        } => {
            title = title_opt.unwrap_or_else(|| title_of(&markdown, &u));
            extracted = markdown;
            original_name = "page.html".into();
            original = Vec::new(); // caller archived raw html separately when available
            url = Some(u);
            file_path = None;
        }
        CaptureInput::File {
            title: title_opt,
            path: fp,
        } => {
            let p = Path::new(&fp);
            if !p.is_file() {
                return Err(format!("server-local file not found: {fp}"));
            }
            let len = fs::metadata(p).map_err(|e| e.to_string())?.len();
            if len > convert::MAX_BYTES {
                return Err(format!(
                    "file is {len} bytes — over the {} byte capture limit",
                    convert::MAX_BYTES
                ));
            }
            let bytes = fs::read(p).map_err(|e| e.to_string())?;
            // Extension first, magic bytes second — a mislabelled .txt that is
            // really a PDF must not be stored as mojibake.
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            let kind = convert::sniff(&bytes).unwrap_or_else(|| convert::from_extension(ext));
            extracted = converter.convert(&bytes, &kind)?;
            title = title_opt.unwrap_or_else(|| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&source_id)
                    .to_string()
            });
            original_name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string();
            original = bytes;
            url = None;
            file_path = Some(fp);
        }
    }

    let manifest = Manifest {
        id: source_id.clone(),
        title: title.clone(),
        url: url.clone(),
        file_path,
        captured_at: now_iso.to_string(),
        ingested: false,
    };
    fs::write(
        packet.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    if !original.is_empty() {
        fs::write(packet.join("original").join(original_name), original)
            .map_err(|e| e.to_string())?;
    } else if url.is_some() {
        // keep a placeholder so original/ is never empty for url captures
        fs::write(
            packet.join("original").join(original_name),
            b"see extracted.md",
        )
        .map_err(|e| e.to_string())?;
    }
    fs::write(packet.join("extracted.md"), &extracted).map_err(|e| e.to_string())?;

    // skeleton source page
    let slug = source_id.to_lowercase();
    let _ = pages::valid_slug(&slug); // SRC- ids are uppercase; page id lowercased
    let page_id = format!("sources/{slug}");
    let page_path = vault.page_path(&page_id);
    if !page_path.exists() {
        fs::write(
            &page_path,
            format!(
                "---\ntitle: \"{title}\"\ntype: source\nsource_id: {source_id}\n---\n\n# {title}\n\nSource: {}\n\n## Key claims\n\n\n## Quotes\n\n",
                url.as_deref().unwrap_or("-")
            ),
        )
        .map_err(|e| e.to_string())?;
    }
    rebuild_metadata(vault)?;

    let preview: String = extracted.chars().take(300).collect();
    Ok(Captured {
        source_id,
        extracted_chars: extracted.chars().count(),
        extracted_preview: preview,
    })
}

#[derive(Debug, Clone)]
pub enum CaptureInput {
    Text {
        title: Option<String>,
        text: String,
    },
    Url {
        title: Option<String>,
        url: String,
        markdown: String,
    },
    File {
        title: Option<String>,
        path: String,
    },
}

/// Pending (uningested) sources, oldest first.
pub fn pending(vault: &VaultPaths) -> Result<Vec<(String, String, usize)>, String> {
    let dir = vault.raw_sources();
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for packet in entries {
        let mf = packet.join("manifest.json");
        if !mf.is_file() {
            continue;
        }
        let raw = fs::read_to_string(&mf).map_err(|e| e.to_string())?;
        let m: Manifest =
            serde_json::from_str(&raw).map_err(|e| format!("manifest {}: {e}", mf.display()))?;
        if m.ingested {
            continue;
        }
        let extracted = packet.join("extracted.md");
        let chars = fs::read_to_string(&extracted)
            .map(|c| c.chars().count())
            .unwrap_or(0);
        out.push((m.id, m.title, chars));
    }
    Ok(out)
}

/// Flip packets to ingested and log. Idempotent per id.
pub fn mark_ingested(vault: &VaultPaths, ids: &[String], now_iso: &str) -> Result<(), String> {
    for id in ids {
        let mf = vault.raw_sources().join(id).join("manifest.json");
        if !mf.is_file() {
            return Err(format!("unknown source '{id}'"));
        }
        let raw = fs::read_to_string(&mf).map_err(|e| e.to_string())?;
        let mut m: Manifest = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if m.ingested {
            continue;
        }
        m.ingested = true;
        fs::write(&mf, serde_json::to_string_pretty(&m).unwrap()).map_err(|e| e.to_string())?;
        super::registry::log_event(
            vault,
            "ingest",
            &serde_json::json!({"source_id": id}),
            now_iso,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::convert::{self, ContentKind, Converter, DefaultConverter};
    use super::*;

    /// Records what the capture path decided each file was, so the wiring
    /// (extension, then magic bytes) is asserted without real documents.
    #[derive(Default)]
    struct RecordingConverter {
        kinds: std::sync::Mutex<Vec<ContentKind>>,
    }

    impl Converter for RecordingConverter {
        fn convert(&self, _bytes: &[u8], kind: &ContentKind) -> Result<String, String> {
            self.kinds.lock().unwrap().push(kind.clone());
            Ok(format!("converted {}", kind.name()))
        }
    }

    fn setup() -> (tempfile::TempDir, VaultPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "s");
        super::super::bootstrap::bootstrap(&v, "t").unwrap();
        (tmp, v)
    }

    #[test]
    fn capture_text_then_ingest_flow() {
        let (_t, v) = setup();
        let c1 = capture(
            &v,
            "2026-09-07",
            "t1",
            CaptureInput::Text {
                title: None,
                text: "# Spec Notes\n\ncontent here\n".into(),
            },
            &DefaultConverter,
        )
        .unwrap();
        assert_eq!(c1.source_id, "SRC-2026-09-07-001");
        assert!(c1.extracted_preview.contains("# Spec Notes"));
        let c2 = capture(
            &v,
            "2026-09-07",
            "t2",
            CaptureInput::Text {
                title: Some("Second".into()),
                text: "body".into(),
            },
            &DefaultConverter,
        )
        .unwrap();
        assert_eq!(c2.source_id, "SRC-2026-09-07-002");

        let pend = pending(&v).unwrap();
        assert_eq!(pend.len(), 2);
        assert_eq!(pend[0].0, "SRC-2026-09-07-001");

        mark_ingested(&v, &["SRC-2026-09-07-001".into()], "t3").unwrap();
        let pend2 = pending(&v).unwrap();
        assert_eq!(pend2.len(), 1);
        assert_eq!(pend2[0].0, "SRC-2026-09-07-002");

        // skeleton page + log projection
        assert!(v.page_path("sources/src-2026-09-07-001").exists());
        super::super::registry::rebuild_log(&v).unwrap();
        let log = fs::read_to_string(v.log_file()).unwrap();
        assert!(log.contains("SRC-2026-09-07-001"));

        let _ = c1;
    }

    #[test]
    fn file_capture_classifies_then_converts() {
        let (_t, v) = setup();
        let files = tempfile::tempdir().unwrap();
        let rec = RecordingConverter::default();
        let cases: [(&str, &[u8], ContentKind); 3] = [
            ("notes.md", b"# Notes\n\nfrom a file\n", ContentKind::Text),
            ("page.html", b"<p>hi</p>", ContentKind::Html),
            // Wrong extension, right magic bytes: a mislabelled PDF is still a
            // PDF, and must not land in the vault as mojibake.
            ("fake.txt", b"%PDF-1.4\n...", ContentKind::Pdf),
        ];
        for (name, bytes, want) in cases {
            let f = files.path().join(name);
            fs::write(&f, bytes).unwrap();
            let c = capture(
                &v,
                "2026-09-07",
                "t",
                CaptureInput::File {
                    title: None,
                    path: f.to_string_lossy().into(),
                },
                &rec,
            )
            .unwrap();
            assert!(c.extracted_preview.contains("converted"), "{name}");
            assert_eq!(rec.kinds.lock().unwrap().pop().unwrap(), want, "{name}");
        }

        // A binary type is refused by name instead of decoded as text.
        let png = files.path().join("shot.png");
        fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
        let err = capture(
            &v,
            "2026-09-07",
            "t",
            CaptureInput::File {
                title: None,
                path: png.to_string_lossy().into(),
            },
            &DefaultConverter,
        )
        .unwrap_err();
        assert!(err.contains("png"), "{err}");

        // Size is refused before the file is read (the file is sparse, so this
        // costs no disk).
        let big = files.path().join("big.md");
        fs::File::create(&big)
            .unwrap()
            .set_len(convert::MAX_BYTES + 1)
            .unwrap();
        let err = capture(
            &v,
            "2026-09-07",
            "t",
            CaptureInput::File {
                title: None,
                path: big.to_string_lossy().into(),
            },
            &DefaultConverter,
        )
        .unwrap_err();
        assert!(err.contains("capture limit"), "{err}");

        let missing = capture(
            &v,
            "2026-09-07",
            "t",
            CaptureInput::File {
                title: None,
                path: "/nope/missing.md".into(),
            },
            &DefaultConverter,
        )
        .unwrap_err();
        assert!(missing.contains("not found"), "{missing}");
    }
}
