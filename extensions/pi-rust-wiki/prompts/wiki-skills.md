---
description: Search the wiki's distilled skills and past cases for patterns relevant to the current task — "have I done something like this before?".
argument-hint: "[query] [--kind skill|case]"
section: LLM Wiki
topLevelCli: true
---

# /wiki-skills

Search the agent working-memory layer: reusable **skills** distilled from past trajectories, and specific past **cases**.

## User Arguments

$ARGUMENTS

The session nudge names the active space; pass it as `space`.

## Steps

1. Call `wiki_recall_skill` with:
   - `query`: current task description or key terms (defaults to `$ARGUMENTS`)
   - `kind`: optional — `skill`, `case`, or `any` (default)
   - `max_results`: optional (default 5)
2. Read the most relevant skill/case pages with `wiki_read_page`.
3. Apply the recalled pattern, citing the source page where helpful.
4. If nothing relevant exists, proceed — consider `/wiki-record` afterward so the next attempt benefits.

**Tip:** Skills generalize across trajectories ("how I do X"); cases are concrete past runs ("the time I did X"). Search `any` first, then narrow.
