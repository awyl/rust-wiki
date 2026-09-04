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
