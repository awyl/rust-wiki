import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export interface CrystallizeConfig {
  enabled: boolean;
  everyNRuns: number;
  /** When true, crystallize fires once per session (old behavior). Default: re-arms after each fire. */
  oncePerSession: boolean;
}

export interface AutopilotConfig {
  bootstrap: boolean;
  researchNudge: boolean;
  /** Render directive text in the UI. False = agent still receives it, silently. */
  display: boolean;
  /** Parent directory for new wiki spaces — enables unattended space creation in bootstrap. */
  wikiRoot: string;
  crystallize: CrystallizeConfig;
}

export const DEFAULT_CONFIG: AutopilotConfig = {
  bootstrap: true,
  researchNudge: true,
  display: true,
  wikiRoot: "",
  crystallize: { enabled: true, everyNRuns: 8, oncePerSession: false },
};

export const CONFIG_FILENAME = "llm-wiki.json";

export interface LoadResult {
  config: AutopilotConfig;
  warning?: string;
}

type PartialConfig = Partial<Omit<AutopilotConfig, "crystallize">> & {
  crystallize?: Partial<CrystallizeConfig>;
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
      display: pick("display"),
      wikiRoot: pick("wikiRoot"),
      crystallize: {
        enabled: project.raw?.crystallize?.enabled ?? global.raw?.crystallize?.enabled ?? DEFAULT_CONFIG.crystallize.enabled,
        everyNRuns:
          project.raw?.crystallize?.everyNRuns ?? global.raw?.crystallize?.everyNRuns ?? DEFAULT_CONFIG.crystallize.everyNRuns,
        oncePerSession:
          project.raw?.crystallize?.oncePerSession ??
          global.raw?.crystallize?.oncePerSession ??
          DEFAULT_CONFIG.crystallize.oncePerSession,
      },
    },
    warning,
  };
}
