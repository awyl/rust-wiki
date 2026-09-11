---
name: retro
description: Record session insights to the rust-wiki vault — atomic retros, post-task or mid-session.
whenToUse: When the user asks to record, retro, or save session learnings after a task — or when invoked by the autopilot worker. Entry point to the llm-wiki skill's retro flow.
---

# Retro — Save Session Insights

Full conventions live in the `llm-wiki` skill — this is the task-end entry point the autopilot nudge references.

1. **Pin**: `wiki_bootstrap` the target space (idempotent), then `wiki_use_space`. Personal-layer writes need no switch (`wiki_ensure_personal_page` / `wiki_write_personal_page`).
2. **Retro**: one `wiki_retro(slug, title, body, relevance?)` per atomic insight — facts over prose (paths, errors, hashes), cross-link generously. `relevance` is calibrated, not decorative: **most insights are medium**; reserve `high` for a constraint that will bite again and `critical` for a fact that changes how future work must be done. If most of a run is `high`, demote until the top slice is small.
3. **Observe** (mid-session): `wiki_observe(title, content, relevance)` for running notes — same rubric, `relevance` is required there.
4. **Structured content**: `wiki_ensure_page` for new pages (scaffold via `wiki_template`), read+`wiki_write_page` for updates (fence mandatory). A fence belongs to `wiki_write_page` only: `wiki_retro` takes a body (the tool writes the frontmatter) and `wiki_ensure_page` takes a body or a fenced document — with your own fence, declare `relevance:` inside it and drop the argument.
5. **Gate**: `wiki_lint(auto_fix=true)`, verify with `wiki_status`.
6. **Worker mode** (headless, extraction file as sole context): apply items the same way, notify via `intercom`.

See `/wiki-retro` and `/wiki-ingest` prompts for the full flows.
