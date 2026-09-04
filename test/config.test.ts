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

  it("keeps an explicit wiki space name", () => {
    write(JSON.stringify({ wiki: "research" }));
    const { config } = loadConfig(dir);
    expect(config.wiki).toBe("research");
  });

  it("falls back to defaults with a warning on malformed JSON", () => {
    write("{ not json");
    const { config, warning } = loadConfig(dir);
    expect(config).toEqual(DEFAULT_CONFIG);
    expect(warning).toContain("llm-wiki.json");
  });
});
