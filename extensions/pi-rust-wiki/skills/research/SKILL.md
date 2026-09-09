---
name: research
description: Search the project's rust-wiki vault and synthesize an answer from existing knowledge.
whenToUse: When a question might be answered from recorded wiki knowledge instead of memory or code exploration. Also called at task start via wiki_recall.
---

# Research the Wiki

Answer from the vault, not from memory. The session nudge names the active space; pass it as `space` on every call.

## Procedure

1. **Pin**: `wiki_bootstrap` the session's space (from the nudge; idempotent), then `wiki_use_space` with the same space. Never `wiki_use_space("personal")` — prohibited; cross-project writes go through `wiki_ensure_personal_page` / `wiki_write_personal_page`.
2. **Recall first**: `wiki_recall` with the user's question as the query (layered search — space + personal layer). Read the previews.
3. **Broaden if thin**: `wiki_search` with key terms; try `type` filters (concept / analysis / synthesis / entity).
4. **Read what matters**: `wiki_read_page` on the top ids. Follow links between pages when they look relevant.
5. **Synthesize**: answer citing page ids, e.g. (see `concepts/prompt-cache-safety`). If the vault has nothing relevant, say so plainly — do not invent wiki content.
6. **Gap?** If the question revealed durable knowledge the vault lacks, note it to the user as a retro candidate — do not write pages yourself during research.

Keep it to 2-5 tool calls unless the question is genuinely broad.
