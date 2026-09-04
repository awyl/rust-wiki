import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { deriveWikiName } from "../extensions/llm-wiki-skills/lib/wikiName.js";

const dirs: string[] = [];
function gitRepo(...commitArgs: string[]): string {
  const dir = mkdtempSync(join(tmpdir(), "wiki-name-"));
  dirs.push(dir);
  const run = (args: string[]) => execFileSync("git", ["-C", dir, ...args], { encoding: "utf8" });
  run(["init", "-q"]);
  run(["config", "user.email", "t@t"]);
  run(["config", "user.name", "t"]);
  execFileSync("git", ["-C", dir, "commit", "--allow-empty", ...commitArgs], { encoding: "utf8" });
  return dir;
}

afterAll(() => {
  for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
});

describe("deriveWikiName", () => {
  it("strips init prefix from first commit subject and appends short hash", () => {
    const dir = gitRepo("-m", "init rust-wiki");
    const hash = execFileSync("git", ["-C", dir, "rev-parse", "--short=7", "HEAD"], { encoding: "utf8" }).trim();
    expect(deriveWikiName(dir)).toBe(`rust-wiki-${hash}`);
  });

  it("keeps non-init subjects, sanitized", () => {
    const dir = gitRepo("-m", "Add: Parser stuff!");
    const hash = execFileSync("git", ["-C", dir, "rev-parse", "--short=7", "HEAD"], { encoding: "utf8" }).trim();
    expect(deriveWikiName(dir)).toBe(`add-parser-stuff-${hash}`);
  });

  it("returns null outside a git repo", () => {
    const dir = mkdtempSync(join(tmpdir(), "wiki-name-"));
    dirs.push(dir);
    expect(deriveWikiName(dir)).toBeNull();
  });

  it("returns null in a repo with no commits", () => {
    const dir = mkdtempSync(join(tmpdir(), "wiki-name-"));
    dirs.push(dir);
    execFileSync("git", ["-C", dir, "init", "-q"], { encoding: "utf8" });
    expect(deriveWikiName(dir)).toBeNull();
  });
});
