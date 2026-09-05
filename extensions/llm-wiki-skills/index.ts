import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { DEFAULT_CONFIG, loadConfig, type AutopilotConfig } from "./lib/config.js";
import { buildCrystallizeDirective, buildResearchNudge } from "./lib/messages.js";
import { deriveWikiName } from "./lib/wikiName.js";

export interface ExtensionDeps {
  /** Test seam: runs the detached bootstrap worker command. */
  spawnDetached?: (command: string) => void;
}

function defaultSpawnDetached(command: string): void {
  const child = spawn("bash", ["-c", command], { detached: true, stdio: "ignore" });
  child.unref();
}

export default function llmWikiAutopilot(pi: ExtensionAPI, deps: ExtensionDeps = {}): void {
  // Headless workers spawned by this extension set this to avoid
  // recursive autopilot firing (bootstrap/crystallize) inside the worker.
  if (process.env.LLM_WIKI_AUTOPILOT_DISABLE) return;

  const here = dirname(fileURLToPath(import.meta.url));
  const skillsDir = join(here, "..", "..", "skills");
  const skillPath = (name: string) => join(skillsDir, name, "SKILL.md");
  const workerPromptPath = join(here, "worker-crystallize.md");
  const bootstrapWorkerPath = join(here, "bootstrap-worker.md");
  const spawnDetached = deps.spawnDetached ?? defaultSpawnDetached;

  let settledRuns = 0;
  let crystallizeProposed = false;
  let wikiName: string | null = null;
  let nudge: string | null = null;
  let config: AutopilotConfig = DEFAULT_CONFIG;

  pi.on("session_start", async (_event, ctx) => {
    settledRuns = 0;
    crystallizeProposed = false;
    wikiName = deriveWikiName(ctx.cwd);
    const loaded = loadConfig(ctx.cwd);
    config = loaded.config;
    if (loaded.warning) ctx.ui.notify(loaded.warning, "warning");
    // Session-static system prompt footer: wiki scoping rides the nudge
    // (byte-identical all session — prompt-cache safe), so bootstrap needs
    // no agent turn at all.
    nudge = buildResearchNudge(wikiName);
    if (config.bootstrap) {
      const logPath = `/tmp/llm-wiki-bootstrap-${wikiName ?? "default"}.log`;
      spawnDetached(
        `LLM_WIKI_AUTOPILOT_DISABLE=1 pi -p "Read ${bootstrapWorkerPath} and follow it. Wiki: ${wikiName ?? "default"}. WikiRoot: ${config.wikiRoot || "unset"}." > ${logPath} 2>&1 &`,
      );
    }
  });

  pi.on("before_agent_start", async (event) => {
    if (!config.researchNudge || !nudge) return;
    if (event.systemPrompt.includes(nudge)) return;
    return { systemPrompt: event.systemPrompt + nudge };
  });

  pi.on("agent_settled", async () => {
    const { crystallize } = config;
    if (!crystallize.enabled) return;
    if (crystallize.oncePerSession && crystallizeProposed) return;
    settledRuns += 1;
    if (settledRuns < crystallize.everyNRuns) return;
    // Re-arm: fires again after another `everyNRuns` settled runs, unless
    // `oncePerSession` pins it to the first fire only.
    settledRuns = 0;
    crystallizeProposed = true;
    await pi.sendMessage(
      buildCrystallizeDirective(skillPath("crystallize"), workerPromptPath, wikiName, config.display),
      // followUp + triggerTurn: if the agent is idle, start a run immediately
      // so the directive executes instead of waiting for the user's next message.
      { deliverAs: "followUp", triggerTurn: true },
    );
  });
}
