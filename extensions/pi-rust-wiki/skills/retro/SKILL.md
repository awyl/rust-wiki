---
name: retro
description: Distil the session's durable knowledge into the project's rust-wiki vault.
whenToUse: When the user asks to record, retro, or save session learnings after a task (retro) — or when invoked by the autopilot worker.
---

# Retro into the wiki

You are the disciplined maintainer. Decide WHAT is durable; the server
handles mechanics (registry, backlinks, index — rebuilt on every write).

## Procedure

1. **Pin**: `wiki_bootstrap` the session's space (from the nudge;
   idempotent), then `wiki_use_space` with the same space. Never
   `wiki_use_space("personal")` — prohibited; cross-project writes go
   through `wiki_ensure_personal_page` / `wiki_write_personal_page`.
2. **Extract** 2-6 durable items from the session: what was learned or
   decided, type (concept / analysis / synthesis / entity), confidence,
   target slug, create-vs-update.
3. **Write**:
   - Atomic insights → `wiki_retro` (slug + title + body with markdown
     links).
   - Structured pages → `wiki_ensure_page` with real content (never bare
     templates). Updates → `wiki_read_page` + `wiki_write_page` (keep
     frontmatter, full document on write).
   - Cross-link: `[label](/folder/page.md)`; find targets with
     `wiki_search`.
4. **Gate**: `wiki_lint` with `auto_fix: true`; fix what it reports.
5. **Verify**: `wiki_status` — page count grew, health not "empty".
6. Report: pages written, lint result, anything skipped and why.

## Rules

- Show the user what you intend to write for inline runs (the headless
  worker skips this — it is unattended by contract).
- Facts over prose: preserve file paths, error strings, hashes, values.
- One idea per page; link instead of duplicating.
