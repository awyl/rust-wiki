# Coverage: zosmaai/pi-llm-wiki → pi-rust-wiki

Every upstream section mapped to its current home. Status: **verbatim** (copied, tool/path names only), **adapted** (remote-storage delta), **dropped** (reason), **added** (ours, no upstream equivalent).

Checked against upstream `skills/llm-wiki/SKILL.md` at commit on 2026-09-11 (post-QMD, post-benchmark).

## skills/llm-wiki/SKILL.md ← upstream skills/llm-wiki/SKILL.md

| Upstream section | Status | Home / note |
|---|---|---|
| Architecture (4 layers, `.llm-wiki/`) | adapted | Architecture: flat server vault, spaces, personal layer |
| Golden Rules 1-7 | verbatim | Golden Rules 1-7 |
| Golden Rule 8 (frontmatter mandatory) | added | strict `wiki_write_page` has no upstream equivalent |
| Space guardrails | added | `wiki_use_space` pin, personal write tools |
| QMD `meta/qmd/` note | dropped | we chose chunk embeddings + additive semantic fusion (design-spec) |
| Trajectories + memory-tool table | shipped | Agent Working-Memory: server trio (`wiki_capture_trajectory`/`wiki_distill_skills`/`wiki_recall_skill`), `skill`/`case` types, `/wiki-record` |
| `/wiki-trajectories` opt-in toggle | dropped | tools always registered here; server-side, no system-prompt cost concern |
| Extension-helps table | adapted | How the Server Helps You (server, not extension) |
| Recall + two-stage links-first | adapted | server-side threshold (`WIKI_RECALL_LINKS_FIRST_THRESHOLD`), personal-layer labels, `wiki_read_page` expand |
| Skills/cases carve-out (`recallSkillInlineMax`) | adapted | links-first spares `skill`/`case` previews (const, not config); ours keeps the query-relevant chunk (≤200 ch) vs upstream's ≤1600 ch body inline |
| wiki_retro flow | verbatim | At End — Save Insights with wiki_retro (+ relevance, search-before-write) |
| Deeper searches | adapted | space-scoped note |
| `/wiki-model`, `/wiki-settings`, `/wiki-dashboard` | dropped | host-specific screens; no background task model |
| Auto-Bootstrap | adapted | `wiki_bootstrap(space)` + `wiki_use_space` |
| Tool list (14+3) | adapted | Available Tools: 24 (see deltas below) |
| Capture → Ingest → Synthesize | adapted | batch text inline, `wiki_template` scaffolds, relevance rules, pre-capture `wiki_recall` |
| Query → Answer → File | adapted | + query-event logging, synthesis on substantial answers |
| Task → Retro | verbatim | Task → Retro |
| Task → Record → Distill | shipped | Agent Working-Memory workflows (trajectories) + `/wiki-record` |
| Naming, Citations, Contradictions | verbatim | Page Conventions |
| Frontmatter block | adapted | full field set + `wiki_template` as scaffold source |
| Entity/Concept/Skill/Case extras | adapted | templates carry extras (case `outcome`, skill `trajectories`) |
| Variants (personal/company) | dropped | spaces + personal layer replace modes |
| Obsidian Integration | adapted | space folder as vault; generated index/log |
| Tips | adapted | server trust; WIKI_SCHEMA.md → `wiki_template` |

## Upstream engine/QA assets (not SKILL sections)

| Upstream asset | Ours | Note |
|---|---|---|
| Guardrails (apply-patch scan, mutation guards, ambient gate) | server-enforced | ownership rejects non-`wiki/` writes; fence + wikilink gates; no patch tools exist server-side |
| INGEST_SYSTEM anti-fabrication contract | added | `wiki-ingest.md` Rules: never fabricate, only entities/concepts present, one-line descriptions |
| Retrieval benchmark (22 pages, 60 graded queries) | **gap** | no graded recall harness in rust-wiki — see audit 2026-09-11 |
| QMD engine (SQLite FTS, CJK) | dropped | embeddings + semantic fusion; CJK case unverified |
| `synthesisLanguage`, `synthesisMaxTokens` | dropped | no background synthesis; synchronous agent-side |
| Custom page types (config-driven) | dropped | fixed 9-type PAGE_TYPES is deliberate KISS |

## Tool deltas (upstream → ours)

| Upstream | Ours | Reason |
|---|---|---|
| `.llm-wiki/` paths, `read`/`write` tools | `space` param, `wiki_read_page`/`wiki_write_page` | remote storage |
| `wiki_bootstrap(topic, mode)` | `wiki_bootstrap(space)` + `wiki_use_space` | spaces, no modes |
| `read raw/.../extracted.md` | batch `extracted` text inline | no local files |
| `wiki_schema` scaffold | `wiki_template(type)` | server-authoritative templates |
| `wiki_graph`, `wiki_suggest` | — | no graph engine; recall + lint cover |
| trajectory trio (auto-extract) | caller-supplied steps + summary | server never sees live session |
| `wiki_reindex` (QMD) | `wiki_reindex_embeddings` | embeddings provider |
| — | personal-layer tools, `wiki_observe`, `wiki_delete_page`, `wiki_watch(run)` scheduler | our additions |
| — | `requirement` page type | our addition (upstream lacks it) |

## prompts/ ← upstream prompts/ (== commands/, identical)

| Prompt | Status |
|---|---|
| wiki-query, wiki-ingest, wiki-lint, wiki-status, wiki-init, wiki-retro, wiki-discover | adapted (space param, remote reads, auto-rebuild) |
| wiki-digest | adapted (log via `wiki_read_page(id="log")`, in-chat report — no outputs/ writes) |
| wiki-run | adapted (`wiki_watch(run=true)` instead of crontab print) |
| wiki-record, wiki-skills | adapted (caller-supplied steps — server never sees live session) |
| wiki-req | adapted (`requirement` type: `wiki/requirements/`, status lifecycle, priorities) |

## skills/research, skills/retro

Thin entry points kept for autopilot hook compatibility (nudge footer, worker directive). Canonical brain text lives in `llm-wiki`.

## templates/

`skills/llm-wiki/templates/pages/*.md` mirror the server `TEMPLATES` in `rust-wiki/src/vault/bootstrap.rs`. Server is authoritative at runtime via `wiki_template`; skill copies are reference only. Change server first, mirror here.

## Worker briefs (ours — no upstream equivalent)

`llm-wiki-skills/worker-retro.md`, `worker-discover.md`: background cadence workers (silent, window-clamped evidence, dedup via `wiki_recall`, relevance rubric, entity step, frontmatter-fence ownership rules). Upstream runs these flows in-band via prompts; ours run detached per user directive (memory #31).