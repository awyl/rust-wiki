import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_CONFIG, loadConfig } from "../extensions/llm-wiki-skills/lib/config.js";

describe("loadConfig", () => {
  let dir: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "wiki-cfg-"));
  });
  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  const write = (body: string) => {
    mkdirSync(join(dir, ".pi"));
    writeFileSync(join(dir, ".pi", "llm-wiki.json"), body);
  };

  const writeGlobal = (body: string, globalDir: string) => {
    mkdirSync(globalDir, { recursive: true });
    writeFileSync(join(globalDir, "llm-wiki.json"), body);
  };

  it("returns defaults when no config file exists", () => {
    const { config, warning } = loadConfig(dir);
    expect(config).toEqual(DEFAULT_CONFIG);
    expect(warning).toBeUndefined();
  });

  it("merges partial config over defaults", () => {
    write(JSON.stringify({ bootstrap: false, crystallize: { everyNRuns: 3 } }));
    const { config } = loadConfig(dir);
    expect(config).toEqual({
      bootstrap: false,
      researchNudge: true,
      crystallize: { enabled: true, everyNRuns: 3 },
    });
  });

  it("falls back to defaults with a warning on malformed JSON", () => {
    write("{ not json");
    const { config, warning } = loadConfig(dir);
    expect(config).toEqual(DEFAULT_CONFIG);
    expect(warning).toContain("llm-wiki.json");
  });

  it("applies global settings when the project has no config", () => {
    const globalDir = join(dir, "global");
    writeGlobal(JSON.stringify({ researchNudge: false, crystallize: { everyNRuns: 12 } }), globalDir);
    const { config } = loadConfig(dir, globalDir);
    expect(config.researchNudge).toBe(false);
    expect(config.crystallize.everyNRuns).toBe(12);
  });

  it("lets project settings override global settings key-by-key", () => {
    const globalDir = join(dir, "global");
    writeGlobal(
      JSON.stringify({ bootstrap: false, researchNudge: false, crystallize: { enabled: false, everyNRuns: 12 } }),
      globalDir,
    );
    write(JSON.stringify({ researchNudge: true, crystallize: { everyNRuns: 2 } }));
    const { config } = loadConfig(dir, globalDir);
    expect(config).toEqual({
      bootstrap: false,
      researchNudge: true,
      crystallize: { enabled: false, everyNRuns: 2 },
    });
  });

  it("ignores a missing global dir", () => {
    write(JSON.stringify({ bootstrap: false }));
    const { config, warning } = loadConfig(dir, join(dir, "nonexistent"));
    expect(config.bootstrap).toBe(false);
    expect(warning).toBeUndefined();
  });

  it("skips a malformed global config with a warning and keeps project settings", () => {
    const globalDir = join(dir, "global");
    writeGlobal("{ not json", globalDir);
    write(JSON.stringify({ bootstrap: false }));
    const { config, warning } = loadConfig(dir, globalDir);
    expect(config.bootstrap).toBe(false);
    expect(warning).toContain("llm-wiki.json");
  });
});
