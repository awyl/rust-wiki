import { describe, expect, it } from "vitest";
import {
  buildBootstrapDirective,
  buildCrystallizeDirective,
  RESEARCH_NUDGE,
} from "../extensions/llm-wiki-skills/lib/messages.js";

describe("directive builders", () => {
  it("lean bootstrap payload carries customType and display", () => {
    const msg = buildBootstrapDirective("rust-wiki-cc79119");
    expect(msg.customType).toBe("llm-wiki-bootstrap");
    expect(msg.display).toBe(true);
  });

  it("lean bootstrap ensures the derived space exists and targets it", () => {
    const msg = buildBootstrapDirective("rust-wiki-cc79119");
    expect(msg.content).toContain("wiki_spaces_list");
    expect(msg.content).toContain("wiki_spaces_create");
    expect(msg.content).toContain('wiki: "rust-wiki-cc79119"');
  });

  it("lean bootstrap keeps index health in the critical path", () => {
    const msg = buildBootstrapDirective("rust-wiki-cc79119");
    expect(msg.content).toContain("wiki_info");
    expect(msg.content).toContain("wiki_index_rebuild");
  });

  it("lean bootstrap skips deep orientation", () => {
    const msg = buildBootstrapDirective("rust-wiki-cc79119");
    expect(msg.content).not.toContain("SKILL.md");
    expect(msg.content).not.toContain("hub");
    expect(msg.content).not.toContain("wiki_schema");
  });

  it("lean bootstrap falls back to the default space when no name is derivable", () => {
    const msg = buildBootstrapDirective(null);
    expect(msg.content).toContain("default space");
    expect(msg.content).not.toContain("wiki_spaces_create");
  });

  it("crystallize payload delegates to a headless worker instead of working inline", () => {
    const msg = buildCrystallizeDirective("/abs/skills/crystallize/SKILL.md", "rust-wiki-cc79119");
    expect(msg.customType).toBe("llm-wiki-crystallize");
    expect(msg.display).toBe(true);
    expect(msg.content).toContain("pi -p");
    expect(msg.content).toContain("LLM_WIKI_AUTOPILOT_DISABLE");
    expect(msg.content).toContain("/abs/skills/crystallize/SKILL.md");
  });

  it("crystallize directive makes the main agent embed its session extraction", () => {
    const msg = buildCrystallizeDirective("/abs/skills/crystallize/SKILL.md", "rust-wiki-cc79119");
    expect(msg.content).toContain("EXTRACTED_KNOWLEDGE");
    expect(msg.content).toContain("cannot see this session");
  });

  it("crystallize worker targets the derived wiki and runs unattended", () => {
    const msg = buildCrystallizeDirective("/abs/skills/crystallize/SKILL.md", "rust-wiki-cc79119");
    expect(msg.content).toContain('wiki: "rust-wiki-cc79119"');
    expect(msg.content).toContain("AUTO-WRITE");
    expect(msg.content).not.toMatch(/wait for confirmation/i);
    expect(msg.content).toContain("wiki_ingest");
    expect(msg.content).toContain("wiki_lint");
  });

  it("crystallize reports a log path for the worker output", () => {
    const msg = buildCrystallizeDirective("/abs/skills/crystallize/SKILL.md", "rust-wiki-cc79119");
    expect(msg.content).toContain("/tmp/llm-wiki-crystallize-rust-wiki-cc79119.log");
  });

  it("research nudge is a stable constant", () => {
    expect(RESEARCH_NUDGE).toContain("research");
    expect(RESEARCH_NUDGE).toBe(RESEARCH_NUDGE); // same reference — constant, cache-safe
  });
});
