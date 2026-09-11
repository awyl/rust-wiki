---
description: Process new source packets and synthesize them into wiki knowledge pages.
argument-hint: "[source_id]"
section: LLM Wiki
topLevelCli: true
---

# /wiki-ingest

Process uningested source packets and synthesize them into wiki knowledge pages.

## User Arguments

$ARGUMENTS

Read the LLM Wiki skill first for conventions. The session nudge names the active space; pass it as `space` on every call.

## Steps

1. Call `wiki_ingest(source_id=<id if provided>, batch_size=3)`.
2. If the tool reports all sources ingested, inform the user and stop.
3. Otherwise, for each source in the returned batch (extracted text is inline):
   a. Update the skeleton source page in `wiki/sources/` with a proper summary, key entities, and concepts
   b. Scaffold with `wiki_template(type="entity")` / `wiki_template(type="concept")` — never from frozen copies
   c. Use `wiki_ensure_page(type="entity", title=<name>)` for each new entity (people, orgs, tools, products)
   d. Use `wiki_ensure_page(type="concept", title=<name>)` for each new concept (ideas, patterns, frameworks)
   e. Add `[label](/folder/page.md)` cross-references between related pages
   f. Flag any contradictions with existing wiki content using `⚠️ **Contradiction**` markers
4. Server auto-rebuilds metadata on every write — no manual step needed.
5. Repeat `wiki_ingest()` until the queue is empty.
6. Report: "Ingested [N] sources → [M] pages created/updated. [X] contradictions flagged."

**Rules:**
- Never modify files in `raw/` — source packets are immutable after capture.
- **Never fabricate.** Only include entities and concepts actually present in the extracted text; nothing invented to look thorough. A faithful 2-3 paragraph summary beats a padded one.
- Keep entity/concept descriptions to **one line** — depth lives in the concept page, not the registry blurb.
- Always cite sources with `[[sources/SRC-...]]`.
- Inside Markdown table cells, never use `[[target|alias]]` pipes (unsupported).
