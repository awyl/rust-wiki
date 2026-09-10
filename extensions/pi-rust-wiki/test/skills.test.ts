import { existsSync, readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SKILLS_DIR = fileURLToPath(new URL("../skills", import.meta.url));

describe("vendored skills", () => {
  it("contains the canonical skill plus entry points", () => {
    for (const name of ["llm-wiki", "retro", "research"]) {
      expect(existsSync(`${SKILLS_DIR}/${name}/SKILL.md`)).toBe(true);
    }
  });

  it("every skill directory has valid frontmatter", () => {
    const dirs = readdirSync(SKILLS_DIR).filter((d) => !d.startsWith("."));
    expect(dirs.length).toBe(3);
    for (const dir of dirs) {
      const md = readFileSync(`${SKILLS_DIR}/${dir}/SKILL.md`, "utf-8");
      expect(md).toMatch(/^---\n/);
      expect(md).toMatch(/^name: /m);
      expect(md).toMatch(/^description: /m);
    }
  });

  it("skills use rust-wiki tool names", () => {
    for (const dir of ["llm-wiki", "retro", "research"]) {
      const md = readFileSync(`${SKILLS_DIR}/${dir}/SKILL.md`, "utf-8");
      expect(md).not.toContain("wiki_content_write");
      expect(md).not.toContain("wiki_ingest(path");
      expect(md).not.toContain(".llm-wiki/");
    }
    const canonical = readFileSync(`${SKILLS_DIR}/llm-wiki/SKILL.md`, "utf-8");
    expect(canonical).toContain("wiki_template");
    expect(canonical).toContain("wiki_use_space");
  });

  it("canonical templates mirror the server", () => {
    const tplDir = `${SKILLS_DIR}/llm-wiki/templates/pages`;
    for (const name of ["concept", "entity", "source", "analysis", "synthesis", "requirement", "skill", "case"]) {
      const md = readFileSync(`${tplDir}/${name}.md`, "utf-8");
      expect(md).toMatch(/^---\ntype: /m);
      expect(md).toContain("{title}");
    }
  });

  it("prompts cover the ported command set", () => {
    const promptsDir = fileURLToPath(new URL("../prompts", import.meta.url));
    for (const name of ["wiki-query", "wiki-ingest", "wiki-lint", "wiki-status", "wiki-init", "wiki-retro", "wiki-discover", "wiki-digest", "wiki-run", "wiki-req", "wiki-record", "wiki-skills"]) {
      expect(existsSync(`${promptsDir}/${name}.md`)).toBe(true);
    }
    for (const dir of ["llm-wiki", "retro", "research"]) {
      const md = readFileSync(`${SKILLS_DIR}/${dir}/SKILL.md`, "utf-8");
      expect(md).not.toContain("trajectories parked");
    }
  });

  it("worker prompts are separate: retro records, discover finds sources", () => {
    const workersDir = fileURLToPath(new URL("../llm-wiki-skills", import.meta.url));
    const retro = readFileSync(`${workersDir}/worker-retro.md`, "utf-8");
    const discover = readFileSync(`${workersDir}/worker-discover.md`, "utf-8");
    // One owner per job: retro no longer runs a discover pass.
    expect(retro).not.toMatch(/bounded discover/i);
    expect(retro).toContain("RETRO DONE");
    expect(discover).toContain("DISCOVER DONE");
    expect(discover).toContain("wiki_capture_source");
    expect(discover).toContain("wiki_lint");
  });
});
