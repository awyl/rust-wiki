---
name: llm-wiki
description: Build and maintain a persistent, interlinked markdown wiki (Karpathy pattern) on the remote rust-wiki server — capture sources, synthesize pages, recall knowledge, save insights. Ported from zosmaai/pi-llm-wiki, retargeted to remote storage.
whenToUse: Call wiki_recall at task start to find relevant wiki pages. Call wiki_retro at task end to save new insights. Use wiki_template for page scaffolds, wiki_ensure_page/wiki_write_page for page writes. The extension injects a brief status line, but explicit calls with task-specific terms get better results.
---

# LLM Wiki (rust-wiki)

You are a disciplined wiki maintainer. The server handles all mechanical work (registry, backlinks, index, projections) — you focus on synthesis, reasoning, and knowledge organization.

Ported from `zosmaai/pi-llm-wiki` `skills/llm-wiki/SKILL.md`. Storage is remote (rust-wiki MCP server), never local files: no vault paths, no `read`/`write` on wiki files. Deltas from upstream are marked `[remote]`; dropped upstream features are listed at the end.

## Architecture

```
<VAULT_ROOT>/                  # server-side, never a local path
├── personal/                  # reserved cross-project layer (auto-created at boot)
└── <space-name>/              # one space per project (`space` param, or pin once)
    ├── config.json            # vault config
    ├── templates/             # page templates (authoritative — see wiki_template)
    ├── raw/                   # immutable captured sources (server-owned, never edit)
    ├── wiki/                  # editable knowledge pages (you own this)
    │   ├── sources/           # one summary per source
    │   ├── entities/          # people, orgs, tools, products
    │   ├── concepts/          # ideas, patterns, frameworks
    │   ├── syntheses/         # cross-cutting analyses
    │   └── analyses/          # durable query answers
    ├── meta/                  # registry, backlinks, index, log, events (server-owned)
    └── outputs/               # generated artifacts
```

## Golden Rules

1. **RAW IS IMMUTABLE.** Never edit `raw/`. Use `wiki_capture_source` to add sources.
2. **META IS SERVER-OWNED.** Never edit `meta/` directly. `events.jsonl` is append-only authoritative activity state; other metadata files are generated projections.
3. **YOU OWN THE WIKI.** Create, update, and cross-reference everything in `wiki/`.
4. **ONE FILE PER THING.** Each entity, concept, source gets its own `.md` file.
5. **CROSS-REFERENCE EVERYTHING.** Every page needs at least 2 links. Prefer standard Markdown: `[label](/folder/page.md)`. Legacy wikilinks `[[folder/page]]` remain readable.
6. **CITE SOURCES.** Every claim links back to its raw source packet.
7. **FLAG CONTRADICTIONS.** When sources disagree, document both sides.
8. **FRONTMATTER IS MANDATORY.** `[remote]` Every page starts with a `---` fence. `wiki_write_page` rejects fenceless content — always `wiki_read_page` first, keep the fence, write the complete file.

> Do not place secrets or private machine paths in page content or event details.

## Space Guardrails `[remote]`

- `wiki_bootstrap` the session's space (idempotent), then `wiki_use_space` with the same space. Pin once per connection; the session nudge names the active space.
- Never `wiki_use_space("personal")` — blocked. Cross-project writes go through `wiki_ensure_personal_page` / `wiki_write_personal_page` (no switch needed).

## How the Server Helps You

| Task                        | Before (manual)              | Now (server-backed)                   |
| --------------------------- | ---------------------------- | ------------------------------------- |
| Track ingestion             | Manual lists                 | Automatic via registry                |
| Update INDEX / LOG          | Manual edit after every page | Auto-rebuilt on every write           |
| Find orphans                | Shell `grep` scans           | `wiki_lint` from backlinks            |
| Block raw edits             | Skill says "don't"           | Server **rejects** non-`wiki/` writes |
| Block fenceless pages       | Silent registry dropout      | `wiki_write_page` **rejects** them    |
| Create source page          | Many steps                   | `wiki_capture_source` + synthesis     |
| **Recall wiki knowledge**   | Never happens                | **Layered search (personal + space)** |
| **Save task insights**      | Manual capture               | `wiki_retro` — one tool call          |

## Wiki Usage

### At Start — Call wiki_recall

**Call `wiki_recall` at the START of every task:**

```
wiki_recall(query="key terms from the user's request", max_results=5)
```

`[remote]` Searches the active space plus the personal cross-project layer, merging results by score. Pass `space` until `wiki_use_space` is pinned.

The extension also searches automatically, but explicit calls with task-specific terms get better results.

#### Two-Stage Recall (links-first for large vaults)

- **Small vaults** (page count ≤ threshold): inline **content previews** — read them directly.
- **Large vaults** (page count > threshold): ranked **links only** (`id`, `title`, `type`, `score`, snippet). Pick what matters, then expand via `wiki_read_page`. Do **not** assume the snippet is the whole page.

`[remote]` The threshold lives server-side. Vault labels mark personal-layer hits.

### At End — Save Insights with wiki_retro

After completing any meaningful task, call `wiki_retro`:
- Non-obvious bug fixes or workarounds
- Architectural decisions and their rationale
- Tool/library gotchas you discovered
- Patterns worth remembering for future sessions

**Do not wait for the user to ask.** One atomic insight per call. Search first with `wiki_recall`: if a page already states the insight, `wiki_write_page` that page instead of adding a near-duplicate under a new slug.

Declare `relevance` (`low|medium|high|critical`) when the insight genuinely is one of those. Recall scales a page's score by it (0.9 / 1.0 / 1.1 / 1.2), so documents of comparable match strength settle in favour of the page that claims importance — and an undeclared page keeps its exact score. Omit it for unremarkable insights; never inflate it.

Record something noticed *mid*-task with `wiki_observe`. It writes a `retro` page under `sources/obs-<date>-<slug>` carrying `relevance` plus optional `tags` / `source_context`.

```
wiki_retro(slug="kebab-case-slug", title="Brief descriptive title", body="Insight in your own words with [links](/folder/page.md)", relevance="high")
```

### Deeper Searches

```
wiki_search(query="broad topic")
```

`[remote]` Searches the active space registry only. For cross-layer search, use `wiki_recall`.

## Available Tools

- `wiki_bootstrap` — Initialize a new vault (`space`)
- `wiki_use_space` — Pin the per-connection space `[remote]`
- `wiki_capture_source` — Capture URL/file/text into immutable packet + skeleton page (`file_path` resolves server-side only; prefer `text`/`url`). Optional `relevance`: low|medium|high|critical
- `wiki_ingest` — Get batch of uningested sources with extracted text inline
- `wiki_ensure_page` — Create entity/concept/synthesis/analysis page from template (no overwrite). Optional `relevance`: low|medium|high|critical — pass it with a body, or omit it and declare `relevance:` inside a fenced document you supply yourself (the two together are rejected)
- `wiki_template` — Authoritative page scaffold per type (`{date}` filled, `{title}` placeholder) `[remote]`
- `wiki_read_page` — Read a page by id `[remote]`
- `wiki_write_page` — Guarded update (requires frontmatter fence) `[remote]`
- `wiki_delete_page` — Delete a page (irreversible; needs operator `allow_delete`, `confirm` repeating the id, and no remaining inbound links unless `force`). Use only on explicit user approval, never from a background worker `[remote]`
- `wiki_ensure_personal_page` / `wiki_write_personal_page` — Personal-layer writes, no switch `[remote]`
- `wiki_recall` — Layered relevance search (space + personal)
- `wiki_capture_trajectory` / `wiki_distill_skills` / `wiki_recall_skill` — Working-memory trio (packets, distillation, skill recall)
- `wiki_search` — Registry keyword search (space only)
- `wiki_retro` — Save an atomic insight (optional `relevance`: low|medium|high|critical)
- `wiki_observe` — Timestamped mid-session note, stored as a `retro` page in `sources/` `[remote]`
- `wiki_lint` — Health check (orphans, missing, contradictions, gaps; `auto_fix`)
- `wiki_status` — Instant stats
- `wiki_rebuild_meta` — Force metadata rebuild (per-space)
- `wiki_log_event` — Record a custom event
- `wiki_watch` — Maintenance scheduler status / immediate run
- `wiki_reindex_embeddings` — Semantic vectors (no-op without provider) `[remote]`

## Workflows

### Capture → Ingest → Synthesize

Before capturing: `wiki_recall` the topic or URL. An existing page covering it gets a `wiki_write_page` update, not a second capture.

1. **Capture**: `wiki_capture_source(url="...")` → packet + skeleton. Pass `relevance="high"|"critical"` only when the source genuinely outranks its peers for future recall; most captures claim nothing
2. **Ingest**: `wiki_ingest()` → batch with extracted text inline `[remote: no local extracted.md]`
3. **Write**: Update skeleton source page with summary, entities, concepts
4. **Scaffold**: `wiki_template(type="concept")` for the exact current scaffold `[remote]`
5. **Ensure**: `wiki_ensure_page(type="entity", title="...")` per entity; `type="concept"` per concept — same `relevance` rule applies
6. **Cross-ref**: Add `[links](/folder/page.md)` between related pages
7. **Done**: Server auto-rebuilds metadata on every write

Crystallize, don't transcribe: shorter and more structured than the input. **Keep:** decisions, patterns, findings, rationale. **Discard:** dead ends, process chat, superseded drafts.

### Query → Answer → File

1. `wiki_recall` with task-specific terms; `wiki_read_page` on the top ids
2. Synthesize with `[link](/folder/page.md)` citations; answer ONLY from wiki content (say so plainly when thin, suggest gap-filling sources)
3. If substantial and novel: `wiki_ensure_page(type="analysis"|"synthesis", ...)` to preserve it
4. `wiki_log_event(kind="query", details={"question": "..."})` to log the query

### Task → Retro

1. Complete a meaningful task
2. `wiki_retro` — one atomic insight per call, facts over prose (paths, errors, hashes)
3. Cross-link; update related pages
4. Next time, layered recall surfaces the insight

## Page Conventions

### Naming

- `kebab-case.md` for all files
- Standard Markdown links: `[label](/concepts/retrieval-augmented-generation.md)`
- Pages live in `wiki/` subdirectories: `concepts/`, `analyses/`, `sources/`, `entities/`, `syntheses/`

### Frontmatter

Scaffold from `wiki_template(type)` — never from frozen copies. Shape:

```yaml
---
type: entity | concept | source | synthesis | analysis | requirement | skill | case
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
Source: add `format: article | paper | note | video | podcast`, `raw_path:`, `ingested:`, `topics: []`
Analysis/Synthesis: add `topic:`, `sources_count:`

Conventions: `title` is the concept name (never `"Paper: ..."`); `tags` flat lowercase hyphenated; `sources` only pages that contributed claims; `concepts` only pages a reader benefits navigating to; `confidence` defaults `0.5` (`0.9` corroborated, `0.2` speculative).

### Citations

Stable source IDs: `[[sources/SRC-2026-04-28-001]]`

### Contradictions

```markdown
> ⚠️ **Contradiction:** Source A claims X, but Source B claims Y.
> See [page-a](/analyses/page-a.md) and [page-b](/analyses/page-b.md).
```

### Accumulation Contract

1. Read first — `wiki_read_page`
2. Preserve list values (`tags`, `sources`, `concepts`) — add, do not replace
3. Update scalars only with clear reason
4. Write the complete file **with fence intact**, verify with `wiki_read_page`

### Avoid

- Fenceless writes (rejected; page unchanged)
- Bare `---` in body content (multi-document rejection)
- `[[slug|label]]` pipes (unsupported — use `[label](/path)`)
- Empty stubs — fill in real content
- One idea per page; link instead of duplicating

## Obsidian Integration

Open the space folder as an Obsidian vault. The server generates `wiki/index.md` (entry page) and `wiki/log.md`; standard Markdown links open natively.

## Tips

- **Start small:** 3-5 sources, let it grow organically
- **Batch efficiently:** Plan all pages for a source, then write them rapidly
- **Trust the server:** Never hand-edit `meta/`, `raw/`, or generated `index.md` / `log.md`

## Agent Working-Memory (Trajectories)

The wiki captures what you *do*, not only what you read. One task = one immutable packet:

```
raw/trajectories/TRJ-*  →  wiki/skills/*  (+ optional wiki/cases/*)
```

- `wiki_capture_trajectory(title, outcome, steps, summary)` — you pass the meaningful tool-call record explicitly (the server never sees the live session). Emits packet + skeleton `cases/` page.
- `wiki_distill_skills()` — undistilled packets with summaries; generalize into `skill` pages via `wiki_ensure_page(type="skill")`, then `wiki_distill_skills(mark_distilled=[...])`.
- `wiki_recall_skill(query, kind)` — "have I done this before?" at task start.

A **skill** generalizes across trajectories ("how I do X"); a **case** is one concrete run. Packets are immutable — edit skill/case pages, never the packet.

## Dropped Upstream Features (deliberate)

- `/wiki-model`, `/wiki-settings`, `/wiki-dashboard` — host-specific screens.
- Background ingest on a task model — always synchronous here.
- `wiki_graph`, `wiki_suggest` — no graph engine; recall + lint cover.
- See `docs/COVERAGE.md` for the section-for-section map.
