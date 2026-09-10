# pi-rust-wiki

Autonomous [rust-wiki](../rust-wiki/) skill triggers for
[pi](https://github.com/badlogic/pi-mono). Requires a running rust-wiki MCP server (`wiki_*` tools).

## What it does

| Trigger | When | Effect |
|---------|------|--------|
| Bootstrap | first user message | **Holds the turn for sub-second mechanical MCP calls, then processes the message**: the extension calls the wiki server directly (`wiki_use_space` → `wiki_bootstrap` if the space is missing). No model involved. Sessions that never receive a message never fire it. Wiki scoping rides the research nudge footer (session-static, cache-safe). Endpoint: `wikiMcpUrl` config key (default = the local rust-wiki server), token = `$WIKI_TOKEN` (falls back to `$AIPROXY_TOKEN` for aiproxy-hosted servers) |
| Research nudge | every turn | Static 3-line system-prompt footer routing knowledge questions to the `research` skill (cache-safe) |
| Retro | every 8 settled runs (re-arms; `oncePerSession` pins to first; `minMutatingCalls` optionally skips quiet windows; backstop fires after 10 quiet windows) | **Fully background, zero model context**: the extension writes mechanical git evidence to a temp file and spawns one detached headless `pi -p` worker (`LLM_WIKI_AUTOPILOT_DISABLE=1`). Static worker instructions live in `worker-retro.md` — zosmaai non-trivial rule (record nothing for trivial sessions). Completion surfaces as a bootstrap-style UI notice only (info on success, warning on failure); logs to `/tmp/llm-wiki-retro-<space>.log` |
| Discovery | every 24 settled runs (on by default; set `discover.enabled: false` to turn off) | Same background mechanism with `worker-discover.md`: finds outside sources the sessions never captured via MCP web search, bounded by `maxCaptures`, then synthesizes linked pages — one page per named person/org/tool/product, matching zosmaai's entity behaviour. Skips URLs already in the vault; `topics` empty means work the vault's own gaps; `dryRun` reports without writing. Shares a single-flight guard with retro (they never run at once); logs to `/tmp/llm-wiki-discover-<space>.log` (appended, one `=== run ===` header per pass) |

Three skills are vendored and load natively — `/skill:llm-wiki` (canonical
workflows), `/skill:retro`, `/skill:research`. Two background worker prompts
sit next to them: `worker-retro.md` and `worker-discover.md`.

## Space guardrails

- `wiki_use_space` pins once: the first successful pin wins; a later pin to a
different space is blocked with the pinned name in the reason.
- `wiki_use_space("personal")` is always blocked — cross-project writes go
through `wiki_ensure_personal_page` / `wiki_write_personal_page` (no switch).

## Install

```bash
# after pushing this repo:
pi install git:github.com/<you>/pi-rust-wiki
# local development install:
pi install /work
```

## Config

Optional `<project>/.pi/llm-wiki.json` (absent = defaults). `//` and
`/* */` comments are allowed — they are stripped before parsing, and a `//`
inside a value (a URL, say) is left alone. A commented example with every
key at its default ships as [`llm-wiki.example.json`](./llm-wiki.example.json)
in the repo root; it is not read, copy it into place:

```json
{
  "bootstrap": true,
  "researchNudge": true,
  "autoInject": false,
  "display": false,
  "wikiMcpUrl": "http://host.containers.internal:8484/mcp"  # or omit when aiproxy hosts the server
  "wikiMcpToken": "",
  "retro": { "enabled": true, "everyNRuns": 8, "oncePerSession": false },
  "discover": { "enabled": true, "everyNRuns": 24, "topics": [], "maxCaptures": 3, "dryRun": false }
}
```

**`researchNudge`:** default `true` — a session-static footer in the system prompt scopes the agent to this project's wiki space and points it at the research/retro skills. Byte-identical all session (prompt-cache safe).

**`autoInject`:** default `false`. When `true`, every user turn triggers a `wiki_recall` search of the project space keyed on the prompt; hits scoring ≥ 2.0 (title/id-strength matches) are injected as a hidden conversation message (`display: false`) so the agent sees relevant wiki pages without being asked. Personal-layer hits are excluded (cross-project noise), max 3 hits, identical prompts never inject twice (retry-safe). Per-turn recall content never touches the system prompt, so the provider's prompt cache stays warm.

**Wiki health surfacing:** problems are pushed, not pull-only. Once per session the extension probes `wiki_status` for the project space; if health is degraded (orphans, gaps) you get a warning notice with counts. The probe re-runs each time retro auto-fires, and (with `autoInject`) a one-line health hint rides the injected recall message. Healthy/empty spaces stay silent.

**`display`:** default `false` — directive text (retro) is delivered silently; you'll see the agent's one-line report and the background worker command. Set `true` to render directive text in the UI.

**`wikiMcpUrl`:** the MCP endpoint the extension calls directly for the mechanical bootstrap (`wiki_use_space` / `wiki_bootstrap`). Default `http://host.containers.internal:9999/mcp/wiki` — the aiproxy-hosted rust-wiki; point at `http://host.containers.internal:8484/mcp` for a standalone local server. The local rust-wiki server. Set it to wherever your rust-wiki binary listens.

**`wikiMcpToken`:** bearer token for that endpoint. Default empty — rust-wiki serves unauthenticated on trusted networks. Set via `WIKI_TOKEN` env or the config key if you front it with auth.

**`retro.oncePerSession`:** default `false` — retro re-arms and fires again after every `everyNRuns` settled runs. Set `true` for the fire-once-per-session behavior.

**`discover`:** default **on**. A background worker runs every `everyNRuns` settled runs (default 24) and adds outside sources the sessions never captured: it searches the web with the MCP search tools, skips URLs already in the vault, captures at most `maxCaptures` (default 3), then synthesizes linked pages — including one `entity` page per named person, organization, tool, or product, which is how zosmaai's vaults filled up. `topics` seeds it (empty = work the vault's own gaps; every topic must link to an existing page). `dryRun: true` reports what it would capture and writes nothing. Set `enabled: false` to turn it off. Discovery and retro share one single-flight guard, so they never overlap. Worker output is appended to `/tmp/llm-wiki-discover-<space>.log`, one `=== run <timestamp> ===` header per pass.

**Wiki space naming:** derived per project from git — `<first-commit-subject>-<short-hash>`, e.g. `init rust-wiki` → `rust-wiki-cc79119`. Bootstrap creates the space if it doesn't exist, then scopes every wiki call to it. Not a git repo (or no commits) → no space can be derived, so the autopilot stays **inert**: no bootstrap, no recall injection, no workers. It never falls back to a shared `default` space.

**Env guard:** setting `LLM_WIKI_AUTOPILOT_DISABLE=1` disables all hooks — used by the retro worker to avoid recursive firing; set it yourself to turn the autopilot off for a session.

**Global config:** `~/.pi/agent/llm-wiki.json` (honors `PI_CODING_AGENT_DIR`) shares settings across all projects. Layering: defaults ← global ← project, per key; a project file overrides only the keys it sets. A malformed file is skipped with a warning (defaults apply to that layer).

## Vendored skills

Brain vendored from `zosmaai/pi-llm-wiki`, retargeted to remote storage — see `docs/COVERAGE.md` for the section-for-section map:

- `skills/llm-wiki/SKILL.md` — canonical brain (all workflows, page conventions). `templates/` mirrors the server templates (reference only; `wiki_template` is authoritative).
- `skills/research`, `skills/retro` — thin autopilot entry points (hook compatibility).
- `prompts/` — 11 ported command prompts (query, ingest, lint, status, init, retro, discover, digest, run, req, record, skills). Nothing parked except host screens.

## Development

```bash
npm install
npm test          # vitest
npm run typecheck # tsc --noEmit
```
