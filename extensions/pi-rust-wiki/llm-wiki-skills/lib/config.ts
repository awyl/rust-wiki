import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export interface RetroConfig {
  enabled: boolean;
  everyNRuns: number;
  /** When true, retro fires once per session (old behavior). Default: re-arms after each fire. */
  oncePerSession: boolean;
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
  /** Bearer token for the wiki MCP endpoint (WIKI_TOKEN env; server is unauthenticated by default). */
  wikiMcpToken: string;
  retro: RetroConfig;
}

export const DEFAULT_WIKI_MCP_URL = "http://host.containers.internal:8484/mcp";

export const DEFAULT_CONFIG: AutopilotConfig = {
  bootstrap: true,
  researchNudge: true,
  autoInject: false,
  display: false,
  wikiMcpUrl: DEFAULT_WIKI_MCP_URL,
  wikiMcpToken: process.env.WIKI_TOKEN ?? "",
  retro: { enabled: true, everyNRuns: 8, oncePerSession: false },
};

export const CONFIG_FILENAME = "llm-wiki.json";

export interface LoadResult {
  config: AutopilotConfig;
  warning?: string;
}

type PartialConfig = Partial<Omit<AutopilotConfig, "retro">> & {
  retro?: Partial<RetroConfig>;
};

/** pi's global agent dir, honoring the documented PI_CODING_AGENT_DIR override. */
export function globalAgentDir(): string {
  return process.env.PI_CODING_AGENT_DIR ?? join(homedir(), ".pi", "agent");
}

function readLayer(path: string): { raw?: PartialConfig; warning?: string } {
  if (!existsSync(path)) return {};
  try {
    return { raw: JSON.parse(readFileSync(path, "utf-8")) as PartialConfig };
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
      },
    },
    warning,
  };
}
