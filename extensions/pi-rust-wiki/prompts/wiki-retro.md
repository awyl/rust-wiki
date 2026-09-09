---
description: Save an atomic insight from the current task into the wiki. Creates a single markdown file that layered recall surfaces in future sessions.
argument-hint: "<title> [--category <category>]"
section: LLM Wiki
topLevelCli: true
---

# /wiki-retro

Save an atomic insight from a completed task into the wiki.

## User Arguments

$ARGUMENTS

Read the LLM Wiki skill first for conventions. The session nudge names the active space; pass it as `space`.

## Steps

1. Identify the key insight(s) from the current task — non-obvious learnings, patterns, or decisions worth preserving
2. For each insight, call `wiki_retro` with:
   - `slug`: unique kebab-case identifier (e.g., `jwt-revocation-pattern`)
   - `title`: short descriptive phrase, ≤60 chars, noun phrase not a sentence
   - `body`: markdown explanation with `[label](/folder/page.md)` links to related wiki pages
   - `category`: optional (frontend, architecture, devops, bugfix, design, etc.)
3. Confirm the insight was saved and will be surfaced by layered recall in future sessions
4. If the insight relates to existing wiki pages, update those pages with cross-references (`wiki_read_page` first, keep the fence)

**Rules:**
- One atomic insight per `wiki_retro` call. Use multiple calls for multiple insights.
- Don't save obvious things. Save non-obvious patterns, tradeoffs, and design decisions.
- Always link the new insight to existing wiki knowledge.
- Inside Markdown table cells, never use `[[target|alias]]` pipes (unsupported).
