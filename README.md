# pi-llm-wiki-autopilot

Autonomous [llm-wiki-skills](https://github.com/geronimo-iia/llm-wiki-skills) triggers for
[pi](https://github.com/badlogic/pi-mono). Requires a connected llm-wiki MCP server (`wiki_*` tools).

## What it does

| Trigger | When | Effect |
|---------|------|--------|
| Bootstrap | `session_start` | Queues a directive: agent derives the project's wiki space from git history, creates it if missing, reads `skills/bootstrap/SKILL.md`, orients via `wiki_info` + config + hub pages |
| Research nudge | every turn | Static 3-line system-prompt footer routing knowledge questions to the `research` skill (cache-safe) |
| Crystallize | after 8 settled agent runs (once per session) | Queues a proposal: agent distils the session per `skills/crystallize/SKILL.md` and asks before writing |

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
  "crystallize": { "enabled": true, "everyNRuns": 8 }
}
```

**Wiki space naming:** derived per project from git — `<first-commit-subject>-<short-hash>`, e.g. `init rust-wiki` → `rust-wiki-cc79119`. Bootstrap creates the space if it doesn't exist, then scopes every wiki call to it. Not a git repo (or no commits) → the default space is used.

**Global config:** `~/.pi/agent/llm-wiki.json` (honors `PI_CODING_AGENT_DIR`) shares settings across all projects. Layering: defaults ← global ← project, per key; a project file overrides only the keys it sets. A malformed file is skipped with a warning (defaults apply to that layer).

## Vendored skills

Snapshot of `geronimo-iia/llm-wiki-skills@main`.
Re-sync: `scripts/vendor.sh <ref>`, then diff `skills/` in git.

## Development

```bash
npm install
npm test          # vitest
npm run typecheck # tsc --noEmit
```
