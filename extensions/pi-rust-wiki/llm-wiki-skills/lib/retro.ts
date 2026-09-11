import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { runWorker, sh, type WorkerResult } from "./worker.js";

export interface RetroEvidence {
  path: string;
  mutatingCalls: number;
}

/** Tool names that mutate on-disk state (bare or namespaced). */
export function isMutatingTool(toolName: string): boolean {
  const base = toolName.includes("__") ? toolName.split("__").pop()! : toolName;
  return base === "edit" || base === "write";
}

/** Combined trigger: cadence window reached AND window did mutating work. */
export function shouldFireRetro(
  settledRuns: number,
  everyNRuns: number,
  mutatingCalls: number,
  minMutatingCalls = 1,
): boolean {
  return settledRuns >= everyNRuns && mutatingCalls >= minMutatingCalls;
}

/**
 * Mechanical session evidence — code-generated, no model context spent.
 *
 * `sinceMs` is the previous retro fire: the evidence covers the interval
 * since then, not "the repo lately". Without it two runs over one long
 * session see the same recent commits and write near-duplicate pages (that
 * happened: `page-types-root-cause` vs `page-types-owns-the-type-folder-map`).
 * `recorded` lists what earlier runs already wrote, so overlap is refused
 * rather than merely discouraged.
 */
export function buildEvidence(
  cwd: string,
  wikiName: string,
  mutatingCalls: number,
  sinceMs: number | null = null,
  recorded: string[] = [],
): RetroEvidence {
  const since = sinceMs === null ? "" : new Date(sinceMs).toISOString();
  const commits =
    sinceMs === null
      ? sh("git", ["log", "--oneline", "-5"], cwd)
      : sh("git", ["log", "--oneline", `--since=${since}`, "-20"], cwd) || "(none in window)";
  const lines = [
    `Wiki space: ${wikiName}`,
    `Mutating tool calls in window: ${mutatingCalls}`,
    sinceMs === null
      ? "Window: whole session so far (no previous retro fire)."
      : `Window: work since ${since} only — everything earlier was already covered by a previous retro run.`,
    "",
    "## git status (short)",
    sh("git", ["status", "--short"], cwd),
    "",
    "## git diff --stat (uncommitted)",
    sh("git", ["diff", "--stat"], cwd),
    "",
    sinceMs === null ? "## recent commits (5)" : `## commits since ${since}`,
    commits,
    "",
    "## already recorded by earlier retro runs (do NOT write these again)",
    recorded.length ? recorded.map((id) => `- ${id}`).join("\n") : "(none)",
  ];
  const path = join(tmpdir(), `llm-wiki-evidence-${wikiName}.md`);
  writeFileSync(path, lines.join("\n"));
  return { path, mutatingCalls };
}

/**
 * Transcript entries from `sinceMs` onward, written next to the evidence.
 *
 * The worker used to get the whole session and "read the tail", so a second
 * run over the same long session re-read the same region. Returns "" when the
 * window can't be determined or the transcript is unreadable — the caller
 * then falls back to the full session file.
 */
export function transcriptWindow(
  sessionFile: string,
  sinceMs: number | null,
  wikiName: string,
): string {
  if (!sessionFile || sinceMs === null) return "";
  let raw: string;
  try {
    raw = readFileSync(sessionFile, "utf-8");
  } catch {
    return "";
  }
  const kept: string[] = [];
  let started = false;
  for (const line of raw.split("\n")) {
    if (!line.trim()) continue;
    let ts: number | null = null;
    try {
      const t = (JSON.parse(line) as { timestamp?: unknown }).timestamp;
      ts = typeof t === "string" ? Date.parse(t) : null;
    } catch {
      // Partial trailing write — keep it only once the window has opened.
    }
    if (ts === null) {
      if (started) kept.push(line);
      continue;
    }
    if (ts >= sinceMs) {
      started = true;
      kept.push(line);
    }
  }
  if (!kept.length) return "";
  const path = join(tmpdir(), `llm-wiki-transcript-${wikiName}.jsonl`);
  try {
    writeFileSync(path, `${kept.join("\n")}\n`);
  } catch {
    return "";
  }
  return path;
}

export type { WorkerResult };

/**
 * Fire the retro worker. Same background mechanism as discovery; the prompt
 * carries the evidence file, the session transcript tail, and the skill.
 */
export function spawnWorker(
  workerPromptPath: string,
  evidencePath: string,
  skillPath: string,
  wikiName: string,
  logPath: string,
  sessionFile = "",
  windowed = false,
): Promise<WorkerResult> {
  const prompt = [
    `Read ${workerPromptPath} and follow it.`,
    `Evidence file: ${evidencePath}.`,
    sessionFile
      ? windowed
        ? `Session transcript, already cut to your window — read all of it: ${sessionFile}.`
        : `Session transcript: ${sessionFile} (read the tail for analysis/decisions not visible in git).`
      : "No transcript available — judge from the evidence file.",
    `Retro skill: ${skillPath}.`,
    `Wiki: ${wikiName}.`,
    "Record only what happened inside the stated window, and never restate a page the evidence lists as already recorded.",
    "When done, print a final single line: RETRO DONE pages=<n> [<ids>].",
  ].join(" ");
  return runWorker(prompt, logPath, tmpdir());
}
