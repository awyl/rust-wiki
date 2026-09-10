import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export interface RetroConfig {
  enabled: boolean;
  everyNRuns: number;
  /** When true, retro fires once per session (old behavior). Default: re-arms after each fire. */
  oncePerSession: boolean;
  /** Minimum mutating (edit/write) tool calls in the window to fire. 0 = every window fires. */
  minMutatingCalls: number;
}

/** Scheduled web discovery: finds outside sources the sessions never captured. */
export interface DiscoverConfig {
  /** Off by default — a discovery pass costs a worker run and wiki pages. */
  enabled: boolean;
  /** One full discovery pass every N settled runs. */
  everyNRuns: number;
  /** Seed topics. Empty = work the vault's own gaps. */
  topics: string[];
  /** Hard cap on captures per run. */
  maxCaptures: number;
  /** Inspect and report only — capture/synthesize nothing. */
  dryRun: boolean;
}

export interface AutopilotConfig {
  bootstrap: boolean;
  researchNudge: boolean;
  /** Per-turn recall injection: search the space on each prompt, inject strong hits as a hidden message. Default off. */
  autoInject: boolean;
  /** Render directive text in the UI. False = agent still receives it, silently. */
  display: boolean;
  /** Wiki MCP endpoint for the mechanical bootstrap calls. */
  wikiMcpUrl: string;
  /** Bearer token for the wiki MCP endpoint (WIKI_TOKEN env, falling back to AIPROXY_TOKEN for aiproxy-hosted servers; server is unauthenticated by default). */
  wikiMcpToken: string;
  retro: RetroConfig;
  discover: DiscoverConfig;
}

export const DEFAULT_WIKI_MCP_URL = "http://host.containers.internal:9999/mcp/wiki";

export const DEFAULT_CONFIG: AutopilotConfig = {
  bootstrap: true,
  researchNudge: true,
  autoInject: false,
  display: false,
  wikiMcpUrl: DEFAULT_WIKI_MCP_URL,
  wikiMcpToken: process.env.WIKI_TOKEN ?? process.env.AIPROXY_TOKEN ?? "",
  retro: { enabled: true, everyNRuns: 8, oncePerSession: false, minMutatingCalls: 0 },
  discover: { enabled: true, everyNRuns: 24, topics: [], maxCaptures: 3, dryRun: false },
};

export const CONFIG_FILENAME = "llm-wiki.json";

export interface LoadResult {
  config: AutopilotConfig;
  warning?: string;
}

type PartialConfig = Partial<Omit<AutopilotConfig, "retro" | "discover">> & {
  retro?: Partial<RetroConfig>;
  discover?: Partial<DiscoverConfig>;
};

/** pi's global agent dir, honoring the documented PI_CODING_AGENT_DIR override. */
export function globalAgentDir(): string {
  return process.env.PI_CODING_AGENT_DIR ?? join(homedir(), ".pi", "agent");
}

/**
 * Strip `//` line and block comments so the config can be documented inline.
 * String-aware: a `//` inside a value (e.g. `"http://host"`) is left alone,
 * including escaped quotes. Newlines are preserved so parse errors still
 * point at the right line.
 */
export function stripJsonComments(text: string): string {
  let out = "";
  let inString = false;
  let inLine = false;
  let inBlock = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    const next = text[i + 1];
    if (inLine) {
      if (c === "\n") {
        inLine = false;
        out += c;
      }
      continue;
    }
    if (inBlock) {
      if (c === "*" && next === "/") {
        inBlock = false;
        i++;
      } else if (c === "\n") {
        out += c;
      }
      continue;
    }
    if (inString) {
      out += c;
      if (c === "\\") {
        out += next ?? "";
        i++;
      } else if (c === '"') {
        inString = false;
      }
      continue;
    }
    if (c === '"') {
      inString = true;
      out += c;
      continue;
    }
    if (c === "/" && next === "/") {
      inLine = true;
      i++;
      continue;
    }
    if (c === "/" && next === "*") {
      inBlock = true;
      i++;
      continue;
    }
    out += c;
  }
  return out;
}

function readLayer(path: string): { raw?: PartialConfig; warning?: string } {
  if (!existsSync(path)) return {};
  try {
    const text = stripJsonComments(readFileSync(path, "utf-8"));
    return { raw: JSON.parse(text) as PartialConfig };
  } catch (err) {
    return { warning: `[llm-wiki-autopilot] malformed ${path}, ignoring it: ${(err as Error).message}` };
  }
}

export function loadConfig(cwd: string, globalDir: string = globalAgentDir()): LoadResult {
  const global = readLayer(join(globalDir, CONFIG_FILENAME));
  const project = readLayer(join(cwd, ".pi", CONFIG_FILENAME));
  const warning = [global.warning, project.warning].find(Boolean);

  const pick = <K extends keyof AutopilotConfig>(key: K): AutopilotConfig[K] =>
    (project.raw?.[key] ?? global.raw?.[key] ?? DEFAULT_CONFIG[key]) as AutopilotConfig[K];

  return {
    config: {
      bootstrap: pick("bootstrap"),
      researchNudge: pick("researchNudge"),
      autoInject: pick("autoInject"),
      display: pick("display"),
      wikiMcpUrl: pick("wikiMcpUrl"),
      wikiMcpToken: pick("wikiMcpToken"),
      retro: {
        enabled: project.raw?.retro?.enabled ?? global.raw?.retro?.enabled ?? DEFAULT_CONFIG.retro.enabled,
        everyNRuns:
          project.raw?.retro?.everyNRuns ?? global.raw?.retro?.everyNRuns ?? DEFAULT_CONFIG.retro.everyNRuns,
        oncePerSession:
          project.raw?.retro?.oncePerSession ??
          global.raw?.retro?.oncePerSession ??
          DEFAULT_CONFIG.retro.oncePerSession,
        minMutatingCalls:
          project.raw?.retro?.minMutatingCalls ??
          global.raw?.retro?.minMutatingCalls ??
          DEFAULT_CONFIG.retro.minMutatingCalls,
      },
      discover: {
        enabled:
          project.raw?.discover?.enabled ??
          global.raw?.discover?.enabled ??
          DEFAULT_CONFIG.discover.enabled,
        everyNRuns:
          project.raw?.discover?.everyNRuns ??
          global.raw?.discover?.everyNRuns ??
          DEFAULT_CONFIG.discover.everyNRuns,
        topics: (
          project.raw?.discover?.topics ??
          global.raw?.discover?.topics ??
          DEFAULT_CONFIG.discover.topics
        ).filter((t): t is string => typeof t === "string" && t.trim().length > 0),
        maxCaptures:
          project.raw?.discover?.maxCaptures ??
          global.raw?.discover?.maxCaptures ??
          DEFAULT_CONFIG.discover.maxCaptures,
        dryRun:
          project.raw?.discover?.dryRun ??
          global.raw?.discover?.dryRun ??
          DEFAULT_CONFIG.discover.dryRun,
      },
    },
    warning,
  };
}
