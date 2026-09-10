import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { DEFAULT_CONFIG, loadConfig, type AutopilotConfig } from "./lib/config.js";
import { ensureWikiReady } from "./lib/bootstrap.js";
import { buildResearchNudge } from "./lib/messages.js";
import { buildEvidence, isMutatingTool, shouldFireRetro, spawnWorker } from "./lib/retro.js";
import { spawnDiscoverWorker } from "./lib/discover.js";
import { notify } from "./lib/notify.js";
import {
  buildHealthHint,
  buildRecallMessage,
  healthForPrompt,
  recallForPrompt,
  type WikiStatusDTO,
} from "./lib/inject.js";
import { deriveWikiName } from "./lib/wikiName.js";

export interface ExtensionDeps {
  /** Test seam: replaces the mechanical ensureWikiReady bootstrap call. */
  ensureWikiReadyFn?: typeof ensureWikiReady;
  /** Test seam: replaces the wiki_status health probe. */
  healthFn?: typeof healthForPrompt;
  /** Test seam: replaces the detached worker spawn. */
  spawnWorkerFn?: typeof spawnWorker;
  /** Test seam: replaces the detached discovery spawn. */
  spawnDiscoverWorkerFn?: typeof spawnDiscoverWorker;
}

export default function llmWikiAutopilot(pi: ExtensionAPI, deps: ExtensionDeps = {}): void {
  // Headless workers spawned by the retro directive set this to avoid
  // recursive autopilot firing (bootstrap/retro) inside the worker.
  if (process.env.LLM_WIKI_AUTOPILOT_DISABLE) return;

  const here = dirname(fileURLToPath(import.meta.url));
  const skillsDir = join(here, "..", "..", "skills");
  const skillPath = (name: string) => join(skillsDir, name, "SKILL.md");
  const workerPromptPath = join(here, "worker-retro.md");
  const discoverPromptPath = join(here, "worker-discover.md");
  const ensure = deps.ensureWikiReadyFn ?? ensureWikiReady;
  const health = deps.healthFn ?? healthForPrompt;
  const spawnW = deps.spawnWorkerFn ?? spawnWorker;
  const spawnD = deps.spawnDiscoverWorkerFn ?? spawnDiscoverWorker;

  let settledRuns = 0;
  let discoverRuns = 0;
  let mutatingCalls = 0;
  let retroProposed = false;
  // Single-flight: retro and discovery never run at once. A window that
  // finds the flag set keeps its counters and retries on the next run.
  let workerInFlight = false;
  let bootstrapRan = false;
  // Space pin: first successful wiki_use_space wins. Personal switching is
  // prohibited — personal writes go through the dedicated personal tools.
  let pinnedSpace: string | null = null;
  let wikiName: string | null = null;
  let nudge: string | null = null;
  let config: AutopilotConfig = DEFAULT_CONFIG;
  // Same-prompt dedupe: an aborted+retried turn re-fires before_agent_start
  // with the identical prompt — inject at most once per distinct prompt.
  let lastInjectedPrompt: string | null = null;
  // Health problems surface once per session up front, and again whenever
  // retro fires (they may have accumulated since).
  let healthChecked = false;

  const isUseSpaceCall = (toolName: string) =>
    toolName === "wiki_use_space" || toolName.endsWith("__wiki_use_space");

  pi.on("tool_call", async (event) => {
    const e = event as unknown as { toolName?: string; input?: { space?: string } };
    if (!e.toolName) return;
    // Non-trivial gate: count file-mutating calls in the window.
    if (isMutatingTool(e.toolName)) mutatingCalls += 1;
    if (!isUseSpaceCall(e.toolName)) return;
    const requested = e.input?.space;
    if (!requested) return;
    if (requested === "personal") {
      return {
        block: true,
        reason:
          'wiki_use_space("personal") is prohibited — write to the personal layer with wiki_ensure_personal_page / wiki_write_personal_page instead (no space switch needed).',
      };
    }
    if (pinnedSpace && requested !== pinnedSpace) {
      return {
        block: true,
        reason: `Wiki space already pinned to "${pinnedSpace}" this session — wiki_use_space("${requested}") blocked.`,
      };
    }
  });

  pi.on("tool_result", async (event) => {
    const e = event as unknown as { toolName?: string; input?: { space?: string }; isError?: boolean };
    if (!e.toolName || !isUseSpaceCall(e.toolName) || e.isError) return;
    const requested = e.input?.space;
    if (requested && requested !== "personal" && !pinnedSpace) pinnedSpace = requested;
  });

  pi.on("session_start", async (_event, ctx) => {
    settledRuns = 0;
    discoverRuns = 0;
    mutatingCalls = 0;
    retroProposed = false;
    bootstrapRan = false;
    pinnedSpace = null;
    wikiName = deriveWikiName(ctx.cwd);
    const loaded = loadConfig(ctx.cwd);
    config = loaded.config;
    if (loaded.warning) notify(ctx, loaded.warning, "warning");
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
          notify(
            ctx,
            `[llm-wiki] bootstrap failed: ${result.detail} — continuing without it (index self-heals via retro)`,
            "warning",
          );
        } else {
          notify(ctx, `[rust-wiki] space "${wikiName ?? "default"}" ${result.space} — ${result.detail}`, "info");
        }
      } catch (err) {
        notify(ctx, `[llm-wiki] bootstrap failed: ${(err as Error).message} — continuing without it`, "warning");
      }
    }
    // Once-per-session health probe (runs regardless of autoInject): push
    // wiki problems into the UI instead of leaving them in pull-only tools.
    if (!healthChecked && wikiName) {
      healthChecked = true;
      const status = await health(config.wikiMcpUrl, config.wikiMcpToken, wikiName);
      const hint = status ? buildHealthHint(status) : undefined;
      if (hint) notify(ctx, `[rust-wiki] ${hint}`, "warning");
    }

    // Per-turn recall injection (opt-in). Volatile content NEVER enters the
    // system prompt — it rides a hidden tail message so the provider's
    // prompt-cache prefix (system prompt + nudge footer) stays stable.
    let recallMessage: ReturnType<typeof buildRecallMessage>;
    if (config.autoInject && wikiName && event.prompt.trim() && event.prompt !== lastInjectedPrompt) {
      lastInjectedPrompt = event.prompt;
      const [matches, status] = await Promise.all([
        recallForPrompt(config.wikiMcpUrl, config.wikiMcpToken, wikiName, event.prompt),
        health(config.wikiMcpUrl, config.wikiMcpToken, wikiName) as Promise<WikiStatusDTO | null>,
      ]);
      recallMessage = buildRecallMessage(matches);
      const hint = status ? buildHealthHint(status) : undefined;
      if (recallMessage && hint) recallMessage.content += `\n\n${hint}`;
    }

    const result: { systemPrompt?: string; message?: NonNullable<ReturnType<typeof buildRecallMessage>> } = {};
    if (config.researchNudge && nudge && !event.systemPrompt.includes(nudge)) {
      result.systemPrompt = event.systemPrompt + nudge;
    }
    if (recallMessage) result.message = recallMessage;
    if (!result.systemPrompt && !result.message) return;
    return result;
  });

  pi.on("agent_settled", async (_event, ctx) => {
    settledRuns += 1;
    discoverRuns += 1;

    // Discovery runs first: the retro block below uses early returns for its
    // own gate, and both share the single-flight guard.
    const { discover } = config;
    if (!discover.enabled) {
      discoverRuns = 0;
    } else if (discoverRuns >= discover.everyNRuns && !workerInFlight) {
      discoverRuns = 0;
      const space = wikiName ?? "default";
      const logPath = `/tmp/llm-wiki-discover-${space}.log`;
      workerInFlight = true;
      void (async () => {
        try {
          const r = await spawnD({
            workerPromptPath: discoverPromptPath,
            wikiName: space,
            topics: discover.topics,
            maxCaptures: discover.maxCaptures,
            dryRun: discover.dryRun,
            logPath,
          });
          notify(ctx, `[rust-wiki] discover: ${r.summary}`, r.ok ? "info" : "warning");
        } catch (err) {
          notify(ctx, `[rust-wiki] discover worker failed: ${(err as Error).message}`, "warning");
        } finally {
          workerInFlight = false;
        }
      })();
    }

    const { retro } = config;
    if (!retro.enabled) return;
    if (retro.oncePerSession && retroProposed) return;
    if (
      !shouldFireRetro(settledRuns, retro.everyNRuns, mutatingCalls, retro.minMutatingCalls) &&
      settledRuns < retro.everyNRuns * 10
    ) {
      // Window not reached, or reached but the window did no mutating
      // work (trivial stretch) — silent reset, no worker, no notice.
      if (settledRuns >= retro.everyNRuns) {
        settledRuns = 0;
        mutatingCalls = 0;
      }
      return;
    }
    // Re-arm: fires again after another `everyNRuns` settled runs, unless
    // `oncePerSession` pins it to the first fire only. Backstop (x10) keeps
    // a long trivial stretch from deferring retro forever.
    // A worker is already running — keep the counters so the next settled
    // run retries this window instead of dropping it.
    if (workerInFlight) return;
    const space = wikiName ?? "default";
    const evidence = buildEvidence(ctx.cwd, space, mutatingCalls);
    // Transcript access: the worker judges non-triviality from the session
    // itself (analysis/decisions live there, not in git). Best-effort —
    // the worker falls back to git evidence when the file is unavailable.
    let sessionFile = "";
    try {
      sessionFile = ctx.sessionManager?.getSessionFile?.() ?? "";
    } catch {
      sessionFile = "";
    }
    settledRuns = 0;
    mutatingCalls = 0;
    retroProposed = true;
    const logPath = `/tmp/llm-wiki-retro-${space}.log`;
    // Fully background: extension-side child process, zero model context.
    // Bootstrap-style UI notice on completion — never injected context.
    workerInFlight = true;
    void (async () => {
      try {
        const r = await spawnW(
          workerPromptPath,
          evidence.path,
          skillPath("retro"),
          space,
          logPath,
          sessionFile,
        );
        notify(ctx, `[rust-wiki] retro: ${r.summary}`, r.ok ? "info" : "warning");
      } catch (err) {
        notify(ctx, `[rust-wiki] retro worker failed: ${(err as Error).message}`, "warning");
      } finally {
        workerInFlight = false;
      }
    })();
  });
}
