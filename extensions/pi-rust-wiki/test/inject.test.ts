import { describe, expect, it } from "vitest";
import { buildRecallBlock, buildRecallMessage, MIN_SCORE, MAX_HITS } from "../llm-wiki-skills/lib/inject.js";

const hit = (over: Partial<Parameters<typeof buildRecallBlock>[0][number]> = {}) => ({
  id: "concepts/okf",
  title: "OKF",
  type: "concept",
  score: 3.0,
  preview: "okf   details",
  ...over,
});

describe("buildRecallBlock", () => {
  it("formats strong space-layer hits with trimmed previews", () => {
    const block = buildRecallBlock([hit()])!;
    expect(block).toContain("## Wiki recall (auto");
    expect(block).toContain("- OKF (concepts/okf) — okf details");
    expect(block).toContain("wiki_read_page");
  });

  it("gates on MIN_SCORE and drops personal-layer hits", () => {
    expect(buildRecallBlock([hit({ score: MIN_SCORE - 0.1 })])).toBeUndefined();
    expect(buildRecallBlock([hit({ layer: "personal", score: 9 })])).toBeUndefined();
    expect(buildRecallBlock([])).toBeUndefined();
  });

  it("caps hits at MAX_HITS and omits empty previews", () => {
    const many = Array.from({ length: MAX_HITS + 2 }, (_, i) => hit({ id: `p/${i}`, title: `T${i}` }));
    const block = buildRecallBlock(many)!;
    expect(block.split("\n").filter((l) => l.startsWith("- "))).toHaveLength(MAX_HITS);
    expect(buildRecallBlock([hit({ preview: "" })])!.includes("— OKF (")).toBe(false);
  });

  it("buildRecallMessage returns a hidden message or undefined", () => {
    const msg = buildRecallMessage([hit()])!;
    expect(msg.display).toBe(false);
    expect(msg.customType).toBe("rust-wiki-recall-context");
    expect(buildRecallMessage([])).toBeUndefined();
  });
});

import { buildHealthHint, healthForPrompt } from "../llm-wiki-skills/lib/inject.js";

describe("buildHealthHint", () => {
  it("silent when healthy or empty", () => {
    expect(buildHealthHint({ space: "s", health: "good", total_pages: 5, orphans: 0, gaps: 0 })).toBeUndefined();
    expect(buildHealthHint({ space: "s", health: "empty", total_pages: 0, orphans: 0, gaps: 0 })).toBeUndefined();
  });

  it("one line with counts when warning", () => {
    const hint = buildHealthHint({ space: "s", health: "warning", total_pages: 6, orphans: 6, gaps: 1 })!;
    expect(hint).toContain("⚠ wiki health: 6 orphans, 1 gaps");
    expect(hint).toContain("wiki_lint");
  });

  it("healthForPrompt resolves null on unreachable server", async () => {
    expect(await healthForPrompt("http://127.0.0.1:1/mcp", "", "s")).toBeNull();
  });
});
