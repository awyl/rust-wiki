import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { buildResearchNudge } from "../extensions/llm-wiki-skills/lib/messages.js";

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

async function loadExtension(deps: Record<string, unknown> = {}) {
  const mod = await import("../extensions/llm-wiki-skills/index.js");
  const fake = createFakePi();
  mod.default(fake.pi as ExtensionAPI, deps);
  return { ...fake };
}

describe("extension hooks", () => {
  it("session_start fires a detached bootstrap worker instead of messaging the agent", async () => {
    const spawned: string[] = [];
    const { handlers, sent } = await loadExtension({
      spawnDetached: (command: string) => void spawned.push(command),
    });
    await handlers.get("session_start")!({ reason: "startup" }, fakeCtx("/work"));
    expect(sent).toHaveLength(0);
    expect(spawned).toHaveLength(1);
    const cmd = spawned[0];
    expect(cmd).toContain("LLM_WIKI_AUTOPILOT_DISABLE=1");
    expect(cmd).toContain("bootstrap-worker.md");
    expect(cmd).toContain("Wiki: rust-wiki-cc79119");
    expect(cmd).toContain("/tmp/llm-wiki-bootstrap-rust-wiki-cc79119.log");
  });

  it("session_start passes the configured wikiRoot to the worker", async () => {
    const dir = mkdtempSync(join(tmpdir(), "hooks-root-"));
    mkdirSync(join(dir, ".pi"), { recursive: true });
    writeFileSync(join(dir, ".pi", "llm-wiki.json"), JSON.stringify({ wikiRoot: "/data" }));
    const spawned: string[] = [];
    try {
      const { handlers } = await loadExtension({
        spawnDetached: (command: string) => void spawned.push(command),
      });
      await handlers.get("session_start")!({ reason: "startup" }, fakeCtx(dir));
      expect(spawned[0]).toContain("WikiRoot: /data");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("before_agent_start appends the nudge with wiki scoping exactly once", async () => {
    const { handlers } = await loadExtension();
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    const first = await handlers.get("before_agent_start")!(
      { prompt: "hi", systemPrompt: "BASE" },
      ctx,
    );
    expect(first!.systemPrompt).toContain("BASE");
    expect(first!.systemPrompt).toContain('wiki: "rust-wiki-cc79119"');
    const second = await handlers.get("before_agent_start")!(
      { prompt: "hi again", systemPrompt: first!.systemPrompt },
      ctx,
    );
    expect(second).toBeUndefined();
  });

  it("agent_settled fires at the threshold and re-arms (recurring by default)", async () => {
    const spawned: string[] = [];
    const { handlers, sent } = await loadExtension({
      spawnDetached: (command: string) => void spawned.push(command),
    });
    const ctx = fakeCtx();
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    const handler = handlers.get("agent_settled")!;
    for (let i = 0; i < 16; i++) await handler({}, ctx);
    const crystallize = sent.filter((s) => s.message.customType === "llm-wiki-crystallize");
    expect(crystallize).toHaveLength(2); // fires at runs 8 and 16
    expect(crystallize[0].message.content).toContain("pi -p");
  });

  it("crystallize delivery auto-triggers an idle agent (followUp + triggerTurn)", async () => {
    const spawned: string[] = [];
    const { handlers, sent } = await loadExtension({
      spawnDetached: (command: string) => void spawned.push(command),
    });
    await handlers.get("session_start")!({ reason: "startup" }, fakeCtx());
    const handler = handlers.get("agent_settled")!;
    for (let i = 0; i < 8; i++) await handler({}, fakeCtx());
    const crystallize = sent.find((s) => s.message.customType === "llm-wiki-crystallize");
    expect(crystallize!.options).toEqual({ deliverAs: "followUp", triggerTurn: true });
  });

  it("env guard disables all hooks (prevents worker recursion)", async () => {
    process.env.LLM_WIKI_AUTOPILOT_DISABLE = "1";
    try {
      const { handlers, sent } = await loadExtension();
      expect(handlers.size).toBe(0);
      expect(sent).toHaveLength(0);
    } finally {
      delete process.env.LLM_WIKI_AUTOPILOT_DISABLE;
    }
  });

  it("session_start resets the crystallize counter and proposal flag", async () => {
    const spawned: string[] = [];
    const { handlers, sent } = await loadExtension({
      spawnDetached: (command: string) => void spawned.push(command),
    });
    const ctx = fakeCtx();
    const settled = handlers.get("agent_settled")!;
    for (let i = 0; i < 8; i++) await settled({}, ctx);
    await handlers.get("session_start")!({ reason: "new" }, ctx);
    for (let i = 0; i < 7; i++) await settled({}, ctx);
    expect(sent.filter((s) => s.message.customType === "llm-wiki-crystallize")).toHaveLength(1);
    await settled({}, ctx);
    expect(sent.filter((s) => s.message.customType === "llm-wiki-crystallize")).toHaveLength(2);
  });

  it("oncePerSession pins crystallize to the first threshold only", async () => {
    const dir = mkdtempSync(join(tmpdir(), "hooks-config-"));
    mkdirSync(join(dir, ".pi"), { recursive: true });
    writeFileSync(
      join(dir, ".pi", "llm-wiki.json"),
      JSON.stringify({ crystallize: { enabled: true, everyNRuns: 8, oncePerSession: true } }),
    );
    try {
      const { handlers, sent } = await loadExtension({
        spawnDetached: () => {},
      });
      await handlers.get("session_start")!({ reason: "startup" }, fakeCtx(dir));
      const settled = handlers.get("agent_settled")!;
      for (let i = 0; i < 24; i++) await settled({}, fakeCtx(dir));
      expect(sent.filter((s) => s.message.customType === "llm-wiki-crystallize")).toHaveLength(1);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("bootstrap worker file", () => {
  it("ships the mechanical ensure/rebuild/receipt procedure", () => {
    const file = readFileSync(
      join(__dirname, "../extensions/llm-wiki-skills/bootstrap-worker.md"),
      "utf-8",
    );
    expect(file).toContain("wiki_info");
    expect(file).toContain("wiki_spaces_create");
    expect(file).toContain("wiki_index_rebuild");
    expect(file).toContain("needs-parent-dir");
    expect(file).toContain("intercom");
  });
});

describe("research nudge", () => {
  it("carries wiki scoping when a name is derivable", () => {
    const nudge = buildResearchNudge("rust-wiki-cc79119");
    expect(nudge).toContain("research");
    expect(nudge).toContain('wiki: "rust-wiki-cc79119"');
  });

  it("omits scoping when no name is derivable", () => {
    expect(buildResearchNudge(null)).not.toContain("wiki:");
  });

  it("is session-static — same input, byte-identical output", () => {
    expect(buildResearchNudge("x")).toBe(buildResearchNudge("x"));
  });
});
