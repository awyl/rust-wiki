---
description: Ask questions against the wiki. Synthesizes answers from wiki pages with cross-reference citations.
argument-hint: "<question>"
section: LLM Wiki
topLevelCli: true
---

# /wiki-query

Ask a question and get an answer synthesized from wiki content.

## User Question

$ARGUMENTS

Read the LLM Wiki skill first for conventions. The session nudge names the active space; pass it as `space` on every call.

## Steps

1. Call `wiki_recall(query=<question>)` to find relevant wiki pages.
2. Read the full content of each matching page with `wiki_read_page`.
3. Synthesize an answer with `[label](/folder/page.md)` citations to specific wiki pages.
4. If the answer is substantial and worth preserving:
   - Call `wiki_ensure_page(type=synthesis, title=<title>, content=<content>)` to save it
5. Call `wiki_log_event(kind="query", details={"question": <question>})` to log the query.

**Rules:**
- Answer ONLY from wiki content, not from general knowledge.
- If the wiki lacks information, say so clearly and suggest what sources would help fill the gap.
