import { writeFileSync } from "node:fs";
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
): Promise<WorkerResult> {
  const prompt = [
    `Read ${workerPromptPath} and follow it.`,
    `Evidence file: ${evidencePath}.`,
    sessionFile
      ? `Session transcript: ${sessionFile} (read the tail for analysis/decisions not visible in git).`
      : "No transcript available — judge from the evidence file.",
    `Retro skill: ${skillPath}.`,
    `Wiki: ${wikiName}.`,
    "When done, print a final single line: RETRO DONE pages=<n>.",
  ].join(" ");
  return runWorker(prompt, logPath, tmpdir());
}
