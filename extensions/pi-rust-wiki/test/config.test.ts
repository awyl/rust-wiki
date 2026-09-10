import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_CONFIG, loadConfig } from "../llm-wiki-skills/lib/config.js";

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
    write(JSON.stringify({ bootstrap: false, retro: { everyNRuns: 3 } }));
    const { config } = loadConfig(dir);
    expect(config).toEqual({
      bootstrap: false,
      researchNudge: true,
      autoInject: false,
      display: false,
      wikiMcpUrl: "http://host.containers.internal:9999/mcp/wiki",
      wikiMcpToken: process.env.WIKI_TOKEN ?? process.env.AIPROXY_TOKEN ?? "",
      retro: {
        enabled: true,
        everyNRuns: 3,
        oncePerSession: false,
        minMutatingCalls: 0,
      },
      discover: { enabled: true, everyNRuns: 24, topics: [], maxCaptures: 3, dryRun: false },
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
    writeGlobal(JSON.stringify({ researchNudge: false, retro: { everyNRuns: 12 } }), globalDir);
    const { config } = loadConfig(dir, globalDir);
    expect(config.researchNudge).toBe(false);
    expect(config.retro.everyNRuns).toBe(12);
  });

  it("lets project settings override global settings key-by-key", () => {
    const globalDir = join(dir, "global");
    writeGlobal(
      JSON.stringify({ bootstrap: false, researchNudge: false, retro: { enabled: false, everyNRuns: 12 } }),
      globalDir,
    );
    write(JSON.stringify({ researchNudge: true, retro: { everyNRuns: 2 } }));
    const { config } = loadConfig(dir, globalDir);
    expect(config).toEqual({
      bootstrap: false,
      researchNudge: true,
      autoInject: false,
      display: false,
      wikiMcpUrl: "http://host.containers.internal:9999/mcp/wiki",
      wikiMcpToken: process.env.WIKI_TOKEN ?? process.env.AIPROXY_TOKEN ?? "",
      retro: {
        enabled: false,
        everyNRuns: 2,
        oncePerSession: false,
        minMutatingCalls: 0,
      },
      discover: { enabled: true, everyNRuns: 24, topics: [], maxCaptures: 3, dryRun: false },
    });
  });

  it("reads the discover block and drops non-string topics", () => {
    write(
      JSON.stringify({
        discover: { enabled: true, everyNRuns: 5, topics: ["rust", 7, "", "embeddings"], maxCaptures: 1, dryRun: true },
      }),
    );
    const { config } = loadConfig(dir);
    expect(config.discover).toEqual({
      enabled: true,
      everyNRuns: 5,
      topics: ["rust", "embeddings"],
      maxCaptures: 1,
      dryRun: true,
    });
  });

  it("discovery defaults on, and a layer can turn it off", () => {
    expect(DEFAULT_CONFIG.discover.enabled).toBe(true);
    write(JSON.stringify({ retro: { everyNRuns: 2 } }));
    expect(loadConfig(dir).config.discover.enabled).toBe(true);
  });

  it("accepts inline comments but keeps // inside values", () => {
    write(`{
  // documented config
  "bootstrap": false, /* keep serving */
  "wikiMcpUrl": "http://host.containers.internal:8484/mcp"
}
`);
    const { config, warning } = loadConfig(dir);
    expect(warning).toBeUndefined();
    expect(config.bootstrap).toBe(false);
    // the URL keeps its double slash — the stripper is string-aware
    expect(config.wikiMcpUrl).toBe("http://host.containers.internal:8484/mcp");
  });

  it("a comment cannot hide a real syntax error", () => {
    write(`{ "bootstrap": false,, } // trailing junk`);
    const { config, warning } = loadConfig(dir);
    expect(config).toEqual(DEFAULT_CONFIG);
    expect(warning).toContain("llm-wiki.json");
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
