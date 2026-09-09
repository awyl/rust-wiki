---
name: retro
description: Distil the session's durable knowledge into the project's rust-wiki vault.
whenToUse: When the user asks to record, retro, or save session learnings after a task (retro) — or when invoked by the autopilot worker. Also use wiki_recall at task start to find relevant wiki pages. Call wiki_retro at task end to save new insights. The extension injects a brief status line, but explicit calls with task-specific terms get better results.
---

# Rust Wiki — Knowledge Management Skill

You are a disciplined wiki maintainer. The server handles mechanics (registry, backlinks, index, projections) — you focus on synthesis, reasoning, and knowledge organization.

## Golden Rules

1. **RAW IS IMMUTABLE.** Never edit `raw/`. Use `wiki_capture_source` to add sources.
2. **META IS SERVER-OWNED.** Never edit `meta/` directly. `events.jsonl` is append-only authoritative activity state; other metadata files are generated projections.
3. **YOU OWN THE WIKI.** Create, update, and cross-reference everything in `wiki/`.
4. **ONE FILE PER THING.** Each entity, concept, source gets its own `.md` file.
5. **CROSS-REFERENCE EVERYTHING.** Every page needs at least 2 links. Prefer standard Markdown: `[label](/folder/page.md)`.
6. **CITE SOURCES.** Every claim links back to its raw source packet.
7. **FLAG CONTRADICTIONS.** When sources disagree, document both sides.

## Available Tools

| Tool | Purpose |
|------|---------|
| `wiki_bootstrap` | Initialize a new vault (one-time, locks scope) |
| `wiki_capture_source` | Capture URL/file/text into immutable packet + skeleton page |
| `wiki_ingest` | Get batch of uningested sources with extracted text |
| `wiki_ensure_page` | Create entity/concept/synthesis/analysis page from template |
| `wiki_read_page` | Read a page's content |
| `wiki_write_page` | Update a page (overwrites) |
| `wiki_retro` | Save an atomic insight (slug + title + body) |
| `wiki_observe` | Record a timestamped mid-session observation |
| `wiki_search` | Search registry for existing pages |
| `wiki_recall` | Layered search (personal + project) for task-relevant pages |
| `wiki_lint` | Health check (orphans, missing, contradictions, gaps) |
| `wiki_status` | Instant stats |
| `wiki_rebuild_meta` | Force metadata rebuild |
| `wiki_log_event` | Record a custom event |
| `wiki_watch` | Run maintenance cycle (lint + auto_fix + status) |
| `wiki_ensure_personal_page` | Create page in personal/root layer (no space switch) |
| `wiki_write_personal_page` | Update page in personal/root layer (no space switch) |

## Workflows

### 1. At Task Start — Recall

Call `wiki_recall` at the START of every task to find relevant wiki pages:

```
wiki_recall(query="key terms from the user's request", max_results=5)
```

This searches both the project wiki and the personal cross-project layer, merging results.

The extension also searches automatically, but explicit calls with task-specific terms get better results.

### 2. Capture → Ingest → Synthesize

For external sources (URLs, files, text):

1. **Capture**: `wiki_capture_source(url="https://...")` → creates immutable packet + skeleton source page
2. **Ingest**: `wiki_ingest()` → get batch of sources needing synthesis (server extracts text inline)
3. **Read**: Read the extracted text from the batch response
4. **Write**: Update skeleton source page with summary, entities, concepts
5. **Ensure**: `wiki_ensure_page(type="entity", title="...")` for each entity discovered
6. **Cross-ref**: Add `[links](/folder/page.md)` between related pages
7. **Done**: Server auto-rebuilds metadata on every write

### 3. Query → Answer → File

When answering questions that could benefit from the wiki:

1. `wiki_recall(query="topic", max_results=5)` — find existing knowledge
2. Read matched pages
3. Synthesize answer with `[link](/folder/page.md)` citations
4. If novel: create analysis page via `wiki_ensure_page(type="analysis")`

### 4. Task → Retro (End of Session)

After completing any meaningful task, save key insights:

1. `wiki_retro(slug="kebab-case-slug", title="Brief title", body="Insight with [links](/folder/page.md)")`
2. One atomic insight per call
3. Facts over prose: preserve file paths, error strings, hashes, config values
4. Cross-link generously

### 5. Retro (Background Worker)

When invoked by the autopilot as a headless worker:

1. Pin: `wiki_bootstrap` the target space (idempotent), then `wiki_use_space`
2. Read the extraction file (your only window into the session)
3. Apply items: retro for insights, ensure_page for structured content, read+write for updates
4. Quality gate: `wiki_lint` with `auto_fix: true`
5. Verify: `wiki_status` — page count grew, health not "empty"
6. Notify main session via `intercom`

## Page Conventions

### Naming

- `kebab-case.md` for all files
- Standard Markdown links: `[label](/concepts/retrieval-augmented-generation.md)`
- Pages live in `wiki/` subdirectories: `concepts/`, `analyses/`, `sources/`, `entities/`, `syntheses/`

### Frontmatter

```yaml
---
type: entity | concept | source | synthesis | analysis
title: "Human-readable title"
status: active | deprecated
created: YYYY-MM-DD
updated: YYYY-MM-DD
tags:
  - tag1
  - tag2
confidence: 0.0-1.0
concepts:
  - concepts/related-concept
sources:
  - sources/SRC-YYYY-MM-DD-NNN
---
```

Entity: add `category: person | organization | tool | project | product`
Concept: add `domain: ai | engineering | business | product | design | personal`

### Citations

Use stable source IDs: `[[sources/SRC-2026-04-28-001]]`

### Contradictions

```markdown
> ⚠️ **Contradiction:** Source A claims X, but Source B claims Y.
> See [page-a](/analyses/page-a.md) and [page-b](/analyses/page-b.md).
```

### Avoid

- Bare `---` in body content (triggers multi-document YAML rejection in parser)
- Wikilinks with pipes `[[slug|label]]` (not supported — use `[label](/path)`)
- Empty template stubs — always fill in real content
- One idea per page; link instead of duplicating

## Space Isolation

- `wiki_use_space` pins once per connection; different space → blocked
- `wiki_use_space("personal")` is always blocked — use `wiki_ensure_personal_page` / `wiki_write_personal_page` instead
- Cross-project items go through personal write tools, never through project-scoped tools

## Git Backing

The vault is git-backed. Commits happen automatically after 5 minutes of write-idle. Pulls rebase with abort-on-conflict (never merge). Push if upstream configured. Git problems surface via `wiki_status` health hints.
