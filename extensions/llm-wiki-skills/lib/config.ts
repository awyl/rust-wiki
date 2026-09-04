import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

export interface CrystallizeConfig {
  enabled: boolean;
  everyNRuns: number;
}

export interface AutopilotConfig {
  bootstrap: boolean;
  researchNudge: boolean;
  crystallize: CrystallizeConfig;
  /** Wiki space this project targets; unset = use the default space. */
  wiki?: string;
}

export const DEFAULT_CONFIG: AutopilotConfig = {
  bootstrap: true,
  researchNudge: true,
  crystallize: { enabled: true, everyNRuns: 8 },
};

export const CONFIG_FILENAME = "llm-wiki.json";

export interface LoadResult {
  config: AutopilotConfig;
  warning?: string;
}

export function loadConfig(cwd: string): LoadResult {
  const path = join(cwd, ".pi", CONFIG_FILENAME);
  if (!existsSync(path)) return { config: DEFAULT_CONFIG };
  try {
    const raw = JSON.parse(readFileSync(path, "utf-8")) as Partial<AutopilotConfig>;
    return {
      config: {
        bootstrap: raw.bootstrap ?? DEFAULT_CONFIG.bootstrap,
        researchNudge: raw.researchNudge ?? DEFAULT_CONFIG.researchNudge,
        crystallize: {
          enabled: raw.crystallize?.enabled ?? DEFAULT_CONFIG.crystallize.enabled,
          everyNRuns: raw.crystallize?.everyNRuns ?? DEFAULT_CONFIG.crystallize.everyNRuns,
        },
        wiki: raw.wiki,
      },
    };
  } catch (err) {
    return {
      config: DEFAULT_CONFIG,
      warning: `[llm-wiki-autopilot] malformed ${path}, using defaults: ${(err as Error).message}`,
    };
  }
}
