import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { buildResearchNudge } from "../extensions/llm-wiki-skills/lib/messages.js";
import type { BootstrapResult } from "../extensions/llm-wiki-skills/lib/bootstrap.js";

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

function recorder(results: BootstrapResult[] = [{ space: "ok", index: "ok", detail: "space ok; index ok" }]) {
  const calls: any[] = [];
  let i = 0;
  return {
    calls,
    ensureWikiReadyFn: async (input: any) => {
      calls.push(input);
      return results[Math.min(i, results.length - 1)];
    },
  };
}

async function loadExtension(deps: Record<string, unknown> = {}) {
  const mod = await import("../extensions/llm-wiki-skills/index.js");
  const fake = createFakePi();
  mod.default(fake.pi as ExtensionAPI, deps);
  return { ...fake };
}

describe("bootstrap hold", () => {
  it("session_start fires nothing; the first agent run runs the mechanical bootstrap first", async () => {
    const { ensureWikiReadyFn, calls } = recorder();
    const { handlers, sent } = await loadExtension({ ensureWikiReadyFn });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    expect(sent).toHaveLength(0);
    expect(calls).toHaveLength(0); // nothing fires until the user speaks

    await handlers.get("before_agent_start")!({ prompt: "hi", systemPrompt: "BASE" }, ctx);
    expect(calls).toHaveLength(1);
    expect(calls[0].wikiName).toBe("rust-wiki-cc79119");
    expect(calls[0].url).toContain("http");
  });

  it("bootstrap runs exactly once per session", async () => {
    const { ensureWikiReadyFn, calls } = recorder();
    const { handlers } = await loadExtension({ ensureWikiReadyFn });
    const ctx = fakeCtx("/work");
    const hook = handlers.get("before_agent_start")!;
    await hook({ prompt: "a", systemPrompt: "BASE" }, ctx);
    await hook({ prompt: "b", systemPrompt: "BASE" }, ctx);
    await hook({ prompt: "c", systemPrompt: "BASE" }, ctx);
    expect(calls).toHaveLength(1);
  });

  it("passes the configured wikiRoot to the bootstrap call", async () => {
    const dir = mkdtempSync(join(tmpdir(), "hooks-root-"));
    mkdirSync(join(dir, ".pi"), { recursive: true });
    writeFileSync(join(dir, ".pi", "llm-wiki.json"), JSON.stringify({ wikiRoot: "/data" }));
    const { ensureWikiReadyFn, calls } = recorder();
    try {
      const { handlers } = await loadExtension({ ensureWikiReadyFn });
      const ctx = fakeCtx(dir);
      await handlers.get("session_start")!({ reason: "startup" }, ctx);
      await handlers.get("before_agent_start")!({ prompt: "hi", systemPrompt: "BASE" }, ctx);
      expect(calls[0].wikiRoot).toBe("/data");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("skips the bootstrap entirely when disabled", async () => {
    const dir = mkdtempSync(join(tmpdir(), "hooks-noboot-"));
    mkdirSync(join(dir, ".pi"), { recursive: true });
    writeFileSync(join(dir, ".pi", "llm-wiki.json"), JSON.stringify({ bootstrap: false }));
    const { ensureWikiReadyFn, calls } = recorder();
    try {
      const { handlers } = await loadExtension({ ensureWikiReadyFn });
      const ctx = fakeCtx(dir);
      await handlers.get("session_start")!({ reason: "startup" }, ctx);
      await handlers.get("before_agent_start")!({ prompt: "hi", systemPrompt: "BASE" }, ctx);
      expect(calls).toHaveLength(0);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("a failing bootstrap never blocks the turn", async () => {
    const { handlers } = await loadExtension({
      ensureWikiReadyFn: async () => {
        throw new Error("mcp exploded");
      },
    });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    const result = await handlers.get("before_agent_start")!(
      { prompt: "hi", systemPrompt: "BASE" },
      ctx,
    );
    expect(result!.systemPrompt).toContain("BASE"); // turn proceeds
  });
});

describe("research nudge", () => {
  it("appends wiki scoping exactly once", async () => {
    const { ensureWikiReadyFn } = recorder();
    const { handlers } = await loadExtension({ ensureWikiReadyFn });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    const hook = handlers.get("before_agent_start")!;
    const first = await hook({ prompt: "hi", systemPrompt: "BASE" }, ctx);
    expect(first!.systemPrompt).toContain('wiki: "rust-wiki-cc79119"');
    const second = await hook({ prompt: "hi again", systemPrompt: first!.systemPrompt }, ctx);
    expect(second).toBeUndefined();
  });

  it("omits scoping when no name is derivable", () => {
    expect(buildResearchNudge(null)).not.toContain("wiki:");
  });

  it("is session-static — same input, byte-identical output", () => {
    expect(buildResearchNudge("x")).toBe(buildResearchNudge("x"));
  });
});

describe("crystallize", () => {
  it("fires at the threshold, re-arms, and auto-triggers an idle agent", async () => {
    const { ensureWikiReadyFn } = recorder();
    const { handlers, sent } = await loadExtension({ ensureWikiReadyFn });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    await handlers.get("before_agent_start")!({ prompt: "hi", systemPrompt: "BASE" }, ctx);
    const settled = handlers.get("agent_settled")!;
    for (let i = 0; i < 16; i++) await settled({}, ctx);
    const crystallize = sent.filter((s) => s.message.customType === "llm-wiki-crystallize");
    expect(crystallize).toHaveLength(2); // runs 8 and 16
    expect(crystallize[0].options).toEqual({ deliverAs: "followUp", triggerTurn: true });
    expect(crystallize[0].message.content).toContain("pi -p");
  });

  it("session_start resets the crystallize counter and proposal flag", async () => {
    const { ensureWikiReadyFn } = recorder();
    const { handlers, sent } = await loadExtension({ ensureWikiReadyFn });
    const ctx = fakeCtx("/work");
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
      const { ensureWikiReadyFn } = recorder();
      const { handlers, sent } = await loadExtension({ ensureWikiReadyFn });
      await handlers.get("session_start")!({ reason: "startup" }, fakeCtx(dir));
      const settled = handlers.get("agent_settled")!;
      for (let i = 0; i < 24; i++) await settled({}, fakeCtx(dir));
      expect(sent.filter((s) => s.message.customType === "llm-wiki-crystallize")).toHaveLength(1);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
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
});

describe("worker file", () => {
  it("worker-crystallize.md ships the full unattended procedure", () => {
    const file = readFileSync(join(__dirname, "../extensions/llm-wiki-skills/worker-crystallize.md"), "utf-8");
    expect(file).toContain("AUTO-WRITE");
    expect(file).toContain("wiki_index_rebuild");
    expect(file).toContain("wiki_ingest");
    expect(file).toContain("wiki_lint");
    expect(file).toContain("accumulation contract");
    expect(file).toContain("intercom");
  });
});
