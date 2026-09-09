import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { buildResearchNudge } from "../llm-wiki-skills/lib/messages.js";
import type { BootstrapResult } from "../llm-wiki-skills/lib/bootstrap.js";

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
  const mod = await import("../llm-wiki-skills/index.js");
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
    expect(first!.systemPrompt).toContain(`space: "rust-wiki-cc79119"`);
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

describe("retro", () => {
  const mutate = { toolName: "edit", input: {} };
  const workerOk = async () => ({ ok: true, summary: "RETRO DONE pages=1 [x]" });

  it("fires at the window when the window did mutating work, then notifies (no context)", async () => {
    const { ensureWikiReadyFn } = recorder();
    const spawned: any[] = [];
    const { handlers, sent } = await loadExtension({
      ensureWikiReadyFn,
      spawnWorkerFn: (async (...a: any[]) => {
        spawned.push(a);
        return workerOk();
      }) as any,
    });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    const toolCall = handlers.get("tool_call")!;
    const settled = handlers.get("agent_settled")!;
    for (let i = 0; i < 16; i++) {
      await toolCall(mutate, {}); // every run mutates
      await settled({}, ctx);
    }
    await new Promise((r) => setImmediate(r));
    expect(spawned).toHaveLength(2); // windows ending at runs 8 and 16
    expect(sent).toHaveLength(0); // zero model context: no sendMessage
    expect(ctx.ui.notify).toHaveBeenCalledWith(expect.stringContaining("RETRO DONE"), "info");
  });

  it("default fires every window — the worker judges triviality from the transcript", async () => {
    const { ensureWikiReadyFn } = recorder();
    let spawns = 0;
    const { handlers } = await loadExtension({
      ensureWikiReadyFn,
      spawnWorkerFn: (async () => {
        spawns += 1;
        return workerOk();
      }) as any,
    });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    const settled = handlers.get("agent_settled")!;
    for (let i = 0; i < 16; i++) await settled({}, ctx); // no tool calls
    await new Promise((r) => setImmediate(r));
    expect(spawns).toBe(2); // windows end at 8 and 16; worker may record zero pages
  });

  it("minMutatingCalls gate skips quiet windows silently", async () => {
    const dir = mkdtempSync(join(tmpdir(), "hooks-gate-"));
    mkdirSync(join(dir, ".pi"), { recursive: true });
    writeFileSync(
      join(dir, ".pi", "llm-wiki.json"),
      JSON.stringify({ retro: { minMutatingCalls: 1 } }),
    );
    try {
      const { ensureWikiReadyFn } = recorder();
      let spawns = 0;
      const { handlers } = await loadExtension({
        ensureWikiReadyFn,
        spawnWorkerFn: (async () => {
          spawns += 1;
          return workerOk();
        }) as any,
      });
      const ctx = fakeCtx(dir);
      await handlers.get("session_start")!({ reason: "startup" }, ctx);
      const settled = handlers.get("agent_settled")!;
      for (let i = 0; i < 16; i++) await settled({}, ctx); // no tool calls
      await new Promise((r) => setImmediate(r));
      expect(spawns).toBe(0);
      expect(ctx.ui.notify).not.toHaveBeenCalledWith(expect.stringContaining("retro"), expect.anything());
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("session_start resets counters", async () => {
    const { ensureWikiReadyFn } = recorder();
    let spawns = 0;
    const { handlers } = await loadExtension({
      ensureWikiReadyFn,
      spawnWorkerFn: (async () => {
        spawns += 1;
        return workerOk();
      }) as any,
    });
    const ctx = fakeCtx("/work");
    const toolCall = handlers.get("tool_call")!;
    const settled = handlers.get("agent_settled")!;
    for (let i = 0; i < 8; i++) {
      await toolCall(mutate, {});
      await settled({}, ctx);
    }
    await handlers.get("session_start")!({ reason: "new" }, ctx);
    for (let i = 0; i < 7; i++) {
      await toolCall(mutate, {});
      await settled({}, ctx);
    }
    await new Promise((r) => setImmediate(r));
    expect(spawns).toBe(1);
  });

  it("oncePerSession pins retro to the first window only", async () => {
    const dir = mkdtempSync(join(tmpdir(), "hooks-config-"));
    mkdirSync(join(dir, ".pi"), { recursive: true });
    writeFileSync(
      join(dir, ".pi", "llm-wiki.json"),
      JSON.stringify({ retro: { enabled: true, everyNRuns: 8, oncePerSession: true } }),
    );
    try {
      const { ensureWikiReadyFn } = recorder();
      let spawns = 0;
      const { handlers } = await loadExtension({
        ensureWikiReadyFn,
        spawnWorkerFn: (async () => {
          spawns += 1;
          return workerOk();
        }) as any,
      });
      await handlers.get("session_start")!({ reason: "startup" }, fakeCtx(dir));
      const toolCall = handlers.get("tool_call")!;
      const settled = handlers.get("agent_settled")!;
      for (let i = 0; i < 24; i++) {
        await toolCall(mutate, {});
        await settled({}, fakeCtx(dir));
      }
      await new Promise((r) => setImmediate(r));
      expect(spawns).toBe(1);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("worker failure surfaces a warning notice", async () => {
    const { ensureWikiReadyFn } = recorder();
    const { handlers } = await loadExtension({
      ensureWikiReadyFn,
      spawnWorkerFn: (async () => ({ ok: false, summary: "worker exited 1" })) as any,
    });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({ reason: "startup" }, ctx);
    const toolCall = handlers.get("tool_call")!;
    const settled = handlers.get("agent_settled")!;
    for (let i = 0; i < 8; i++) {
      await toolCall(mutate, {});
      await settled({}, ctx);
    }
    await new Promise((r) => setImmediate(r));
    expect(ctx.ui.notify).toHaveBeenCalledWith(expect.stringContaining("worker exited 1"), "warning");
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
  it("worker-retro.md ships the full unattended procedure", () => {
    const file = readFileSync(join(__dirname, "../llm-wiki-skills/worker-retro.md"), "utf-8");
    expect(file).toContain("AUTO-WRITE");
    expect(file).toContain("non-trivial");
    expect(file).toContain("wiki_use_space");
    expect(file).toContain("wiki_retro");
    expect(file).toContain("wiki_lint");
    expect(file).toContain("RETRO DONE");
    expect(file).not.toContain("intercom");
  });
});

describe("health hints (C+D)", () => {
  const warn = { space: "s", health: "warning", total_pages: 6, orphans: 3, gaps: 0 };
  const good = { space: "s", health: "good", total_pages: 6, orphans: 0, gaps: 0 };

  it("surfaces problems once per session; silent when healthy", async () => {
    const calls: string[] = [];
    const healthFn = async (_u: string, _t: string, space: string) => {
      calls.push(space);
      return warn;
    };
    const { handlers } = await loadExtension({ ensureWikiReadyFn: async () => ({ space: "ok", index: "ok", detail: "d" }), healthFn });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({}, ctx);
    await handlers.get("before_agent_start")!({ prompt: "a", systemPrompt: "BASE" }, ctx);
    await handlers.get("before_agent_start")!({ prompt: "b", systemPrompt: "BASE" }, ctx);
    expect(calls).toHaveLength(1); // once per session
    expect(ctx.ui.notify).toHaveBeenCalledWith(expect.stringContaining("3 orphans"), "warning");
  });

  it("retro fire is notice-only — no extra health re-check, no context", async () => {
    const healthFn = async () => good;
    const { handlers, sent } = await loadExtension({
      ensureWikiReadyFn: async () => ({ space: "ok", index: "ok", detail: "d" }),
      healthFn,
      spawnWorkerFn: (async () => ({ ok: true, summary: "RETRO DONE pages=0" })) as any,
    });
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({}, ctx);
    const toolCall = handlers.get("tool_call")!;
    for (let i = 0; i < 8; i++) {
      await toolCall({ toolName: "edit", input: {} }, {});
      await handlers.get("agent_settled")!({}, ctx);
    }
    await new Promise((r) => setImmediate(r));
    expect(ctx.ui.notify).toHaveBeenCalledWith(expect.stringContaining("RETRO DONE"), "info");
    expect(sent).toHaveLength(0);
  });
});

describe("use_space intercept", () => {
  const useSpace = (space: string) => ({ toolName: "wiki_use_space", input: { space } });
  const useSpaceResult = (space: string, isError = false) => ({
    toolName: "wiki_use_space",
    input: { space },
    isError,
  });

  it("first use_space pins; different space blocked", async () => {
    const { handlers } = await loadExtension();
    await handlers.get("session_start")!({}, fakeCtx("/work"));
    expect(await handlers.get("tool_call")!(useSpace("proj-a"), {})).toBeUndefined();
    await handlers.get("tool_result")!(useSpaceResult("proj-a"), {});
    expect(await handlers.get("tool_call")!(useSpace("proj-a"), {})).toBeUndefined(); // idempotent
    const blocked = await handlers.get("tool_call")!(useSpace("proj-b"), {});
    expect(blocked).toEqual({
      block: true,
      reason: expect.stringContaining('pinned to "proj-a"'),
    });
  });

  it("use_space personal always blocked", async () => {
    const { handlers } = await loadExtension();
    await handlers.get("session_start")!({}, fakeCtx("/work"));
    const blocked = await handlers.get("tool_call")!(useSpace("personal"), {});
    expect(blocked).toEqual({ block: true, reason: expect.stringContaining("personal") });
  });

  it("failed use_space does not pin", async () => {
    const { handlers } = await loadExtension();
    await handlers.get("session_start")!({}, fakeCtx("/work"));
    await handlers.get("tool_result")!(useSpaceResult("proj-a", true), {});
    expect(await handlers.get("tool_call")!(useSpace("proj-b"), {})).toBeUndefined();
  });

  it("pin resets on new session", async () => {
    const { handlers } = await loadExtension();
    const ctx = fakeCtx("/work");
    await handlers.get("session_start")!({}, ctx);
    await handlers.get("tool_result")!(useSpaceResult("proj-a"), {});
    await handlers.get("session_start")!({}, ctx);
    expect(await handlers.get("tool_call")!(useSpace("proj-b"), {})).toBeUndefined();
  });

  it("non-space tools untouched", async () => {
    const { handlers } = await loadExtension();
    await handlers.get("session_start")!({}, fakeCtx("/work"));
    expect(
      await handlers.get("tool_call")!({ toolName: "wiki_recall", input: {} }, {}),
    ).toBeUndefined();
  });
});
