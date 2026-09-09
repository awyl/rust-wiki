//! Bootstrap: create a space's vault structure (mechanical, no LLM).
//! Idempotent: an existing vault is left untouched (`created: false`).

use std::fs;
use std::path::Path;

use serde::Serialize;

use super::layout::VaultPaths;

#[derive(Debug)]
pub struct BootstrapError(pub String);

impl std::fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for BootstrapError {}

#[derive(Debug, Serialize, PartialEq)]
pub struct BootstrapResult {
    pub created: bool,
    pub space: String,
}

#[derive(Debug, Serialize)]
struct VaultConfig<'a> {
    space: &'a str,
    mode: &'a str,
    knowledge_format: &'a str,
    created_at: String,
}

/// Obsidian-friendly orientation page: open the space folder as a vault.
const SPACE_README: &str = "# Wiki space

Open this folder in Obsidian (or any Markdown editor) — pages are plain
Markdown with standard links, and `wiki/index.md` is the generated
entry point.

Layout:

- `wiki/` — editable knowledge pages (`concepts/`, `entities/`,
  `syntheses/`, `analyses/`, `sources/`). `wiki/index.md`, directory
  `index.md` files, and `wiki/log.md` are generated — do not edit.
- `raw/` — immutable captured sources. Do not edit.
- `meta/` — server-owned registry and event stream. Do not edit.
- `templates/` — page templates used on creation.
";

// Ported from zosmaai/pi-llm-wiki skills/llm-wiki/templates/pages/*.md.
// Adaptations: `{title}`/`{date}` placeholders, `raw/sources/` paths (flat
// layout, no `.llm-wiki/` nesting), plus our `status`/`tags`/`confidence` lines.
const TEMPLATES: &[(&str, &str)] = &[
    (
        "concept",
        "---\ntype: concept\ntitle: \"{title}\"\nstatus: active\ndomain: ai\ncreated: {date}\nupdated: {date}\ntags: []\nconfidence: 0.5\nconcepts: []\nsources: []\n---\n\n# {title}\n\nOne-line definition of this concept.\n\n## Definition\n\n\n## How It Works\n\n\n## Examples\n\n\n## Related Concepts\n\n\n## Sources\n\n",
    ),
    (
        "entity",
        "---\ntype: entity\ntitle: \"{title}\"\nstatus: active\ncategory: tool\ncreated: {date}\nupdated: {date}\ntags: []\nconfidence: 0.5\nconcepts: []\nsources: []\n---\n\n# {title}\n\nOne-line description of who/what this is and why they matter.\n\n## Overview\n\n\n## Key Facts\n\n\n## Links\n\n\n## Sources\n\n",
    ),
    (
        "source",
        "---\ntype: source\ntitle: \"{title}\"\nstatus: active\nformat: article\nraw_path: \ningested: {date}\ntopics: []\ncreated: {date}\nupdated: {date}\nconfidence: 0.5\nconcepts: []\n---\n\n# {title}\n\n## Summary\n\n\n## Key Takeaways\n\n\n## Entities Mentioned\n\n\n## Concepts Mentioned\n\n\n## Notable Quotes\n\n\n## Connections\n\n",
    ),
    (
        "analysis",
        "---\ntype: analysis\ntitle: \"{title}\"\nstatus: active\ntopic: \"\"\ncreated: {date}\nupdated: {date}\ntags: []\nconfidence: 0.5\nsources: []\nsources_count: 0\n---\n\n# {title}\n\n> _Durable answer derived from wiki content._\n\n## Question\n\n\n## Answer\n\n\n## Key Insights\n\n\n## Sources Used\n\n\n## Related Pages\n\n",
    ),
    (
        "synthesis",
        "---\ntype: synthesis\ntitle: \"{title}\"\nstatus: active\ntopic: \"\"\ncreated: {date}\nupdated: {date}\ntags: []\nconfidence: 0.5\nsources: []\nsources_count: 0\n---\n\n# {title}\n\n## Question\n\n\n## Analysis\n\n\n## Key Insights\n\n\n## Conclusion\n\n\n## Sources Used\n\n\n## Related Pages\n\n",
    ),
];

/// Create the vault at `vault` if missing. Existing files are never overwritten.
pub fn bootstrap(vault: &VaultPaths, now_iso: &str) -> Result<BootstrapResult, BootstrapError> {
    if vault.config_file().exists() {
        return Ok(BootstrapResult {
            created: false,
            space: space_name(vault),
        });
    }
    let dirs = [
        vault.templates(),
        vault.raw_sources(),
        vault.wiki_pages().join("sources"),
        vault.wiki_pages().join("entities"),
        vault.wiki_pages().join("concepts"),
        vault.wiki_pages().join("syntheses"),
        vault.wiki_pages().join("analyses"),
        vault.meta(),
        vault.outputs(),
        vault.discoveries(),
    ];
    for d in &dirs {
        fs::create_dir_all(d)
            .map_err(|e| BootstrapError(format!("create_dir_all {}: {e}", d.display())))?;
    }
    let config = VaultConfig {
        space: &space_name(vault),
        mode: "personal",
        knowledge_format: "okf-0.2",
        created_at: now_iso.to_string(),
    };
    let config_json = serde_json::to_string_pretty(&config).unwrap();
    write_if_absent(&vault.config_file(), &config_json)?;
    write_if_absent(&vault.registry_file(), "{\"pages\":{}}")?;
    write_if_absent(&vault.backlinks_file(), "{}")?;
    write_if_absent(&vault.events_file(), "")?;
    write_if_absent(&vault.index_file(), "# Index\n")?;
    write_if_absent(&vault.log_file(), "# Log\n")?;
    for (name, body) in TEMPLATES {
        write_if_absent(&vault.templates().join(format!("{name}.md")), body)?;
    }
    write_if_absent(&vault.space_root.join("README.md"), SPACE_README)?;
    Ok(BootstrapResult {
        created: true,
        space: space_name(vault),
    })
}

fn space_name(vault: &VaultPaths) -> String {
    vault
        .space_root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn write_if_absent(path: &Path, content: &str) -> Result<(), BootstrapError> {
    if !path.exists() {
        fs::write(path, content)
            .map_err(|e| BootstrapError(format!("write {}: {e}", path.display())))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_flat_layout_once_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let v = VaultPaths::new(tmp.path(), "proj-x");
        let r1 = bootstrap(&v, "2026-09-06T00:00:00Z").unwrap();
        assert!(r1.created);
        assert!(v.config_file().exists());
        assert!(v.registry_file().exists());
        assert!(v.templates().join("concept.md").exists());
        assert!(v.raw_sources().is_dir());
        assert!(v.discoveries().is_dir());
        assert!(v.space_root.join("README.md").exists());
        // flat: config sits directly in the space dir
        assert_eq!(v.config_file(), tmp.path().join("proj-x/config.json"));
        // second run: no-op, files untouched
        let reg = std::fs::read_to_string(v.registry_file()).unwrap();
        let r2 = bootstrap(&v, "2026-09-07T00:00:00Z").unwrap();
        assert!(!r2.created);
        assert_eq!(std::fs::read_to_string(v.registry_file()).unwrap(), reg);
    }
}
