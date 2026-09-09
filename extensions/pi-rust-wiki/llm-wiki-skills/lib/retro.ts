import { execFileSync, spawn } from "node:child_process";
import { openSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

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

function sh(cmd: string, args: string[], cwd: string): string {
  try {
    return execFileSync(cmd, args, { cwd, encoding: "utf-8", timeout: 15000 }).trim();
  } catch {
    return "(unavailable)";
  }
}

/**
 * Mechanical session evidence — code-generated, no model context spent.
 * The worker judges non-triviality from this (zosmaai criteria) and may
 * write zero pages.
 */
export function buildEvidence(cwd: string, wikiName: string, mutatingCalls: number): RetroEvidence {
  const lines = [
    `Wiki space: ${wikiName}`,
    `Mutating tool calls in window: ${mutatingCalls}`,
    "",
    "## git status (short)",
    sh("git", ["status", "--short"], cwd),
    "",
    "## git diff --stat (uncommitted)",
    sh("git", ["diff", "--stat"], cwd),
    "",
    "## recent commits (5)",
    sh("git", ["log", "--oneline", "-5"], cwd),
  ];
  const path = join(tmpdir(), `llm-wiki-evidence-${wikiName}.md`);
  writeFileSync(path, lines.join("\n"));
  return { path, mutatingCalls };
}

export interface WorkerResult {
  ok: boolean;
  summary: string;
}

const REPORT_RE = /(RETRO DONE.*|SWEEP DONE.*|BACKFILL DONE.*)/;

/**
 * Spawn the headless worker as an extension-side background process —
 * zero model context in the main session. Resolves when the worker exits;
 * the caller surfaces a bootstrap-style UI notice (never context).
 */
export function spawnWorker(
  workerPromptPath: string,
  evidencePath: string,
  skillPath: string,
  wikiName: string,
  logPath: string,
  sessionFile = "",
): Promise<WorkerResult> {
  return new Promise((resolve) => {
    const prompt = [
      `Read ${workerPromptPath} and follow it.`,
      `Evidence file: ${evidencePath}.`,
      sessionFile ? `Session transcript: ${sessionFile} (read the tail for analysis/decisions not visible in git).` : "No transcript available — judge from the evidence file.",
      `Retro skill: ${skillPath}.`,
      `Wiki: ${wikiName}.`,
      "When done, print a final single line: RETRO DONE pages=<n>.",
    ].join(" ");
    let logFd: number;
    try {
      logFd = openSync(logPath, "w");
    } catch (err) {
      resolve({ ok: false, summary: `cannot open log: ${(err as Error).message}` });
      return;
    }
    const child = spawn("pi", ["-p", prompt], {
      env: { ...process.env, LLM_WIKI_AUTOPILOT_DISABLE: "1" },
      stdio: ["ignore", logFd, logFd],
      detached: false,
    });
    child.on("error", (err) => resolve({ ok: false, summary: `spawn failed: ${err.message}` }));
    child.on("close", (code) => {
      if (code !== 0) {
        resolve({ ok: false, summary: `worker exited ${code} (log: ${logPath})` });
        return;
      }
      const tail = sh("tail", ["-5", logPath], tmpdir());
      const m = tail.match(REPORT_RE);
      resolve({ ok: true, summary: m ? m[1] : "worker done, no report line" });
    });
  });
}
