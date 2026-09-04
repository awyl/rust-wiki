import { existsSync, readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SKILLS_DIR = fileURLToPath(new URL("../skills", import.meta.url));

describe("vendored skills", () => {
  it("contains the three auto-trigger skills", () => {
    for (const name of ["bootstrap", "crystallize", "research"]) {
      expect(existsSync(`${SKILLS_DIR}/${name}/SKILL.md`), name).toBe(true);
    }
  });

  it("every skill directory has valid frontmatter", () => {
    const dirs = readdirSync(SKILLS_DIR).filter((d) => !d.startsWith("."));
    expect(dirs.length).toBeGreaterThanOrEqual(17);
    for (const dir of dirs) {
      const md = readFileSync(`${SKILLS_DIR}/${dir}/SKILL.md`, "utf-8");
      expect(md).toMatch(/^---\n/, dir);
      expect(md).toMatch(/^name: /m, dir);
      expect(md).toMatch(/^description: /m, dir);
    }
  });
});
