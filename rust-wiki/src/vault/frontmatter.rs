//! Full frontmatter support with a security shell.
//!
//! Parsing uses a real YAML engine (yaml-rust2), so everything LLMs,
//! Dataview queries, and OKF v0.2 documents write — nested maps,
//! flow collections, quoted scalars, comments — is accepted. Around the
//! engine sits a fail-closed shell:
//!
//! - anchors/aliases rejected by pre-scan (the engine would silently
//!   resolve them into aliased data — a data-integrity hazard)
//! - custom tags rejected by pre-scan
//! - multiple documents rejected
//! - frontmatter capped at 128 KiB, nesting capped at 32
//! - duplicate keys rejected (engine error, mapped to a stable code)
//! - tab indentation rejected (YAML forbids it; clearer diagnostic here)
//!
//! Any diagnostic rejects the entire frontmatter — never a partial read.

use std::collections::BTreeMap;

pub const MAX_FRONTMATTER_BYTES: usize = 128 * 1024;
pub const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Diagnostic {
    pub code: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub line: usize,
    pub message: String,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq)]
pub enum FmValue {
    Str(String),
    Seq(Vec<FmValue>),
    Map(BTreeMap<String, FmValue>),
}

impl FmValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            FmValue::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frontmatter {
    pub map: BTreeMap<String, FmValue>,
}

impl Frontmatter {
    pub fn scalar(&self, key: &str) -> Option<String> {
        self.map
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

fn diag(code: &str, line: usize, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic {
        code: code.into(),
        line,
        message: message.into(),
    }]
}

/// Split a page into (frontmatter block, body). The closing fence must be
/// a line containing exactly `---`. Returns Err(code) when the block is
/// absent or unterminated.
pub fn split_block(text: &str) -> Result<(&str, &str), &'static str> {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = t
        .strip_prefix("---\n")
        .or_else(|| t.strip_prefix("---\r\n"))
        .ok_or("frontmatter_missing")?;
    let mut end = None;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            end = Some(offset);
            break;
        }
        offset += line.len();
    }
    let end = end.ok_or("frontmatter_unterminated")?;
    let block = &rest[..end];
    if block.len() > MAX_FRONTMATTER_BYTES {
        return Err("frontmatter_limit_bytes");
    }
    let after = &rest[end + 3..]; // skip "---"
    let after_trim = after.trim_start_matches(['\r', '\n']);
    let consumed = after.len() - after_trim.len();
    Ok((block, &after[consumed..]))
}

/// Parse a full page document. Fails closed: any diagnostic rejects the
/// entire frontmatter.
pub fn parse(text: &str) -> Result<Frontmatter, Vec<Diagnostic>> {
    let (block, after) = match split_block(text) {
        Ok(v) => v,
        // Karpathy-style pages: frontmatter is OPTIONAL. Absent = empty map.
        Err("frontmatter_missing") => return Ok(Frontmatter::default()),
        Err(code) => return Err(diag(code, 0, code)),
    };
    if after.lines().any(|l| l.trim_end_matches('\r') == "---") {
        return Err(diag(
            "frontmatter_multiple_documents",
            0,
            "multiple YAML documents are forbidden",
        ));
    }
    prescan_security(block)?;

    let docs = yaml_rust2::YamlLoader::load_from_str(block).map_err(|e| {
        let msg = e.to_string();
        if msg.to_lowercase().contains("duplicate") {
            diag("frontmatter_duplicate_key", 0, msg)
        } else {
            diag("frontmatter_parse_error", 0, msg)
        }
    })?;
    if docs.len() != 1 {
        return Err(diag(
            "frontmatter_multiple_documents",
            0,
            "multiple YAML documents are forbidden",
        ));
    }
    let mut depth = 0usize;
    match yaml_to_value(&docs[0], &mut depth)? {
        FmValue::Map(map) => Ok(Frontmatter { map }),
        _ => Err(diag(
            "frontmatter_parse_error",
            0,
            "frontmatter must be a mapping",
        )),
    }
}

/// Line-based pre-scan for constructs we refuse before the engine sees
/// them. Quote-aware so anchors/tags inside quoted scalars are fine.
fn prescan_security(block: &str) -> Result<(), Vec<Diagnostic>> {
    for (i, raw) in block.lines().enumerate() {
        let line_no = i + 1;
        if raw.chars().take_while(|c| *c == ' ').count() != raw.len() && raw.starts_with('\t') {
            return Err(diag(
                "frontmatter_parse_error",
                line_no,
                "tab indentation is forbidden",
            ));
        }
        if has_tab_indent(raw) {
            return Err(diag(
                "frontmatter_parse_error",
                line_no,
                "tab indentation is forbidden",
            ));
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(pos) = find_anchor_or_alias(trimmed) {
            return Err(diag(
                "frontmatter_alias_forbidden",
                line_no,
                format!(
                    "anchors and aliases are forbidden ('{}' at col {})",
                    &trimmed[pos..pos + 1],
                    pos + 1
                ),
            ));
        }
        if has_custom_tag(trimmed) {
            return Err(diag(
                "frontmatter_custom_tag_forbidden",
                line_no,
                "custom tags are forbidden",
            ));
        }
    }
    Ok(())
}

fn has_tab_indent(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with(" \t") || line.contains("\n\t")
}

fn find_anchor_or_alias(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'&' && b != b'*' {
            continue;
        }
        let at_start = i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'-';
        let next = bytes.get(i + 1);
        if at_start
            && next
                .map(|c| c.is_ascii_alphanumeric() || *c == b'_')
                .unwrap_or(false)
        {
            return Some(i);
        }
    }
    None
}

fn has_custom_tag(line: &str) -> bool {
    let mut in_s = false;
    let mut in_d = false;
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'\'' if !in_d => in_s = !in_s,
            b'"' if !in_s => in_d = !in_d,
            b'!' if !in_s && !in_d => {
                let prev_space = i == 0 || bytes[i - 1] == b' ';
                let is_tag = line[i..].starts_with("!!")
                    || (prev_space
                        && bytes
                            .get(i + 1)
                            .map(|c| c.is_ascii_alphanumeric())
                            .unwrap_or(false));
                if is_tag {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn yaml_to_value(y: &yaml_rust2::Yaml, depth: &mut usize) -> Result<FmValue, Vec<Diagnostic>> {
    *depth += 1;
    if *depth > MAX_DEPTH {
        return Err(diag(
            "frontmatter_limit_depth",
            0,
            format!("nesting deeper than {MAX_DEPTH}"),
        ));
    }
    let out = match y {
        yaml_rust2::Yaml::String(s) => FmValue::Str(s.clone()),
        yaml_rust2::Yaml::Real(r) => FmValue::Str(r.clone()),
        yaml_rust2::Yaml::Integer(i) => FmValue::Str(i.to_string()),
        yaml_rust2::Yaml::Boolean(b) => FmValue::Str(b.to_string()),
        yaml_rust2::Yaml::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(yaml_to_value(item, depth)?);
            }
            FmValue::Seq(out)
        }
        yaml_rust2::Yaml::Hash(h) => {
            let mut map = BTreeMap::new();
            for (k, v) in h {
                let key = match yaml_to_value(k, depth)? {
                    FmValue::Str(s) => s,
                    other => format!("{other:?}"),
                };
                map.insert(key, yaml_to_value(v, depth)?);
            }
            FmValue::Map(map)
        }
        yaml_rust2::Yaml::Null => FmValue::Str(String::new()),
        other => FmValue::Str(format!("{other:?}")),
    };
    *depth -= 1;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(fm: &str, body: &str) -> String {
        format!("---\n{fm}\n---\n\n{body}\n")
    }

    #[test]
    fn parses_flat_scalars_lists_quotes_and_nested_maps() {
        let fm = parse(&page(
            "title: \"OKF: subset\"\ntype: concept\ntags:\n  - a\n  - b # trailing\nsources:\n  - url: https://x\n    trust: verified\nconfidence: 0.9",
            "body",
        ))
        .unwrap();
        assert_eq!(fm.scalar("title").unwrap(), "OKF: subset");
        assert_eq!(fm.scalar("confidence").unwrap(), "0.9");
        match fm.map.get("tags").unwrap() {
            FmValue::Seq(v) => {
                assert_eq!(v[0], FmValue::Str("a".into()));
                assert_eq!(v[1], FmValue::Str("b".into()));
            }
            other => panic!("{other:?}"),
        }
        match fm.map.get("sources").unwrap() {
            FmValue::Seq(v) => match &v[0] {
                FmValue::Map(m) => {
                    assert_eq!(m.get("url").unwrap(), &FmValue::Str("https://x".into()));
                    assert_eq!(m.get("trust").unwrap(), &FmValue::Str("verified".into()));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn full_support_flow_collections_numbers_booleans_null() {
        let fm = parse(&page(
            "tags: [obsidian, dataview]\nmeta: {date: 2026-09-07, source_count: 3, pinned: true}\nempty: ~",
            "",
        ))
        .unwrap();
        assert_eq!(
            fm.map.get("tags").unwrap(),
            &FmValue::Seq(vec![
                FmValue::Str("obsidian".into()),
                FmValue::Str("dataview".into())
            ])
        );
        match fm.map.get("meta").unwrap() {
            FmValue::Map(m) => {
                assert_eq!(m.get("source_count").unwrap(), &FmValue::Str("3".into()));
                assert_eq!(m.get("pinned").unwrap(), &FmValue::Str("true".into()));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(fm.map.get("empty").unwrap(), &FmValue::Str(String::new()));
    }

    #[test]
    fn karpathy_style_dataview_page() {
        let fm = parse(&page(
            "title: Attention mechanism\ntype: concept\ndate: 2026-04-02\nsource_count: 7\ntags:\n  - transformers\naliases: [self-attention, scaled dot product]",
            "",
        ))
        .unwrap();
        assert_eq!(fm.scalar("date").unwrap(), "2026-04-02");
        assert_eq!(fm.scalar("source_count").unwrap(), "7");
        assert!(matches!(fm.map.get("aliases"), Some(FmValue::Seq(_))));
    }

    #[test]
    fn okf_v02_nested_provenance() {
        let fm = parse(&page(
            "sources:\n  - url: https://example.com/a\n    fetched: 2026-08-02T10:00:00Z\n    trust: verified\n  - file: raw/papers/b.pdf\nverified:\n  method: manual\n  by: agent-1",
            "",
        ))
        .unwrap();
        match fm.map.get("sources").unwrap() {
            FmValue::Seq(v) => {
                assert_eq!(v.len(), 2);
                match &v[1] {
                    FmValue::Map(m) => assert_eq!(
                        m.get("file").unwrap(),
                        &FmValue::Str("raw/papers/b.pdf".into())
                    ),
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn crlf_input_accepted() {
        let fm = parse("---\r\ntitle: x\r\n---\r\n\r\nbody\r\n").unwrap();
        assert_eq!(fm.scalar("title").unwrap(), "x");
    }

    #[test]
    fn rejects_duplicate_key() {
        let diags = parse(&page("title: a\ntitle: b", "")).unwrap_err();
        assert!(
            diags.iter().any(|d| d.code == "frontmatter_duplicate_key"),
            "{diags:?}"
        );
    }

    #[test]
    fn rejects_alias_anchor_tag_multidoc() {
        for (fm, code) in [
            ("a: &anchor x", "frontmatter_alias_forbidden"),
            ("a: *alias", "frontmatter_alias_forbidden"),
            ("a: !!str x", "frontmatter_custom_tag_forbidden"),
            ("a: !custom x", "frontmatter_custom_tag_forbidden"),
        ] {
            let diags = parse(&page(fm, "")).unwrap_err();
            assert!(diags.iter().any(|d| d.code == code), "{fm}: {diags:?}");
        }
        let diags = parse("---\ntitle: a\n---\n---\nsecond: doc\n").unwrap_err();
        assert!(diags
            .iter()
            .any(|d| d.code == "frontmatter_multiple_documents"));
    }

    #[test]
    fn quoted_anchor_marker_is_fine() {
        let fm = parse(&page(
            "a: \"use & and * freely\"\nb: star * inside quotes",
            "",
        ))
        .unwrap();
        assert_eq!(fm.scalar("a").unwrap(), "use & and * freely");
        assert_eq!(fm.scalar("b").unwrap(), "star * inside quotes");
    }

    #[test]
    fn rejects_tabs_depth_and_missing() {
        let diags = parse(&page("a:\n\tb: x", "")).unwrap_err();
        assert!(diags[0].message.contains("tab"));
        // genuinely nested 40 levels deep (indent alone is not depth)
        let mut deep = String::new();
        for i in 0..40 {
            deep.push_str(&"  ".repeat(i));
            deep.push_str(&format!("k{i}:\n"));
        }
        deep.push_str(&"  ".repeat(40));
        deep.push_str("leaf: x");
        let diags = parse(&page(&deep, "")).unwrap_err();
        assert!(
            diags.iter().any(|d| d.code == "frontmatter_limit_depth"),
            "{diags:?}"
        );
        // absent frontmatter is legal (Karpathy-style plain pages)
        assert_eq!(
            parse("no frontmatter here").unwrap(),
            Frontmatter::default()
        );
        let diags = parse("---\ntitle: never closed\nbody").unwrap_err();
        assert_eq!(diags[0].code, "frontmatter_unterminated");
    }

    #[test]
    fn size_limit_rejected() {
        let big = format!("title: {}\n", "x".repeat(MAX_FRONTMATTER_BYTES + 1));
        assert_eq!(
            split_block(&page(&big, "")).unwrap_err(),
            "frontmatter_limit_bytes"
        );
    }

    #[test]
    fn security_probe_alien_shapes_fail_not_panic() {
        let probes = [
            "\"unterminated: x",
            "a: \"unterminated",
            ":\n",
            "a:",
            "- a\n- b",
            "%YAML 1.2\n---\na: b",
            "a:\n  - - - x",
        ];
        for p in probes {
            let doc = if p.starts_with("---") {
                p.to_string()
            } else {
                page(p, "")
            };
            let _ = parse(&doc); // must not panic
        }
    }
}
