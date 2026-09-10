import { execFileSync, spawn } from "node:child_process";
import { appendFileSync, openSync } from "node:fs";

export interface WorkerResult {
  ok: boolean;
  summary: string;
}

/** Best-effort synchronous command — never throws, returns a marker instead. */
export function sh(cmd: string, args: string[], cwd: string): string {
  try {
    return execFileSync(cmd, args, { cwd, encoding: "utf-8", timeout: 15000 }).trim();
  } catch {
    return "(unavailable)";
  }
}

const REPORT_RE = /((?:RETRO|SWEEP|BACKFILL|DISCOVER) DONE.*)/;

/**
 * Spawn a headless worker as an extension-side background process — zero model
 * context in the main session. Resolves when the worker exits; the caller
 * surfaces a bootstrap-style UI notice (never context).
 *
 * The log is opened in APPEND mode with a run header. Truncating (`"w"`) raced
 * here: a second run opening the same path truncated the file while the first
 * run's stdout was still pointed at it, so their reports overwrote each other
 * and the parsed summary came out garbled.
 */
export function runWorker(prompt: string, logPath: string, cwd: string): Promise<WorkerResult> {
  return new Promise((resolve) => {
    let logFd: number;
    try {
      appendFileSync(logPath, `\n=== run ${new Date().toISOString()} ===\n`);
      logFd = openSync(logPath, "a");
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
      const tail = sh("tail", ["-5", logPath], cwd);
      const m = tail.match(REPORT_RE);
      resolve({ ok: true, summary: m ? m[1] : "worker done, no report line" });
    });
  });
}
