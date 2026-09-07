import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { DEFAULT_CONFIG, loadConfig, type AutopilotConfig } from "./lib/config.js";
import { ensureWikiReady } from "./lib/bootstrap.js";
import { buildRetroDirective, buildResearchNudge } from "./lib/messages.js";
import { deriveWikiName } from "./lib/wikiName.js";

export interface ExtensionDeps {
  /** Test seam: replaces the mechanical ensureWikiReady bootstrap call. */
  ensureWikiReadyFn?: typeof ensureWikiReady;
}

export default function llmWikiAutopilot(pi: ExtensionAPI, deps: ExtensionDeps = {}): void {
  // Headless workers spawned by the retro directive set this to avoid
  // recursive autopilot firing (bootstrap/retro) inside the worker.
  if (process.env.LLM_WIKI_AUTOPILOT_DISABLE) return;

  const here = dirname(fileURLToPath(import.meta.url));
  const skillsDir = join(here, "..", "..", "skills");
  const skillPath = (name: string) => join(skillsDir, name, "SKILL.md");
  const workerPromptPath = join(here, "worker-retro.md");
  const ensure = deps.ensureWikiReadyFn ?? ensureWikiReady;

  let settledRuns = 0;
  let retroProposed = false;
  let bootstrapRan = false;
  let wikiName: string | null = null;
  let nudge: string | null = null;
  let config: AutopilotConfig = DEFAULT_CONFIG;

  pi.on("session_start", async (_event, ctx) => {
    settledRuns = 0;
    retroProposed = false;
    bootstrapRan = false;
    wikiName = deriveWikiName(ctx.cwd);
    const loaded = loadConfig(ctx.cwd);
    config = loaded.config;
    if (loaded.warning) ctx.ui.notify(loaded.warning, "warning");
    // Session-static system prompt footer: wiki scoping rides the nudge
    // (byte-identical all session — prompt-cache safe).
    nudge = buildResearchNudge(wikiName);
  });

  pi.on("before_agent_start", async (event, ctx) => {
    // Bootstrap holds the user's first turn: mechanical MCP calls (ensure
    // space, rebuild degraded index) run to completion — sub-second — before
    // the first message is processed. Sessions that never receive a message
    // never fire it.
    if (config.bootstrap && !bootstrapRan) {
      bootstrapRan = true;
      try {
        const result = await ensure({
          wikiName: wikiName ?? "default",
          url: config.wikiMcpUrl,
          token: config.wikiMcpToken,
        });
        if (result.space === "error") {
          ctx.ui.notify(`[llm-wiki] bootstrap failed: ${result.detail} — continuing without it (index self-heals via retro)`, "warning");
        } else {
          ctx.ui.notify(`[rust-wiki] space "${wikiName ?? "default"}" ${result.space} — ${result.detail}`, "info");
        }
      } catch (err) {
        ctx.ui.notify(`[llm-wiki] bootstrap failed: ${(err as Error).message} — continuing without it`, "warning");
      }
    }
    if (!config.researchNudge || !nudge) return;
    if (event.systemPrompt.includes(nudge)) return;
    return { systemPrompt: event.systemPrompt + nudge };
  });

  pi.on("agent_settled", async () => {
    const { retro } = config;
    if (!retro.enabled) return;
    if (retro.oncePerSession && retroProposed) return;
    settledRuns += 1;
    if (settledRuns < retro.everyNRuns) return;
    // Re-arm: fires again after another `everyNRuns` settled runs, unless
    // `oncePerSession` pins it to the first fire only.
    settledRuns = 0;
    retroProposed = true;
    await pi.sendMessage(
      buildRetroDirective(skillPath("retro"), workerPromptPath, wikiName, config.display),
      // followUp + triggerTurn: if the agent is idle, start a run immediately
      // so the directive executes instead of waiting for the user's next message.
      { deliverAs: "followUp", triggerTurn: true },
    );
  });
}
