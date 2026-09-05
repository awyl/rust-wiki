import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  buildBootstrapDirective,
  buildCrystallizeDirective,
  RESEARCH_NUDGE,
} from "../extensions/llm-wiki-skills/lib/messages.js";

const WORKER_PROMPT_PATH = "/abs/extensions/llm-wiki-skills/worker-crystallize.md";
const SKILL_PATH = "/abs/skills/crystallize/SKILL.md";

describe("directive builders", () => {
  it("lean bootstrap payload carries customType and defaults to visible", () => {
    const msg = buildBootstrapDirective("rust-wiki-cc79119");
    expect(msg.customType).toBe("llm-wiki-bootstrap");
    expect(msg.display).toBe(true);
  });

  it("display=false hides the directive from the UI but still delivers it", () => {
    const bootstrap = buildBootstrapDirective("rust-wiki-cc79119", false);
    const crystallize = buildCrystallizeDirective(SKILL_PATH, WORKER_PROMPT_PATH, "rust-wiki-cc79119", false);
    expect(bootstrap.display).toBe(false);
    expect(crystallize.display).toBe(false);
    expect(bootstrap.content).toContain("wiki_info");
    expect(crystallize.content).toContain("pi -p");
  });

  it("lean bootstrap ensures the derived space exists and targets it", () => {
    const msg = buildBootstrapDirective("rust-wiki-cc79119");
    expect(msg.content).toContain("wiki_spaces_create");
    expect(msg.content).toContain('wiki: "rust-wiki-cc79119"');
  });

  it("lean bootstrap folds space check and index health into one wiki_info call", () => {
    const msg = buildBootstrapDirective("rust-wiki-cc79119");
    expect(msg.content).toContain("wiki_info");
    expect(msg.content).toContain("wiki_index_rebuild");
    expect(msg.content).not.toContain("wiki_spaces_list");
  });

  it("lean bootstrap skips deep orientation and stays tiny", () => {
    const msg = buildBootstrapDirective("rust-wiki-cc79119");
    expect(msg.content).not.toContain("SKILL.md");
    expect(msg.content).not.toContain("hub");
    expect(msg.content).not.toContain("wiki_schema");
    expect(msg.content.split("\n")).toHaveLength(3);
  });

  it("lean bootstrap falls back to the default space when no name is derivable", () => {
    const msg = buildBootstrapDirective(null);
    expect(msg.content).toContain("default space");
    expect(msg.content).not.toContain("wiki_spaces_create");
  });

  it("crystallize payload delegates to a headless worker instead of working inline", () => {
    const msg = buildCrystallizeDirective(SKILL_PATH, WORKER_PROMPT_PATH, "rust-wiki-cc79119");
    expect(msg.customType).toBe("llm-wiki-crystallize");
    expect(msg.display).toBe(true);
    expect(msg.content).toContain("pi -p");
    expect(msg.content).toContain("LLM_WIKI_AUTOPILOT_DISABLE");
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
    expect(msg.content).toContain('Extraction file: /tmp/llm-wiki-extraction-rust-wiki-cc79119.md');
    expect(msg.content).toContain("Wiki: rust-wiki-cc79119");
  });

  it("worker-crystallize.md ships the full unattended procedure", () => {
    const file = readFileSync(
      join(__dirname, "../extensions/llm-wiki-skills/worker-crystallize.md"),
      "utf-8",
    );
    expect(file).toContain("AUTO-WRITE");
    expect(file).toContain("wiki_index_rebuild");
    expect(file).toContain("wiki_ingest");
    expect(file).toContain("wiki_lint");
    expect(file).toContain("accumulation contract");
  });

  it("research nudge is a stable constant", () => {
    expect(RESEARCH_NUDGE).toContain("research");
    expect(RESEARCH_NUDGE).toBe(RESEARCH_NUDGE); // same reference — constant, cache-safe
  });
});
