import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { loadConfig } from "./lib/config.js";
import {
  buildBootstrapDirective,
  buildCrystallizeDirective,
  RESEARCH_NUDGE,
} from "./lib/messages.js";

export default function llmWikiAutopilot(pi: ExtensionAPI): void {
  const here = dirname(fileURLToPath(import.meta.url));
  const skillsDir = join(here, "..", "..", "skills");
  const skillPath = (name: string) => join(skillsDir, name, "SKILL.md");

  if (!existsSync(skillsDir)) {
    // Hooks still register: skills may load from another pi skill location.
    console.warn(
      "[llm-wiki-autopilot] skills/ not found next to extension; directives will point at a missing path",
    );
  }

  let settledRuns = 0;
  let crystallizeProposed = false;

  pi.on("session_start", async (_event, ctx) => {
    settledRuns = 0;
    crystallizeProposed = false;
    const { config, warning } = loadConfig(ctx.cwd);
    if (warning) ctx.ui.notify(warning, "warning");
    if (config.bootstrap) {
      await pi.sendMessage(buildBootstrapDirective(skillPath("bootstrap")), {
        deliverAs: "nextTurn",
      });
    }
  });

  pi.on("before_agent_start", async (event) => {
    const { config } = loadConfig(process.cwd());
    if (!config.researchNudge) return;
    if (event.systemPrompt.includes(RESEARCH_NUDGE)) return;
    return { systemPrompt: event.systemPrompt + RESEARCH_NUDGE };
  });

  pi.on("agent_settled", async () => {
    const { config } = loadConfig(process.cwd());
    if (!config.crystallize.enabled || crystallizeProposed) return;
    settledRuns += 1;
    if (settledRuns >= config.crystallize.everyNRuns) {
      crystallizeProposed = true;
      await pi.sendMessage(buildCrystallizeDirective(skillPath("crystallize")), {
        deliverAs: "nextTurn",
      });
    }
  });
}
