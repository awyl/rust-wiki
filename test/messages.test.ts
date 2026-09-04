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

  it("bootstrap ensures the derived space exists and targets it", () => {
    const msg = buildBootstrapDirective("/abs/skills/bootstrap/SKILL.md", "rust-wiki-cc79119");
    expect(msg.content).toContain("wiki_spaces_create");
    expect(msg.content).toContain('wiki: "rust-wiki-cc79119"');
  });

  it("bootstrap falls back to the default space when no name is derivable", () => {
    const msg = buildBootstrapDirective("/abs/skills/bootstrap/SKILL.md");
    expect(msg.content).toContain("default space");
    expect(msg.content).not.toContain("wiki_spaces_create");
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
