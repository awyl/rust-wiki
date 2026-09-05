import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { DEFAULT_CONFIG, loadConfig, type AutopilotConfig } from "./lib/config.js";
import {
  buildBootstrapDirective,
  buildCrystallizeDirective,
  RESEARCH_NUDGE,
} from "./lib/messages.js";
import { deriveWikiName } from "./lib/wikiName.js";

export default function llmWikiAutopilot(pi: ExtensionAPI): void {
  // Headless workers spawned by the crystallize directive set this to avoid
  // recursive autopilot firing (bootstrap/crystallize) inside the worker.
  if (process.env.LLM_WIKI_AUTOPILOT_DISABLE) return;

  const here = dirname(fileURLToPath(import.meta.url));
  const skillsDir = join(here, "..", "..", "skills");
  const skillPath = (name: string) => join(skillsDir, name, "SKILL.md");
  const workerPromptPath = join(here, "worker-crystallize.md");

  let settledRuns = 0;
  let crystallizeProposed = false;
  let wikiName: string | null = null;
  let config: AutopilotConfig = DEFAULT_CONFIG;

  pi.on("session_start", async (_event, ctx) => {
    settledRuns = 0;
    crystallizeProposed = false;
    wikiName = deriveWikiName(ctx.cwd);
    const loaded = loadConfig(ctx.cwd);
    config = loaded.config;
    if (loaded.warning) ctx.ui.notify(loaded.warning, "warning");
    if (config.bootstrap) {
      await pi.sendMessage(buildBootstrapDirective(wikiName, config.display, config.wikiRoot), {
        deliverAs: "nextTurn",
      });
    }
  });

  pi.on("before_agent_start", async (event) => {
    if (!config.researchNudge) return;
    if (event.systemPrompt.includes(RESEARCH_NUDGE)) return;
    return { systemPrompt: event.systemPrompt + RESEARCH_NUDGE };
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
      {
        deliverAs: "nextTurn",
      },
    );
  });
}
