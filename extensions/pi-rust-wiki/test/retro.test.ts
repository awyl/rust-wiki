import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { buildEvidence, transcriptWindow } from "../llm-wiki-skills/lib/retro.js";
import { parseReport } from "../llm-wiki-skills/lib/worker.js";

/** Git repo with one commit, so `sh("git", …)` returns real output. */
function repo(): string {
  const dir = mkdtempSync(join(tmpdir(), "llm-wiki-retro-"));
  const git = (...args: string[]) => execFileSync("git", args, { cwd: dir });
  git("init", "-q");
  git("-c", "user.email=t@e.com", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "first");
  return dir;
}

const cleanups: string[] = [];
afterEach(() => {
  for (const dir of cleanups.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe("retro evidence window", () => {
  it("first fire covers the whole session and records nothing", () => {
    const dir = repo();
    cleanups.push(dir);
    const ev = buildEvidence(dir, "proj", 3);
    const doc = readFileSync(ev.path, "utf-8");
    expect(doc).toContain("Window: whole session so far");
    expect(doc).toContain("## recent commits (5)");
    expect(doc).toContain("(none)");
  });

  it("later fires are bounded to the interval since the previous fire", () => {
    const dir = repo();
    cleanups.push(dir);
    const since = Date.parse("2026-09-11T02:00:00Z");
    const ev = buildEvidence(dir, "proj", 1, since, ["sources/earlier-insight"]);
    const doc = readFileSync(ev.path, "utf-8");
    expect(doc).toContain("Window: work since 2026-09-11T02:00:00.000Z only");
    expect(doc).toContain("## commits since 2026-09-11T02:00:00.000Z");
    expect(doc).not.toContain("## recent commits (5)");
    // The don't-duplicate list is the belt to the window's braces.
    expect(doc).toContain("## already recorded by earlier retro runs");
    expect(doc).toContain("- sources/earlier-insight");
  });
});

describe("transcript window", () => {
  const lines = (stamps: string[]) =>
    stamps
      .map((t, i) => JSON.stringify({ type: "message", timestamp: t, body: `entry-${i}` }))
      .join("\n") + "\n";

  it("keeps only entries at or after the window start", () => {
    const dir = mkdtempSync(join(tmpdir(), "llm-wiki-sess-"));
    cleanups.push(dir);
    const file = join(dir, "s.jsonl");
    writeFileSync(
      file,
      lines(["2026-09-11T01:00:00Z", "2026-09-11T01:59:00Z", "2026-09-11T02:00:00Z", "2026-09-11T02:05:00Z"]),
    );
    const slice = transcriptWindow(file, Date.parse("2026-09-11T02:00:00Z"), "proj");
    expect(slice).not.toBe("");
    const kept = readFileSync(slice, "utf-8").trim().split("\n");
    expect(kept).toHaveLength(2);
    expect(kept[0]).toContain("entry-2");
    expect(kept[1]).toContain("entry-3");
  });

  it("returns '' when there is no window or no transcript, so callers fall back", () => {
    const dir = mkdtempSync(join(tmpdir(), "llm-wiki-sess-"));
    cleanups.push(dir);
    const file = join(dir, "s.jsonl");
    writeFileSync(file, lines(["2026-09-11T01:00:00Z"]));
    expect(transcriptWindow(file, null, "proj")).toBe(""); // first fire
    expect(transcriptWindow(join(dir, "missing.jsonl"), 1, "proj")).toBe("");
    expect(transcriptWindow("", 1, "proj")).toBe("");
    // Window inside the file but after every entry — nothing to hand over.
    expect(transcriptWindow(file, Date.parse("2026-09-11T03:00:00Z"), "proj")).toBe("");
  });
});

describe("worker report parsing", () => {
  it("keeps the summary and extracts the bracketed page ids", () => {
    const r = parseReport("noise\nRETRO DONE pages=2 [sources/a, entities/b]\n");
    expect(r.summary).toBe("RETRO DONE pages=2 [sources/a, entities/b]");
    expect(r.ids).toEqual(["sources/a", "entities/b"]);
  });

  it("handles a report without ids and a missing report", () => {
    expect(parseReport("RETRO DONE pages=0")).toEqual({
      summary: "RETRO DONE pages=0",
      ids: [],
    });
    expect(parseReport("nothing useful").ids).toEqual([]);
    expect(parseReport("nothing useful").summary).toContain("no report line");
  });
});
