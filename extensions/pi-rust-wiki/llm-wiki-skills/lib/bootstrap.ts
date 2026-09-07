import { callTool, type McpCallOptions } from "./mcpClient.js";

export interface BootstrapResult {
  space: "ok" | "created" | "error";
  index: "ok" | "n/a" | "error";
  detail: string;
}

export interface BootstrapInput extends McpCallOptions {
  wikiName: string;
}

function parseUseSpace(raw: string): { exists: boolean; totalPages: number | null } {
  try {
    const parsed = JSON.parse(raw);
    return { exists: Boolean(parsed?.exists), totalPages: parsed?.total_pages ?? null };
  } catch {
    return { exists: false, totalPages: null };
  }
}

/**
 * Mechanical bootstrap against rust-wiki: ensure the space exists.
 * rust-wiki keeps projections fresh on every write — there is no
 * degraded-index state and no wikiRoot (spaces are server-side dirs).
 * Never throws.
 */
export async function ensureWikiReady(input: BootstrapInput): Promise<BootstrapResult> {
  const opts: McpCallOptions = { url: input.url, token: input.token, timeoutMs: input.timeoutMs };
  try {
    const first = parseUseSpace(await callTool("wiki_use_space", { space: input.wikiName }, opts));
    if (!first.exists) {
      await callTool("wiki_bootstrap", { space: input.wikiName }, opts);
      const second = parseUseSpace(await callTool("wiki_use_space", { space: input.wikiName }, opts));
      return { space: "created", index: "ok", detail: second.exists ? "space created" : "space created (verify failed)" };
    }
    return { space: "ok", index: "ok", detail: "space ok" };
  } catch (err) {
    return { space: "error", index: "error", detail: (err as Error).message };
  }
}
