---
name: research
description: Search the project's rust-wiki vault and synthesize an answer from existing knowledge.
whenToUse: When a question might be answered from recorded wiki knowledge instead of memory or code exploration. Entry point to the llm-wiki skill's recall flow.
---

# Research the Wiki

Answer from the vault, not from memory. Full conventions live in the `llm-wiki` skill — this is the recall-first entry point the autopilot nudge references.

1. **Pin**: `wiki_bootstrap` the session's space (from the nudge; idempotent), then `wiki_use_space`. Never `wiki_use_space("personal")`.
2. **Recall**: `wiki_recall` with the user's question (layered: space + personal). Read previews.
3. **Broaden if thin**: `wiki_search` with key terms; try `type` filters.
4. **Read**: `wiki_read_page` on the top ids; follow links.
5. **Synthesize**: answer citing page ids. If nothing relevant, say so — do not invent.
6. **Log**: `wiki_log_event(kind="query", details={"question": "..."})`.
7. **Gap?** Note it as a retro candidate — do not write pages during research.

Keep it to 2-5 tool calls unless genuinely broad. See `/wiki-query` prompt for the full flow.
