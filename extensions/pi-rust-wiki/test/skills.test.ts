import { existsSync, readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SKILLS_DIR = fileURLToPath(new URL("../skills", import.meta.url));

describe("vendored skills", () => {
  it("contains the two cutover skills", () => {
    for (const name of ["retro", "research"]) {
      expect(existsSync(`${SKILLS_DIR}/${name}/SKILL.md`)).toBe(true);
    }
  });

  it("every skill directory has valid frontmatter", () => {
    const dirs = readdirSync(SKILLS_DIR).filter((d) => !d.startsWith("."));
    expect(dirs.length).toBe(2);
    for (const dir of dirs) {
      const md = readFileSync(`${SKILLS_DIR}/${dir}/SKILL.md`, "utf-8");
      expect(md).toMatch(/^---\n/);
      expect(md).toMatch(/^name: /m);
      expect(md).toMatch(/^description: /m);
    }
  });

  it("skills target rust-wiki tools, not geronimo", () => {
    for (const dir of ["retro", "research"]) {
      const md = readFileSync(`${SKILLS_DIR}/${dir}/SKILL.md`, "utf-8");
      expect(md).toContain("wiki_use_space");
      expect(md).not.toContain("wiki_content_write");
      expect(md).not.toContain("wiki_ingest(path");
    }
  });
});
