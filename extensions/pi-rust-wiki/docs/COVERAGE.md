# Coverage: zosmaai/pi-llm-wiki → pi-rust-wiki

Every upstream section mapped to its current home. Status: **verbatim** (copied, tool/path names only), **adapted** (remote-storage delta), **dropped** (reason), **added** (ours, no upstream equivalent).

## skills/llm-wiki/SKILL.md ← upstream skills/llm-wiki/SKILL.md

| Upstream section | Status | Home / note |
|---|---|---|
| Architecture (4 layers, `.llm-wiki/`) | adapted | Architecture: flat server vault, spaces, personal layer |
| Golden Rules 1-7 | verbatim | Golden Rules 1-7 |
| Golden Rule 8 (frontmatter mandatory) | added | strict `wiki_write_page` has no upstream equivalent |
| Space guardrails | added | `wiki_use_space` pin, personal write tools |
| Trajectories + memory-tool table | dropped | deferred: needs server types + packets |
| Extension-helps table | adapted | How the Server Helps You (server, not extension) |
| Recall + two-stage links-first | adapted | server-side threshold, personal-layer labels, `wiki_read_page` expand |
| Skills/cases carve-out | dropped | trajectories parked |
| wiki_retro flow | verbatim | At End |
| Deeper searches | adapted | space-scoped note |
| `/wiki-model`, `/wiki-settings`, `/wiki-dashboard` | dropped | host-specific screens |
| Auto-Bootstrap | adapted | `wiki_bootstrap(space)` + `wiki_use_space` |
| Tool list (14+3) | adapted | Available Tools: 18 (see deltas below) |
| Capture → Ingest → Synthesize | adapted | batch text inline, `wiki_template` scaffolds |
| Query → Answer → File | adapted | + query-event logging, synthesis on substantial answers |
| Task → Retro | verbatim | Task → Retro |
| Task → Record → Distill | dropped | trajectories parked |
| Naming, Citations, Contradictions | verbatim | Page Conventions |
| Frontmatter block | adapted | full field set + `wiki_template` as scaffold source |
| Entity/Concept/Skill/Case extras | adapted | skill/case extras dropped (types don't exist) |
| Variants (personal/company) | dropped | spaces + personal layer replace modes |
| Obsidian Integration | adapted | space folder as vault; generated index/log |
| Tips | adapted | server trust; WIKI_SCHEMA.md → `wiki_template` |

## Tool deltas (upstream → ours)

| Upstream | Ours | Reason |
|---|---|---|
| `.llm-wiki/` paths, `read`/`write` tools | `space` param, `wiki_read_page`/`wiki_write_page` | remote storage |
| `wiki_bootstrap(topic, mode)` | `wiki_bootstrap(space)` + `wiki_use_space` | spaces, no modes |
| `read raw/.../extracted.md` | batch `extracted` text inline | no local files |
| `wiki_schema` scaffold | `wiki_template(type)` | server-authoritative templates |
| `wiki_graph`, `wiki_suggest` | — | no graph engine; recall + lint cover |
| trajectory trio | caller-supplied steps + summary | server never sees live session |
| — | personal-layer tools, `wiki_observe`, `wiki_reindex_embeddings` | our additions |

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
