import { runWorker, type WorkerResult } from "./worker.js";

export interface DiscoverInput {
  workerPromptPath: string;
  wikiName: string;
  /** Empty = let the worker work the vault's own gaps. */
  topics: string[];
  maxCaptures: number;
  dryRun: boolean;
  logPath: string;
}

/**
 * Fire the scheduled discovery worker from the launch line. The worker does
 * the reasoning; this only hands it the bounds (topics, caps, dry-run) and
 * the wiki space — the main session never sees the result.
 */
export function spawnDiscoverWorker(input: DiscoverInput): Promise<WorkerResult> {
  const topics = input.topics.length
    ? input.topics.join(", ")
    : "(none configured — work the vault's own gaps)";
  const prompt = [
    `Read ${input.workerPromptPath} and follow it.`,
    `Wiki: ${input.wikiName}.`,
    `Topics: ${topics}.`,
    `Max captures this run: ${input.maxCaptures}.`,
    input.dryRun ? "DRY RUN: capture and write nothing; report what you would have captured." : "",
    "When done, print a final single line: DISCOVER DONE captured=<n> topic=<topic>.",
  ]
    .filter(Boolean)
    .join(" ");
  return runWorker(prompt, input.logPath, process.cwd());
}
