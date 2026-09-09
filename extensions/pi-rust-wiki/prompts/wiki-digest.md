---
description: Generate a daily or weekly digest of wiki changes — new sources, pages, insights, and gaps.
argument-hint: "[--period daily|weekly]"
section: LLM Wiki
topLevelCli: true
---

# /wiki-digest

Generate a digest of recent wiki activity. Server files are not directly readable — build the digest from tool calls.

## User Arguments

$ARGUMENTS

The session nudge names the active space; pass it as `space`.

## Steps

1. Call `wiki_status()` for current stats (page count, orphans, gaps, health).
2. Call `wiki_lint(auto_fix=false)` for orphans, missing pages, contradictions, gaps.
3. Read `wiki/log.md` via `wiki_read_page(id="log")` for recent events in the period.
4. Summarize:
   - New sources captured
   - New pages created or updated
   - Key insights or connections made
   - Knowledge gaps identified
   - Health trends (improving, stable, declining)
5. Report the digest in-chat (no server-side outputs/ writes exist — do not invent a save step).
6. Call `wiki_log_event(kind="digest", details={"period": <daily|weekly>})` to record generation.
