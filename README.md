# pi-llm-wiki-autopilot

Autonomous [llm-wiki-skills](https://github.com/geronimo-iia/llm-wiki-skills) triggers for
[pi](https://github.com/badlogic/pi-mono). Requires a connected llm-wiki MCP server (`wiki_*` tools).

## What it does

| Trigger | When | Effect |
|---------|------|--------|
| Bootstrap | `session_start` | Queues a lean directive: agent calls `wiki_info` once — creates the git-derived space if missing, rebuilds the index if degraded. No orientation; research/crystallize orient themselves when invoked |
| Research nudge | every turn | Static 3-line system-prompt footer routing knowledge questions to the `research` skill (cache-safe) |
| Crystallize | every 8 settled agent runs (re-arms; `oncePerSession` pins to first) | Queues a 4-line delegation directive: main agent writes its session extraction to a temp file and fires one detached headless `pi -p` worker (`LLM_WIKI_AUTOPILOT_DISABLE=1`). Static worker instructions live in `worker-crystallize.md` inside the package — directives stay tiny. Worker auto-writes, ingests, rebuilds the index, logs to `/tmp/llm-wiki-crystallize-<wiki>.log`, and intercom-sends a completion line to the main session |

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
  "display": true,
  "wikiRoot": "/data",
  "crystallize": { "enabled": true, "everyNRuns": 8, "oncePerSession": false }
}
```

**`display`:** set to `false` to stop directive text rendering in the UI — the agent still receives it silently (you'll just see its one-line report and the background worker command).

**`wikiRoot`:** parent directory used to create new wiki spaces — bootstrap then creates the space unattended instead of asking you for the parent directory. Leave unset to keep the ask-once behavior.

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
