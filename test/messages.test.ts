import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { buildCrystallizeDirective, buildResearchNudge } from "../extensions/llm-wiki-skills/lib/messages.js";

const WORKER_PROMPT_PATH = "/abs/extensions/llm-wiki-skills/worker-crystallize.md";
const SKILL_PATH = "/abs/skills/crystallize/SKILL.md";

describe("directive builders", () => {
  it("crystallize payload delegates to a headless worker instead of working inline", () => {
    const msg = buildCrystallizeDirective(SKILL_PATH, WORKER_PROMPT_PATH, "rust-wiki-cc79119");
    expect(msg.customType).toBe("llm-wiki-crystallize");
    expect(msg.display).toBe(true);
    expect(msg.content).toContain("pi -p");
    expect(msg.content).toContain("LLM_WIKI_AUTOPILOT_DISABLE");
  });

  it("display=false hides the directive from the UI but still delivers it", () => {
    const msg = buildCrystallizeDirective(SKILL_PATH, WORKER_PROMPT_PATH, "rust-wiki-cc79119", false);
    expect(msg.display).toBe(false);
    expect(msg.content).toContain("pi -p");
  });

  it("crystallize directive carries only paths — static instructions live in the worker file", () => {
    const msg = buildCrystallizeDirective(SKILL_PATH, WORKER_PROMPT_PATH, "rust-wiki-cc79119");
    expect(msg.content).toContain(WORKER_PROMPT_PATH);
    expect(msg.content).toContain(SKILL_PATH);
    expect(msg.content).toContain("/tmp/llm-wiki-extraction-rust-wiki-cc79119.md");
    expect(msg.content).toContain("/tmp/llm-wiki-crystallize-rust-wiki-cc79119.log");
    expect(msg.content).not.toContain("AUTO-WRITE");
    expect(msg.content).not.toContain("wiki_index_rebuild");
    expect(msg.content).not.toContain("wiki_ingest");
  });

  it("crystallize worker prompt names the extraction file and wiki", () => {
    const msg = buildCrystallizeDirective(SKILL_PATH, WORKER_PROMPT_PATH, "rust-wiki-cc79119");
    expect(msg.content).toContain("Extraction file: /tmp/llm-wiki-extraction-rust-wiki-cc79119.md");
    expect(msg.content).toContain("Wiki: rust-wiki-cc79119");
  });

  it("crystallize falls back to the default space when no name is derivable", () => {
    const msg = buildCrystallizeDirective(SKILL_PATH, WORKER_PROMPT_PATH, null);
    expect(msg.content).toContain("default space");
  });

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

describe("research nudge", () => {
  it("is a stable footer routing knowledge questions to the research skill", () => {
    expect(buildResearchNudge("x")).toContain("research");
  });

  it("stays byte-identical for the same session input (cache-safe)", () => {
    expect(buildResearchNudge("x")).toBe(buildResearchNudge("x"));
  });
});
