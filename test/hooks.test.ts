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
    expect(crystallize[0].message.content).toContain("pi -p");
    expect(sent.filter((s) => s.message.customType === "llm-wiki-crystallize")).toHaveLength(1);
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
