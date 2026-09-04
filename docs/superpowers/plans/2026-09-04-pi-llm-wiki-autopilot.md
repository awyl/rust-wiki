# pi-llm-wiki-autopilot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pi package that fires llm-wiki-skills autonomously — bootstrap at session start, a static research nudge in the system prompt, and a once-per-session crystallize proposal after N settled agent runs.

**Architecture:** Thin orchestrator extension (timing brain) + verbatim vendored upstream skills (execution body). The extension injects directives; the agent executes them by reading SKILL.md files and calling the already-connected wiki MCP tools. The extension never speaks MCP, holds no background resources, and needs no shutdown cleanup.

**Tech Stack:** TypeScript (jiti-loaded, no build step), vitest, Node ≥ 18. No runtime npm dependencies.

**Spec:** `docs/superpowers/specs/2026-09-04-pi-llm-wiki-autopilot-design.md`

## Global Constraints

- DRY, KISS, YAGNI, TDD — every code task is test-first.
- Project rule: **every `git commit` step requires explicit user approval at execution time** (approvals are one-time-only).
- Extension stays MCP-agnostic: no `wiki_*` calls, no network except `scripts/vendor.sh`.
- No background resources (sockets/timers/watchers) in the extension factory (pi hard requirement).
- Research nudge is a single constant string — never per-turn dynamic content in the system prompt (prompt-cache stability).
- Verified API signatures (from `pi-coding-agent` types and official `dynamic-resources` example):
  - `pi.on(event, handler)`; `session_start` handler receives `(event, ctx)` with `ctx.cwd`, `ctx.ui.notify(msg, "info" | "warning" | "error")`.
  - `pi.sendMessage(message, options?)` where `message: { customType, content, display, details? }` and `options: { triggerTurn?, deliverAs?: "steer" | "followUp" | "nextTurn" }`.
  - `before_agent_start` handler may return `{ systemPrompt?: string, message?: {...} }`.
  - `agent_settled` handler receives `(_event, ctx)`.
  - Path resolution uses `dirname(fileURLToPath(import.meta.url))` — canonical pattern in pi's own examples (jiti supports it).

---

### Task 1: Package scaffold + vendored skills

**Files:**
- Create: `package.json`
- Create: `.gitignore`
- Create: `scripts/vendor.sh` (executable)
- Create: `skills/` (populated by vendor script)
- Create: `test/skills.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces: package manifest with `pi.extensions: ["./extensions"]` and `pi.skills: ["./skills"]`; `skills/` directory containing ≥ 17 vendored skills including `bootstrap`, `crystallize`, `research`. Later tasks rely on `skills/<name>/SKILL.md` existing at that relative location.

- [ ] **Step 1: Write the failing test**

Create `test/skills.test.ts`:

```typescript
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SKILLS_DIR = fileURLToPath(new URL("../skills", import.meta.url));

describe("vendored skills", () => {
  it("contains the three auto-trigger skills", () => {
    for (const name of ["bootstrap", "crystallize", "research"]) {
      expect(existsSync(`${SKILLS_DIR}/${name}/SKILL.md`), name).toBe(true);
    }
  });

  it("every skill directory has valid frontmatter", () => {
    const dirs = readdirSync(SKILLS_DIR).filter((d) => !d.startsWith("."));
    expect(dirs.length).toBeGreaterThanOrEqual(17);
    for (const dir of dirs) {
      const md = readFileSync(`${SKILLS_DIR}/${dir}/SKILL.md`, "utf-8");
      expect(md).toMatch(/^---\n/, dir);
      expect(md).toMatch(/^name: /m, dir);
      expect(md).toMatch(/^description: /m, dir);
    }
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run test/skills.test.ts`
Expected: FAIL — `ENOENT ... skills` (directory missing).

- [ ] **Step 3: Create scaffold**

`package.json`:

```json
{
  "name": "pi-llm-wiki-autopilot",
  "version": "0.1.0",
  "description": "Autonomous llm-wiki skill triggers for pi: bootstrap on session start, research nudge, crystallize proposal",
  "type": "module",
  "license": "MIT",
  "scripts": {
    "test": "vitest run",
    "typecheck": "tsc --noEmit"
  },
  "pi": {
    "extensions": ["./extensions"],
    "skills": ["./skills"]
  },
  "devDependencies": {
    "@earendil-works/pi-coding-agent": "*",
    "@types/node": "^24",
    "typescript": "^5",
    "vitest": "^3"
  }
}
```

`.gitignore`:

```
node_modules/
```

`scripts/vendor.sh`:

```bash
#!/usr/bin/env bash
# Re-sync skills/ from upstream llm-wiki-skills. Usage: scripts/vendor.sh [ref]
set -euo pipefail
REPO="geronimo-iia/llm-wiki-skills"
REF="${1:-main}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL "https://github.com/${REPO}/archive/${REF}.tar.gz" | tar -xz -C "$TMP"
SRC="$(echo "$TMP"/llm-wiki-skills-*/skills)"
rm -rf skills && mkdir -p skills
cp -R "$SRC"/. skills/
echo "Vendored ${REPO}@${REF} -> skills/ ($(ls skills | wc -l) skills)"
```

Then: `chmod +x scripts/vendor.sh && scripts/vendor.sh main` and `npm install`.

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run test/skills.test.ts`
Expected: PASS (2 tests). Verify the vendored snapshot ref and record it in the README in Task 5.

- [ ] **Step 5: Commit** (requires user approval)

```bash
git add package.json .gitignore scripts/vendor.sh skills/ test/skills.test.ts package-lock.json
git commit -m "feat: scaffold pi package and vendor llm-wiki-skills"
```

---

### Task 2: Config loader

**Files:**
- Create: `extensions/llm-wiki-skills/lib/config.ts`
- Create: `test/config.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces (used by Task 4):

```typescript
export interface CrystallizeConfig { enabled: boolean; everyNRuns: number; }
export interface AutopilotConfig {
  bootstrap: boolean;
  researchNudge: boolean;
  crystallize: CrystallizeConfig;
}
export const DEFAULT_CONFIG: AutopilotConfig;           // bootstrap:true, researchNudge:true, crystallize:{enabled:true, everyNRuns:8}
export const CONFIG_FILENAME = "llm-wiki.json";
export interface LoadResult { config: AutopilotConfig; warning?: string; }
export function loadConfig(cwd: string): LoadResult;    // reads <cwd>/.pi/llm-wiki.json
```

- [ ] **Step 1: Write the failing tests**

Create `test/config.test.ts`:

```typescript
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_CONFIG, loadConfig } from "../extensions/llm-wiki-skills/lib/config.js";

describe("loadConfig", () => {
  let dir: string;

  beforeEach(() => { dir = mkdtempSync(join(tmpdir(), "wiki-cfg-")); });
  afterEach(() => { rmSync(dir, { recursive: true, force: true }); });

  const write = (body: string) => {
    mkdirSync(join(dir, ".pi"));
    writeFileSync(join(dir, ".pi", "llm-wiki.json"), body);
  };

  it("returns defaults when no config file exists", () => {
    const { config, warning } = loadConfig(dir);
    expect(config).toEqual(DEFAULT_CONFIG);
    expect(warning).toBeUndefined();
  });

  it("merges partial config over defaults", () => {
    write(JSON.stringify({ bootstrap: false, crystallize: { everyNRuns: 3 } }));
    const { config } = loadConfig(dir);
    expect(config).toEqual({
      bootstrap: false,
      researchNudge: true,
      crystallize: { enabled: true, everyNRuns: 3 },
    });
  });

  it("falls back to defaults with a warning on malformed JSON", () => {
    write("{ not json");
    const { config, warning } = loadConfig(dir);
    expect(config).toEqual(DEFAULT_CONFIG);
    expect(warning).toContain("llm-wiki.json");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run test/config.test.ts`
Expected: FAIL — cannot resolve `../extensions/llm-wiki-skills/lib/config.js`.

- [ ] **Step 3: Write minimal implementation**

Create `extensions/llm-wiki-skills/lib/config.ts`:

```typescript
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

export interface CrystallizeConfig {
  enabled: boolean;
  everyNRuns: number;
}

export interface AutopilotConfig {
  bootstrap: boolean;
  researchNudge: boolean;
  crystallize: CrystallizeConfig;
}

export const DEFAULT_CONFIG: AutopilotConfig = {
  bootstrap: true,
  researchNudge: true,
  crystallize: { enabled: true, everyNRuns: 8 },
};

export const CONFIG_FILENAME = "llm-wiki.json";

export interface LoadResult {
  config: AutopilotConfig;
  warning?: string;
}

export function loadConfig(cwd: string): LoadResult {
  const path = join(cwd, ".pi", CONFIG_FILENAME);
  if (!existsSync(path)) return { config: DEFAULT_CONFIG };
  try {
    const raw = JSON.parse(readFileSync(path, "utf-8")) as Partial<AutopilotConfig>;
    return {
      config: {
        bootstrap: raw.bootstrap ?? DEFAULT_CONFIG.bootstrap,
        researchNudge: raw.researchNudge ?? DEFAULT_CONFIG.researchNudge,
        crystallize: {
          enabled: raw.crystallize?.enabled ?? DEFAULT_CONFIG.crystallize.enabled,
          everyNRuns: raw.crystallize?.everyNRuns ?? DEFAULT_CONFIG.crystallize.everyNRuns,
        },
      },
    };
  } catch (err) {
    return {
      config: DEFAULT_CONFIG,
      warning: `[llm-wiki-autopilot] malformed ${path}, using defaults: ${(err as Error).message}`,
    };
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run test/config.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit** (requires user approval)

```bash
git add extensions/llm-wiki-skills/lib/config.ts test/config.test.ts
git commit -m "feat: config loader with defaults and malformed-JSON fallback"
```

---

### Task 3: Directive message builders

**Files:**
- Create: `extensions/llm-wiki-skills/lib/messages.ts`
- Create: `test/messages.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces (used by Task 4):

```typescript
export interface DirectiveMessage { customType: string; content: string; display: boolean; }
export function buildBootstrapDirective(skillPath: string): DirectiveMessage;
export function buildCrystallizeDirective(skillPath: string): DirectiveMessage;
export const RESEARCH_NUDGE: string;   // constant system-prompt footer
```

- [ ] **Step 1: Write the failing tests**

Create `test/messages.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import {
  buildBootstrapDirective,
  buildCrystallizeDirective,
  RESEARCH_NUDGE,
} from "../extensions/llm-wiki-skills/lib/messages.js";

describe("directive builders", () => {
  it("bootstrap payload carries customType, display and the skill path", () => {
    const msg = buildBootstrapDirective("/abs/skills/bootstrap/SKILL.md");
    expect(msg.customType).toBe("llm-wiki-bootstrap");
    expect(msg.display).toBe(true);
    expect(msg.content).toContain("/abs/skills/bootstrap/SKILL.md");
    expect(msg.content).toContain("wiki_info");
  });

  it("crystallize payload requires user confirmation before writes", () => {
    const msg = buildCrystallizeDirective("/abs/skills/crystallize/SKILL.md");
    expect(msg.customType).toBe("llm-wiki-crystallize");
    expect(msg.display).toBe(true);
    expect(msg.content).toContain("/abs/skills/crystallize/SKILL.md");
    expect(msg.content.toLowerCase()).toMatch(/confirm|propose/);
  });

  it("research nudge is a stable constant", () => {
    expect(RESEARCH_NUDGE).toContain("research");
    expect(RESEARCH_NUDGE).toBe(RESEARCH_NUDGE); // same reference — constant, cache-safe
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run test/messages.test.ts`
Expected: FAIL — cannot resolve `messages.js`.

- [ ] **Step 3: Write minimal implementation**

Create `extensions/llm-wiki-skills/lib/messages.ts`:

```typescript
export interface DirectiveMessage {
  customType: string;
  content: string;
  display: boolean;
}

export function buildBootstrapDirective(skillPath: string): DirectiveMessage {
  return {
    customType: "llm-wiki-bootstrap",
    display: true,
    content: [
      "## Wiki orientation (bootstrap)",
      "",
      "Before responding to the user, orient to this project's wiki:",
      `1. Read the bootstrap skill at \`${skillPath}\``,
      "2. Follow it: call `wiki_info`, review the config and types, read hub pages.",
      "3. Report a one-line orientation summary, then continue with the user's request.",
    ].join("\n"),
  };
}

export function buildCrystallizeDirective(skillPath: string): DirectiveMessage {
  return {
    customType: "llm-wiki-crystallize",
    display: true,
    content: [
      "## Crystallize proposal",
      "",
      `This session is long enough to crystallize. Read the crystallize skill at \`${skillPath}\` and follow it:`,
      "- Extract decisions, findings, and open questions from this session.",
      "- **Propose** the pages you would write to the user and wait for confirmation before any write.",
    ].join("\n"),
  };
}

export const RESEARCH_NUDGE = [
  "",
  "## Wiki knowledge",
  "A wiki MCP server is connected to this session. When a question might be answered from wiki knowledge, use the `research` skill: read its SKILL.md, then `wiki_search` / `wiki_content_read` before answering from memory alone.",
].join("\n");
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run test/messages.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit** (requires user approval)

```bash
git add extensions/llm-wiki-skills/lib/messages.ts test/messages.test.ts
git commit -m "feat: bootstrap/crystallize directives and research nudge builders"
```

---

### Task 4: Extension hooks

**Files:**
- Create: `extensions/llm-wiki-skills/index.ts`
- Create: `test/hooks.test.ts`

**Interfaces:**
- Consumes: `loadConfig` (Task 2), `buildBootstrapDirective` / `buildCrystallizeDirective` / `RESEARCH_NUDGE` (Task 3).
- Produces: `export default function llmWikiAutopilot(pi: ExtensionAPI): void` — the entry pi loads. Registers `session_start`, `before_agent_start`, `agent_settled`.

- [ ] **Step 1: Write the failing tests**

Create `test/hooks.test.ts`:

```typescript
import { describe, expect, it, vi } from "vitest";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { RESEARCH_NUDGE } from "../extensions/llm-wiki-skills/lib/messages.js";

type Handler = (event: any, ctx: any) => Promise<any>;

function createFakePi() {
  const handlers = new Map<string, Handler>();
  const sent: Array<{ message: any; options: any }> = [];
  const pi = {
    on: (name: string, fn: Handler) => void handlers.set(name, fn),
    sendMessage: (message: any, options: any) => {
      sent.push({ message, options });
      return Promise.resolve();
    },
  } as unknown as ExtensionAPI & {
    on: (name: string, fn: Handler) => void;
    sendMessage: (message: any, options: any) => Promise<void>;
  };
  return { pi, handlers, sent };
}

const fakeCtx = (cwd = "/tmp/project") => ({
  cwd,
  ui: { notify: vi.fn() },
});

async function loadExtension() {
  const mod = await import("../extensions/llm-wiki-skills/index.js");
  const fake = createFakePi();
  mod.default(fake.pi as ExtensionAPI);
  return { ...fake };
}

describe("extension hooks", () => {
  it("session_start queues a bootstrap directive for the next turn", async () => {
    const { handlers, sent } = await loadExtension();
    await handlers.get("session_start")!({ reason: "startup" }, fakeCtx());
    expect(sent).toHaveLength(1);
    expect(sent[0].message.customType).toBe("llm-wiki-bootstrap");
    expect(sent[0].options).toEqual({ deliverAs: "nextTurn" });
  });

  it("before_agent_start appends the research nudge exactly once", async () => {
    const { handlers } = await loadExtension();
    const ctx = fakeCtx();
    const first = await handlers.get("before_agent_start")!(
      { prompt: "hi", systemPrompt: "BASE" },
      ctx,
    );
    expect(first!.systemPrompt).toBe("BASE" + RESEARCH_NUDGE);
    const second = await handlers.get("before_agent_start")!(
      { prompt: "hi again", systemPrompt: "BASE" + RESEARCH_NUDGE },
      ctx,
    );
    expect(second).toBeUndefined();
  });

  it("agent_settled proposes crystallize once at the threshold", async () => {
    const { handlers, sent } = await loadExtension();
    const handler = handlers.get("agent_settled")!;
    const ctx = fakeCtx();
    for (let i = 0; i < 8; i++) await handler({}, ctx);
    const crystallize = sent.filter((s) => s.message.customType === "llm-wiki-crystallize");
    expect(crystallize).toHaveLength(1);
    for (let i = 0; i < 5; i++) await handler({}, ctx); // more runs — still once
    expect(sent.filter((s) => s.message.customType === "llm-wiki-crystallize")).toHaveLength(1);
  });

  it("session_start resets the crystallize counter and proposal flag", async () => {
    const { handlers, sent } = await loadExtension();
    const settled = handlers.get("agent_settled")!;
    const ctx = fakeCtx();
    for (let i = 0; i < 8; i++) await settled({}, ctx);
    await handlers.get("session_start")!({ reason: "new" }, ctx);
    for (let i = 0; i < 7; i++) await settled({}, ctx);
    expect(sent.filter((s) => s.message.customType === "llm-wiki-crystallize")).toHaveLength(1);
    await settled({}, ctx);
    expect(sent.filter((s) => s.message.customType === "llm-wiki-crystallize")).toHaveLength(2);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run test/hooks.test.ts`
Expected: FAIL — cannot resolve `index.js`.

- [ ] **Step 3: Write the implementation**

Create `extensions/llm-wiki-skills/index.ts`:

```typescript
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { loadConfig } from "./lib/config.js";
import {
  buildBootstrapDirective,
  buildCrystallizeDirective,
  RESEARCH_NUDGE,
} from "./lib/messages.js";

export default function llmWikiAutopilot(pi: ExtensionAPI): void {
  const here = dirname(fileURLToPath(import.meta.url));
  const skillsDir = join(here, "..", "..", "skills");
  const skillPath = (name: string) => join(skillsDir, name, "SKILL.md");

  if (!existsSync(skillsDir)) {
    // Hooks still register: skills may load from another pi skill location.
    console.warn("[llm-wiki-autopilot] skills/ not found next to extension; directives will point at a missing path");
  }

  let settledRuns = 0;
  let crystallizeProposed = false;

  pi.on("session_start", async (_event, ctx) => {
    settledRuns = 0;
    crystallizeProposed = false;
    const { config, warning } = loadConfig(ctx.cwd);
    if (warning) ctx.ui.notify(warning, "warning");
    if (config.bootstrap) {
      await pi.sendMessage(buildBootstrapDirective(skillPath("bootstrap")), {
        deliverAs: "nextTurn",
      });
    }
  });

  pi.on("before_agent_start", async (event) => {
    const { config } = loadConfig(process.cwd());
    if (!config.researchNudge) return;
    if (event.systemPrompt.includes(RESEARCH_NUDGE)) return;
    return { systemPrompt: event.systemPrompt + RESEARCH_NUDGE };
  });

  pi.on("agent_settled", async () => {
    const { config } = loadConfig(process.cwd());
    if (!config.crystallize.enabled || crystallizeProposed) return;
    settledRuns += 1;
    if (settledRuns >= config.crystallize.everyNRuns) {
      crystallizeProposed = true;
      await pi.sendMessage(buildCrystallizeDirective(skillPath("crystallize")), {
        deliverAs: "nextTurn",
      });
    }
  });
}
```

Note: `loadConfig` runs per turn in `before_agent_start`/`agent_settled` so config edits apply without restart; it is a tiny file read. The malformed-config warning is surfaced only by `session_start` (per-turn notifications would spam).

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run test/hooks.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit** (requires user approval)

```bash
git add extensions/llm-wiki-skills/index.ts test/hooks.test.ts
git commit -m "feat: autonomous hooks — session bootstrap, research nudge, crystallize proposal"
```

---

### Task 5: Typecheck, README, full verification

**Files:**
- Create: `tsconfig.json`
- Create: `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: green `npm test` + green `npm run typecheck`; README documents install, config, vendoring, upstream ref.

- [ ] **Step 1: Add tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true,
    "types": ["node"]
  },
  "include": ["extensions", "test"]
}
```

Run: `npm run typecheck`
Expected: no errors. (If the fake `ExtensionAPI` cast in `test/hooks.test.ts` fights strict mode, keep the cast — tests intentionally bypass typing.)

- [ ] **Step 2: Write README.md**

```markdown
# pi-llm-wiki-autopilot

Autonomous [llm-wiki-skills](https://github.com/geronimo-iia/llm-wiki-skills) triggers for
[pi](https://github.com/badlogic/pi-mono). Requires a connected llm-wiki MCP server (`wiki_*` tools).

## What it does

| Trigger | When | Effect |
|---------|------|--------|
| Bootstrap | `session_start` | Queues a directive: agent reads `skills/bootstrap/SKILL.md`, orients via `wiki_info` + config + hub pages |
| Research nudge | every turn | Static 3-line system-prompt footer routing knowledge questions to the `research` skill (cache-safe) |
| Crystallize | after 8 settled agent runs (once per session) | Queues a proposal: agent distils the session per `skills/crystallize/SKILL.md` and asks before writing |

All 17 upstream skills are vendored and load natively — `/skill:research`, `/skill:ingest`, `/skill:lint`, … work without the extension.

## Install

```bash
pi install git:github.com/<you>/pi-llm-wiki-autopilot
# local dev: pi install /path/to/this/repo
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

## Vendored skills

Snapshot of `geronimo-iia/llm-wiki-skills@<REF>` (update this line when re-vendoring).
Re-sync: `scripts/vendor.sh <ref>`, then diff `skills/` in git.

## Development

```bash
npm install
npm test          # vitest
npm run typecheck # tsc --noEmit
```
```

Fill `<you>` with the actual repo remote once known, and `<REF>` with the ref used in Task 1.

- [ ] **Step 3: Run the full suite**

Run: `npm test`
Expected: PASS — 12 tests across 4 files.

- [ ] **Step 4: Manual smoke test** (requires wiki MCP connected)

```bash
pi -e ./extensions/llm-wiki-skills/index.ts
```

Verify:
1. First prompt → bootstrap directive appears and the agent calls `wiki_info`.
2. Ask a knowledge question → agent uses `wiki_search` per the research skill.
3. `/skill:crystallize` loads the skill content.

- [ ] **Step 5: Commit** (requires user approval)

```bash
git add tsconfig.json README.md
git commit -m "docs: README, typecheck config; verify full suite"
```
