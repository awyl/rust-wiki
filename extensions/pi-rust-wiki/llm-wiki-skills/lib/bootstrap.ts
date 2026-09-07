import { callTool, type McpCallOptions } from "./mcpClient.js";

export interface BootstrapResult {
  space: "ok" | "created" | "needs-parent-dir" | "error";
  index: "ok" | "rebuilt" | "n/a" | "error";
  detail: string;
}

export interface BootstrapInput extends McpCallOptions {
  wikiName: string;
  wikiRoot?: string;
}

function parseInfo(raw: string): { spaces: string[]; degraded: boolean } {
  try {
    const parsed = JSON.parse(raw);
    const spaces: string[] = Array.isArray(parsed?.spaces) ? parsed.spaces : [];
    const degraded = parsed?.index_status?.status === "degraded";
    return { spaces, degraded };
  } catch {
    return { spaces: [], degraded: raw.includes("degraded") };
  }
}

/**
 * Mechanical bootstrap: ensure the space exists and the index is healthy.
 * Makes no content decisions — plumbing only. Never throws.
 */
export async function ensureWikiReady(input: BootstrapInput): Promise<BootstrapResult> {
  const opts: McpCallOptions = { url: input.url, token: input.token, timeoutMs: input.timeoutMs };
  try {
    const info = await callTool("wiki_info", {}, opts);
    const { spaces, degraded } = parseInfo(info);

    let created = false;
    if (!spaces.includes(input.wikiName)) {
      if (!input.wikiRoot) {
        return { space: "needs-parent-dir", index: "n/a", detail: "space missing and no wikiRoot configured" };
      }
      const root = input.wikiRoot.replace(/\/+$/, "");
      await callTool("wiki_spaces_create", { name: input.wikiName, path: `${root}/${input.wikiName}` }, opts);
      created = true;
    }

    if (degraded) {
      await callTool("wiki_index_rebuild", { wiki: input.wikiName }, opts);
      return { space: created ? "created" : "ok", index: "rebuilt", detail: "index rebuilt" };
    }
    return { space: created ? "created" : "ok", index: "ok", detail: "space ok; index ok" };
  } catch (err) {
    return { space: "error", index: "error", detail: (err as Error).message };
  }
}
