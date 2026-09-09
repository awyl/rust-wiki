---
description: Show wiki health overview — source count, page stats, orphan count, last activity dates.
argument-hint: ""
section: LLM Wiki
topLevelCli: true
---

# /wiki-status

Show a quick overview of wiki health and statistics.

## Steps

1. Call `wiki_status(space=<active space>)` to get the current wiki health report.
2. Present the results to the user.
3. If health shows warnings (orphans, gaps), suggest running `/wiki-lint` for a detailed analysis.
