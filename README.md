# pi-llm-wiki-autopilot

Autonomous [llm-wiki-skills](https://github.com/geronimo-iia/llm-wiki-skills) triggers for
[pi](https://github.com/badlogic/pi-mono). Requires a connected llm-wiki MCP server (`wiki_*` tools).

## What it does

| Trigger | When | Effect |
|---------|------|--------|
| Bootstrap | first user message | **Holds the turn for sub-second mechanical MCP calls, then processes the message**: the extension calls the wiki server directly (`wiki_info` → create space via `wikiRoot` if missing → rebuild degraded index). No model involved. Sessions that never receive a message never fire it. Wiki scoping rides the research nudge footer (session-static, cache-safe). Endpoint: `wikiMcpUrl` config key (default = the aiproxy MCP route), token = `$AIPROXY_TOKEN` |
| Research nudge | every turn | Static 3-line system-prompt footer routing knowledge questions to the `research` skill (cache-safe) |
| Crystallize | every 8 settled agent runs (re-arms; `oncePerSession` pins to first) | Queues a 4-line delegation directive that **auto-triggers an idle agent** (`followUp` + `triggerTurn` — no user nudge needed): main agent writes its session extraction to a temp file and fires one detached headless `pi -p` worker (`LLM_WIKI_AUTOPILOT_DISABLE=1`). Static worker instructions live in `worker-crystallize.md` inside the package — directives stay tiny. Worker auto-writes, ingests, rebuilds the index, logs to `/tmp/llm-wiki-crystallize-<wiki>.log`, and intercom-sends a completion line to the main session |

All 17 upstream skills are vendored and load natively — `/skill:research`, `/skill:ingest`, `/skill:lint`, … work without the extension.

## Install

```bash
# after pushing this repo:
pi install git:github.com/<you>/pi-llm-wiki-autopilot
# local development install:
pi install /work
```

## Config

Optional `<project>/.pi/llm-wiki.json` (absent = defaults):

```json
{
  "bootstrap": true,
  "researchNudge": true,
  "display": false,
  "wikiRoot": "/data",
  "wikiMcpUrl": "http://host.containers.internal:9999/mcp/wiki",
  "wikiMcpToken": "AIPROXY_TOKEN",
  "crystallize": { "enabled": true, "everyNRuns": 8, "oncePerSession": false }
}
```

**`display`:** default `false` — directive text (crystallize) is delivered silently; you'll see the agent's one-line report and the background worker command. Set `true` to render directive text in the UI.

**`wikiRoot`:** parent directory used to create new wiki spaces — bootstrap then creates the space unattended instead of asking you for the parent directory. Leave unset to keep the ask-once behavior.

**`wikiMcpUrl`:** the MCP endpoint the extension calls directly for mechanical bootstrap checks (`wiki_info` / `wiki_spaces_create` / `wiki_index_rebuild`). Default `http://host.containers.internal:9999/mcp/wiki` — the aiproxy proxy's per-server route for the wiki server. If your wiki MCP server is exposed elsewhere (standalone `llm-wiki serve --http`, different host/port, direct engine URL), set this to that endpoint:

```json
{ "wikiMcpUrl": "http://localhost:8080/mcp" }
```

Any MCP endpoint that serves the `wiki_*` tools works — point it at the same server your agent's wiki tools use, or the engine's own HTTP interface.

**`wikiMcpToken`:** bearer token for that endpoint. Defaults to `$AIPROXY_TOKEN` when set, otherwise the literal default that matches the stock aiproxy route. For a direct engine endpoint with no auth, set it to an empty string.

**`crystallize.oncePerSession`:** default `false` — crystallize re-arms and fires again after every `everyNRuns` settled runs. Set `true` for the old fire-once-per-session behavior.

**Wiki space naming:** derived per project from git — `<first-commit-subject>-<short-hash>`, e.g. `init rust-wiki` → `rust-wiki-cc79119`. Bootstrap creates the space if it doesn't exist, then scopes every wiki call to it. Not a git repo (or no commits) → the default space is used.

**Env guard:** setting `LLM_WIKI_AUTOPILOT_DISABLE=1` disables all hooks — used by the crystallize worker to avoid recursive firing; set it yourself to turn the autopilot off for a session.

**Global config:** `~/.pi/agent/llm-wiki.json` (honors `PI_CODING_AGENT_DIR`) shares settings across all projects. Layering: defaults ← global ← project, per key; a project file overrides only the keys it sets. A malformed file is skipped with a warning (defaults apply to that layer).

## Recommended: turn on wiki auto-rebuild

The llm-wiki engine commits every page write (`ingest.auto_commit`), which advances the wiki's git HEAD. The search index is stamped against that HEAD, so after any session that wrote pages, the next session's bootstrap reports `index_status: degraded` — and search/lint results are unreliable until a rebuild runs.

One-time setting (global server setting — applies to every wiki space, per-space override is not supported):

```
wiki_config(action: "set", global: true, key: "index.auto_rebuild", value: "true")
```

**Status (2026-09-05):** verified a no-op in llm-wiki 1.0.0 — even with the setting loaded by a freshly restarted engine, the index is not rebuilt on startup, query, or ingest. Harmless to enable (it may be implemented in a later engine version), but do not rely on it: search results stay correct while the status reads "degraded" (it is a stamp/status artifact, not stale data), and the autopilot's lean bootstrap self-heals with a `wiki_index_rebuild` call whenever it finds the index degraded (~33-53 ms at ~12 pages).

## Vendored skills

Snapshot of `geronimo-iia/llm-wiki-skills@main`.
Re-sync: `scripts/vendor.sh <ref>`, then diff `skills/` in git.

## Development

```bash
npm install
npm test          # vitest
npm run typecheck # tsc --noEmit
```
