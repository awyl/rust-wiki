//! Bytes → markdown: the one place that decides what a captured source is.
//!
//! Both capture paths funnel through here — a URL fetch (HTTP `content-type`)
//! and a server-local file (extension) — so the same document behaves the same
//! way whichever door it came in through.

/// Upper bound on a captured document. Sources are articles and papers, not
/// disk images; the cap stops an accidental or hostile response from eating
/// the server's memory before anyone looks at it.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// What the bytes are. `Unsupported` keeps the raw type so errors can name it
/// rather than guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentKind {
    Html,
    Text,
    Pdf,
    Unsupported(String),
}

impl ContentKind {
    pub fn name(&self) -> String {
        match self {
            ContentKind::Html => "html".into(),
            ContentKind::Text => "text".into(),
            ContentKind::Pdf => "pdf".into(),
            ContentKind::Unsupported(t) => t.clone(),
        }
    }
}

/// Classify an HTTP `content-type` (parameters such as `charset=` ignored).
pub fn from_content_type(ct: &str) -> ContentKind {
    let mime = ct
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match mime.as_str() {
        "text/html" | "application/xhtml+xml" => ContentKind::Html,
        "application/pdf" => ContentKind::Pdf,
        // Structured text is captured as-is: the point is the content, and
        // rendering json/xml as markdown would lose the structure.
        "application/json" | "application/xml" | "application/yaml" | "application/x-yaml" => {
            ContentKind::Text
        }
        m if m.starts_with("text/") => ContentKind::Text,
        "" => ContentKind::Unsupported("unknown".into()),
        other => ContentKind::Unsupported(other.into()),
    }
}

/// Classify a file extension (with or without the dot, any case).
pub fn from_extension(ext: &str) -> ContentKind {
    match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "html" | "htm" | "xhtml" => ContentKind::Html,
        "pdf" => ContentKind::Pdf,
        // An extensionless file (README, LICENSE) is text.
        "" | "md" | "markdown" | "txt" | "text" | "json" | "xml" | "yml" | "yaml" | "toml"
        | "csv" | "log" => ContentKind::Text,
        other => ContentKind::Unsupported(other.into()),
    }
}

/// Magic-byte override: servers and file names lie, `%PDF-` does not. Without
/// this a PDF served as `text/plain` would be stored as mojibake.
pub fn sniff(bytes: &[u8]) -> Option<ContentKind> {
    bytes.starts_with(b"%PDF-").then_some(ContentKind::Pdf)
}

/// Seam: turn captured bytes into markdown. Production uses `DefaultConverter`;
/// tests stub it, so no fixture documents are needed for the plumbing paths.
pub trait Converter: Send + Sync {
    fn convert(&self, bytes: &[u8], kind: &ContentKind) -> Result<String, String>;
}

/// The real converter. Stateless.
pub struct DefaultConverter;

impl Converter for DefaultConverter {
    fn convert(&self, bytes: &[u8], kind: &ContentKind) -> Result<String, String> {
        match kind {
            ContentKind::Html => Ok(html2md::parse_html(&as_text(bytes))),
            ContentKind::Text => Ok(as_text(bytes)),
            ContentKind::Pdf => pdf_to_markdown(bytes),
            ContentKind::Unsupported(t) => Err(format!(
                "unsupported content type '{t}' — supported: html, text, markdown, json, xml, pdf"
            )),
        }
    }
}

/// Lossy on purpose: a mislabelled encoding should degrade to readable text,
/// not abort the capture.
fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn pdf_to_markdown(bytes: &[u8]) -> Result<String, String> {
    let raw = pdf_extract::extract_text_from_mem(bytes).map_err(|e| format!("pdf text: {e}"))?;
    let cleaned = tidy(&raw);
    if cleaned.trim().is_empty() {
        return Err(
            "no text layer in this pdf (a scanned image carries none, and OCR is not supported)"
                .into(),
        );
    }
    Ok(cleaned)
}

/// pdf-extract emits one line per visual line, which reads badly in a wiki.
/// Join wrapped lines and undo end-of-line hyphenation; leave blank lines and
/// indentation alone, because they are all a table or code block has.
fn tidy(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        let line = line.trim_end();
        if let Some(stem) = line.strip_suffix('-') {
            // Only a hyphen at the very end of a line is hyphenation; a real
            // compound (`state-of-the-art`) is followed by a space.
            if !stem.ends_with('-') {
                out.push_str(stem);
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    // Collapse the runs of empty lines pdf-extract leaves between blocks.
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page PDF with a text object, written by hand so the test needs no
    /// fixture binary. Standard-font PDFs carry their text as plain bytes.
    fn minimal_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        let mut objs = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_string(),
            format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, body) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!(
            "xref\n0 {}\n0000000000 65535 f \n",
            objs.len() + 1
        ));
        for off in &offsets {
            pdf.push_str(&format!("{off:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        ));
        objs.clear();
        pdf.into_bytes()
    }

    #[test]
    fn content_type_is_classified_not_guessed() {
        assert_eq!(
            from_content_type("text/html; charset=utf-8"),
            ContentKind::Html
        );
        assert_eq!(from_content_type("TEXT/PLAIN"), ContentKind::Text);
        assert_eq!(from_content_type("application/pdf"), ContentKind::Pdf);
        assert_eq!(from_content_type("application/json"), ContentKind::Text);
        // A binary type must be refused, never decoded as text.
        assert_eq!(
            from_content_type("image/png"),
            ContentKind::Unsupported("image/png".into())
        );
        assert_eq!(
            from_content_type(""),
            ContentKind::Unsupported("unknown".into())
        );
    }

    #[test]
    fn extensions_map_to_kinds() {
        assert_eq!(from_extension("HTML"), ContentKind::Html);
        assert_eq!(from_extension(".pdf"), ContentKind::Pdf);
        assert_eq!(from_extension("md"), ContentKind::Text);
        assert_eq!(from_extension(""), ContentKind::Text);
        assert_eq!(
            from_extension("png"),
            ContentKind::Unsupported("png".into())
        );
    }

    #[test]
    fn magic_bytes_beat_a_lying_type() {
        let pdf = minimal_pdf("hello");
        assert_eq!(from_content_type("text/plain"), ContentKind::Text);
        assert_eq!(sniff(&pdf), Some(ContentKind::Pdf));
        assert_eq!(sniff(b"# plain markdown"), None);
    }

    #[test]
    fn default_converter_handles_html_text_and_refusals() {
        let c = DefaultConverter;
        let md = c
            .convert(b"<h1>Title</h1><p>Body</p>", &ContentKind::Html)
            .unwrap();
        // html2md emits setext headings; the point is that the tags are gone.
        assert!(md.contains("Title") && md.contains("Body"), "{md}");
        assert!(!md.contains("<h1>"), "html was not converted: {md}");
        assert_eq!(
            c.convert(b"plain\n", &ContentKind::Text).unwrap(),
            "plain\n"
        );
        let err = c
            .convert(
                b"\x89PNG\r\n",
                &ContentKind::Unsupported("image/png".into()),
            )
            .unwrap_err();
        assert!(err.contains("image/png"), "{err}");
    }

    #[test]
    fn default_converter_extracts_pdf_text() {
        let c = DefaultConverter;
        let pdf = minimal_pdf("Chunking splits long pages");
        let md = c.convert(&pdf, &ContentKind::Pdf).unwrap();
        assert!(md.contains("Chunking splits long pages"), "{md}");
    }

    #[test]
    fn a_pdf_without_a_text_layer_says_so() {
        let c = DefaultConverter;
        // Header only, no content stream: nothing to extract, and saying so is
        // better than writing an empty page.
        let err = c
            .convert(b"%PDF-1.4\nnot really a pdf\n", &ContentKind::Pdf)
            .unwrap_err();
        assert!(err.contains("pdf"), "{err}");
    }

    #[test]
    fn tidy_joins_wrapped_lines_and_hyphenation() {
        assert_eq!(
            tidy("a long-\nword here\n\n\n\nnext\n"),
            "a longword here\n\nnext"
        );
        // A real hyphen at a line end that is part of a compound is harmless
        // either way; a line ending in two hyphens is left intact.
        assert_eq!(tidy("range --\n"), "range --");
    }
}
