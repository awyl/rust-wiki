# pi-llm-wiki-autopilot — Design

**Date:** 2026-09-04
**Status:** Approved design, pending implementation plan

## Purpose

A pi package that makes the [llm-wiki-skills](https://github.com/geronimo-iia/llm-wiki-skills) workflows run autonomously inside pi sessions. The wiki MCP server (llm-wiki) is already connected and exposes `wiki_*` tools to the agent. What is missing is timing: something must decide *when* each skill should fire without the user asking.

The extension is a thin orchestrator. It decides when; the skills decide how; the agent executes via the wiki MCP tools. The extension never speaks MCP itself.

Reference implementations studied:

- `geronimo-iia/llm-wiki-skills` — 17 Agent-Skills-standard SKILL.md files that orchestrate llm-wiki MCP tools. Source of the vendored skills. Declares `bootstrap`, `crystallize`, `research` as auto-capable.
- `zosmaai/pi-llm-wiki` — a pi package that hooks wiki behavior into pi. Heavy: implements its own engine, vault format, and MCP server. We reuse its hook patterns (`session_start` queue, `before_agent_start` injection, cache-stable system prompt) but none of its engine.

## Architecture

```
┌─────────────┐    queue directive     ┌──────────────┐   read SKILL.md   ┌───────────────────┐
│  Extension   │ ─────────────────────▶ │    Agent      │ ────────────────▶ │ wiki MCP (llm-wiki)│
│ (timing brain)│                       │ (worker)      │   call wiki_*    │  (already running) │
└─────────────┘                        └──────────────┘                   └───────────────────┘
       │
       ▼ vendors + exposes
  skills/  (17 SKILL.md, loaded natively by pi)
```

Separation of concerns:

- **Extension** (`extensions/llm-wiki-skills/`) — lifecycle hooks, config, injection payloads. No wiki logic, no network.
- **Skills** (`skills/`) — verbatim vendored copies of upstream llm-wiki-skills. Execution knowledge.
- **pi** — loads skills natively from the `pi.skills` manifest; gives `/skill:name` commands for free.

## Components

```
/work/
├── package.json                  # pi package manifest
├── extensions/llm-wiki-skills/
│   ├── index.ts                  # factory: registers the 3 hooks
│   └── lib/
│       ├── config.ts             # loadConfig(): defaults + .pi/llm-wiki.json merge
│       └── messages.ts           # directive builders with absolute SKILL.md paths
├── skills/                       # vendored llm-wiki-skills (verbatim snapshot)
│   ├── bootstrap/SKILL.md
│   ├── crystallize/SKILL.md
│   ├── research/SKILL.md
│   └── … (17 total)
├── scripts/vendor.sh             # re-sync skills/ from upstream GitHub tarball
└── test/
    ├── config.test.ts
    ├── messages.test.ts
    └── hooks.test.ts
```

### package.json

```json
{
  "name": "pi-llm-wiki-autopilot",
  "version": "0.1.0",
  "type": "module",
  "pi": {
    "extensions": ["./extensions"],
    "skills": ["./skills"]
  },
  "devDependencies": {
    "@earendil-works/pi-coding-agent": "*",
    "typescript": "*",
    "vitest": "*"
  }
}
```

The `pi.skills` entry is how skills reach pi — no `resources_discover` event needed.

### index.ts — the three hooks

**1. `session_start` → bootstrap directive**

If `bootstrap` enabled: queue a message for the first prompt:

```typescript
pi.sendMessage(
  {
    customType: "llm-wiki-bootstrap",
    content: "<bootstrap directive text>",
    display: true,
  },
  { deliverAs: "nextTurn" },
);
```

The directive names the bootstrap skill and gives its absolute path (resolved from `import.meta.url` → `../../skills/bootstrap/SKILL.md`) and tells the agent to read and follow it. Agent orients: `wiki_info` → config → hub pages.

**2. `before_agent_start` → research nudge**

If `researchNudge` enabled and the footer is not already present in `event.systemPrompt`: return `{ systemPrompt: event.systemPrompt + FOOTER }`.

The footer is a constant 2–3 line string reminding the agent that wiki knowledge is available and the research skill is the tool for knowledge questions. It never varies, so the provider prompt-cache prefix stays stable. (Lesson from pi-llm-wiki issue #92: volatile per-turn content in the system prompt invalidates the whole cache every turn.)

**3. `agent_settled` → crystallize proposal**

In-memory counter of settled agent runs, reset per extension instance (pi rebinds extensions on `/new`, `/resume`, `/fork` — the counter resets with them, which is the desired session scope).

At `crystallize.everyNRuns` (default 8), and only once per session: queue a crystallize proposal message (`deliverAs: "nextTurn"`). The proposal tells the agent to load the crystallize skill and follow it — which includes its own rule: *always confirm with the user before writing*.

### lib/config.ts

```typescript
export interface AutopilotConfig {
  bootstrap: boolean;
  researchNudge: boolean;
  crystallize: { everyNRuns: number; enabled: boolean };
}
```

- Defaults: `bootstrap: true`, `researchNudge: true`, `crystallize: { everyNRuns: 8, enabled: true }`
- Merge source: `<cwd>/.pi/llm-wiki.json`. Absent file = defaults. Malformed JSON = defaults + one-time warning via `ctx.ui.notify` (never crash session start).
- Loaded in the factory and re-resolved per `session_start` (cwd can change across session replacement).

### lib/messages.ts

Pure functions returning directive strings. Absolute skill paths passed in by index.ts (resolved once from `import.meta.url`). Keep builders pure → trivially unit-testable.

- `bootstrapDirective(skillPath: string): string`
- `researchNudge(): string` (constant)
- `crystallizeDirective(skillPath: string): string`

## Data Flow

1. `session_start` — config resolved. Bootstrap on → directive queued.
2. First user prompt — agent receives queued directive, reads `skills/bootstrap/SKILL.md`, calls `wiki_info`, orients. If MCP is down, the skill's own step 0 surfaces the failure to the user; extension does nothing special.
3. Every turn — system prompt carries the static research nudge. Knowledge questions route to the research skill (`wiki_search` → read pages → synthesize).
4. After N settled runs — crystallize proposal queued once. Next turn, agent proposes what to capture; user confirms; agent writes via `wiki_content_write` / `wiki_content_new`.
5. Any time — `/skill:research`, `/skill:crystallize`, `/skill:ingest`, … work natively; zero extension involvement.

## Error Handling

| Failure | Behavior |
|---|---|
| Missing `.pi/llm-wiki.json` | defaults, silent |
| Malformed config JSON | defaults + one warning, never crash startup |
| Missing `skills/` directory | console warning; hooks still register (skills may load from another location) |
| Wiki MCP unreachable | surfaced by agent via `wiki_info` (skill step 0); extension stays out of the way |
| Extension load in print/JSON mode | hooks fire; `sendMessage` queues still work; no UI calls (none used) |

No background resources, no sockets, no timers → `session_shutdown` needs no cleanup.

## Testing

vitest. Fake `ExtensionAPI`: records registered handlers; tests invoke handlers with synthetic events and a fake `ctx`.

- **config.test.ts** — defaults; JSON merge; malformed JSON falls back; partial config keeps other defaults.
- **messages.test.ts** — payload shape (`customType`, `content`, `display`); absolute path appears in content; nudge is a stable constant (two calls, same string).
- **hooks.test.ts** —
  - `session_start` with bootstrap on → one `sendMessage` queued with `deliverAs: "nextTurn"`;
  - disabled → no message;
  - `before_agent_start` → footer appended once, not duplicated when already present;
  - `agent_settled` × threshold → exactly one crystallize message total (once-per-session invariant);
  - below threshold → no message.

Manual smoke test after implementation: `pi -e ./extensions/llm-wiki-skills/index.ts` in a project with the wiki MCP connected; verify bootstrap fires on first prompt and `/skill:research` completes.

## Vendoring

`scripts/vendor.sh`: download upstream tarball for a pinned ref, replace `skills/` contents, print the upstream commit. Re-vendoring is a deliberate, reviewable act (diff in git). The vendored snapshot is recorded in README (upstream repo + ref).

## Out of Scope (YAGNI)

- No custom tools — skills already expose everything the agent needs via MCP.
- No custom `/wiki:*` commands — `/skill:*` covers invocation.
- No MCP client code, no health pings, no connection management — MCP already wired by the user's environment.
- No per-turn prompt classification for the research nudge — static footer chosen for cache stability and simplicity.
- No lint/stats/graph automation — those skills remain manual (`/skill:lint` etc.). Can be added later behind config if wanted.

## Open Item

Package name: `pi-llm-wiki-autopilot` used throughout; rename is a one-line change in `package.json` + README if the user prefers another.
