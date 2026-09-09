---
description: Auto-discover new sources from the web. Searches by topic and known knowledge gaps, captures packets for later ingest.
argument-hint: "[--topic <topic>]"
section: LLM Wiki
topLevelCli: true
---

# /wiki-discover

Find new source material for the wiki by searching the web and capturing it as source packets.

## User Arguments

$ARGUMENTS

The session nudge names the active space; pass it as `space`.

## Steps

1. Call `wiki_status()` to check current page counts and gaps.
2. Use `wiki_search(query=<topic>)` to find existing pages and identify what's already covered.
3. Search the web for new sources:
   - If `--topic` is specified in `$ARGUMENTS`, focus on that topic
   - Otherwise, search for the wiki's main topic + "latest", "news", "update"
4. For each promising result (max 5-10):
   a. Call `wiki_capture_source(url=<url>)` to capture it as an immutable source packet
   b. Skip ads, listicles, and duplicates — prefer in-depth analysis
5. Report: "Discovered [N] new sources captured as packets. Run `/wiki-ingest` to synthesize them into knowledge pages."

**Rules:**
- Do NOT write to `raw/` — always use `wiki_capture_source`.
- Server auto-rebuilds metadata on capture.
