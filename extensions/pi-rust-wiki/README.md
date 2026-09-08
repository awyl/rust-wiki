# pi-rust-wiki

Autonomous [rust-wiki](../rust-wiki/) skill triggers for
[pi](https://github.com/badlogic/pi-mono). Requires a running rust-wiki MCP server (`wiki_*` tools).

## What it does

| Trigger | When | Effect |
|---------|------|--------|
| Bootstrap | first user message | **Holds the turn for sub-second mechanical MCP calls, then processes the message**: the extension calls the wiki server directly (`wiki_use_space` → `wiki_bootstrap` if the space is missing). No model involved. Sessions that never receive a message never fire it. Wiki scoping rides the research nudge footer (session-static, cache-safe). Endpoint: `wikiMcpUrl` config key (default = the local rust-wiki server), token = `$WIKI_TOKEN` (falls back to `$AIPROXY_TOKEN` for aiproxy-hosted servers) |
| Research nudge | every turn | Static 3-line system-prompt footer routing knowledge questions to the `research` skill (cache-safe) |
| Retro | every 8 settled agent runs (re-arms; `oncePerSession` pins to first) | Queues a 4-line delegation directive that **auto-triggers an idle agent** (`followUp` + `triggerTurn` — no user nudge needed): main agent writes its session extraction to a temp file and fires one detached headless `pi -p` worker (`LLM_WIKI_AUTOPILOT_DISABLE=1`). Static worker instructions live in `worker-retro.md` inside the package — directives stay tiny. Worker auto-writes pages, lints, logs to `/tmp/llm-wiki-retro-<space>.log`, and intercom-sends a completion line to the main session |

Two skills are vendored and load natively — `/skill:research`, `/skill:retro`.

## Install

```bash
# after pushing this repo:
pi install git:github.com/<you>/pi-rust-wiki
# local development install:
pi install /work
```

## Config

Optional `<project>/.pi/llm-wiki.json` (absent = defaults):

```json
{
  "bootstrap": true,
  "researchNudge": true,
  "autoInject": false,
  "display": false,
  "wikiMcpUrl": "http://host.containers.internal:8484/mcp"  # or omit when aiproxy hosts the server
  "wikiMcpToken": "",
  "retro": { "enabled": true, "everyNRuns": 8, "oncePerSession": false }
}
```

**`researchNudge`:** default `true` — a session-static footer in the system prompt scopes the agent to this project's wiki space and points it at the research/retro skills. Byte-identical all session (prompt-cache safe).

**`autoInject`:** default `false`. When `true`, every user turn triggers a `wiki_recall` search of the project space keyed on the prompt; hits scoring ≥ 2.0 (title/id-strength matches) are injected as a hidden conversation message (`display: false`) so the agent sees relevant wiki pages without being asked. Personal-layer hits are excluded (cross-project noise), max 3 hits, identical prompts never inject twice (retry-safe). Per-turn recall content never touches the system prompt, so the provider's prompt cache stays warm.

**Wiki health surfacing:** problems are pushed, not pull-only. Once per session the extension probes `wiki_status` for the project space; if health is degraded (orphans, gaps) you get a warning notice with counts. The probe re-runs each time retro auto-fires, and (with `autoInject`) a one-line health hint rides the injected recall message. Healthy/empty spaces stay silent.

**`display`:** default `false` — directive text (retro) is delivered silently; you'll see the agent's one-line report and the background worker command. Set `true` to render directive text in the UI.

**`wikiMcpUrl`:** the MCP endpoint the extension calls directly for the mechanical bootstrap (`wiki_use_space` / `wiki_bootstrap`). Default `http://host.containers.internal:9999/mcp/wiki` — the aiproxy-hosted rust-wiki; point at `http://host.containers.internal:8484/mcp` for a standalone local server. The local rust-wiki server. Set it to wherever your rust-wiki binary listens.

**`wikiMcpToken`:** bearer token for that endpoint. Default empty — rust-wiki serves unauthenticated on trusted networks. Set via `WIKI_TOKEN` env or the config key if you front it with auth.

**`retro.oncePerSession`:** default `false` — retro re-arms and fires again after every `everyNRuns` settled runs. Set `true` for the fire-once-per-session behavior.

**Wiki space naming:** derived per project from git — `<first-commit-subject>-<short-hash>`, e.g. `init rust-wiki` → `rust-wiki-cc79119`. Bootstrap creates the space if it doesn't exist, then scopes every wiki call to it. Not a git repo (or no commits) → the default space is used.

**Env guard:** setting `LLM_WIKI_AUTOPILOT_DISABLE=1` disables all hooks — used by the retro worker to avoid recursive firing; set it yourself to turn the autopilot off for a session.

**Global config:** `~/.pi/agent/llm-wiki.json` (honors `PI_CODING_AGENT_DIR`) shares settings across all projects. Layering: defaults ← global ← project, per key; a project file overrides only the keys it sets. A malformed file is skipped with a warning (defaults apply to that layer).

## Vendored skills

Two adapted skills targeting rust-wiki tools: `research` (recall/search/read + synthesize) and `retro` (record session insights). The old geronimo skill suite was removed at the cutover.

## Development

```bash
npm install
npm test          # vitest
npm run typecheck # tsc --noEmit
```
