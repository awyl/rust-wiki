---
description: Run the full wiki cycle: discover → ingest → lint. Optionally trigger the server maintenance run.
argument-hint: ""
section: LLM Wiki
topLevelCli: true
---

# /wiki-run

Run the complete wiki maintenance cycle: discover new sources, ingest them, and lint for health.

## Steps

1. **Discover:** Web-search the wiki's topic, capture each with `wiki_capture_source(url=<url>)` (max 5-10). See `/wiki-discover`.
2. **Ingest:** Call `wiki_ingest(batch_size=3)` and process returned sources — update source pages, create entity/concept pages, add cross-references. See `/wiki-ingest`.
3. **Lint:** Call `wiki_lint(auto_fix=true)` for the health check.
4. If critical gaps found → optionally run one more discover+ingest cycle.
5. For a server-side all-spaces maintenance pass, call `wiki_watch(run=true)`.
6. Report final summary.
