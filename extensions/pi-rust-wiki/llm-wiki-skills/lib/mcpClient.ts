/**
 * Minimal streamable-HTTP MCP client for the wiki server's mechanical
 * bootstrap calls (wiki_info / wiki_spaces_create / wiki_index_rebuild).
 *
 * Boundary note: this is plumbing, not wiki logic. The extension makes no
 * decisions about content — it only ensures the space exists and the index
 * is healthy, exactly what the bootstrap directive used to rent a model for.
 */

export interface McpCallOptions {
  url: string;
  token?: string;
  timeoutMs?: number;
}

let sessionId: string | null = null;
let initialized = false;

function resetSession(): void {
  sessionId = null;
  initialized = false;
}

async function post(url: string, token: string | undefined, timeoutMs: number, body: unknown, sessionIdOverride?: string | null): Promise<{ status: number; messages: any[]; sessionHeader: string | null }> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      Accept: "application/json, text/event-stream",
    };
    if (token) headers.Authorization = `Bearer ${token}`;
    if (sessionIdOverride) headers["Mcp-Session-Id"] = sessionIdOverride;
    const res = await fetch(url, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
      signal: controller.signal,
    });
    const text = await res.text();
    return { status: res.status, messages: parseMessages(text), sessionHeader: res.headers.get("mcp-session-id") };
  } finally {
    clearTimeout(timer);
  }
}

/** MCP endpoints answer with plain JSON or SSE frames — accept both. */
export function parseMessages(text: string): any[] {
  const trimmed = text.trim();
  if (!trimmed) return [];
  if (!trimmed.startsWith("data:") && !trimmed.startsWith("event:")) {
    try {
      return [JSON.parse(trimmed)];
    } catch {
      return [];
    }
  }
  const messages: any[] = [];
  for (const line of trimmed.split("\n")) {
    const dataLine = line.trim();
    if (!dataLine.startsWith("data:")) continue;
    try {
      messages.push(JSON.parse(dataLine.slice(5).trim()));
    } catch {
      // keep-alive comment or partial frame
    }
  }
  return messages;
}

async function ensureInitialized(opts: McpCallOptions): Promise<void> {
  if (initialized) return;
  resetSession();
  const init = await post(opts.url, opts.token, opts.timeoutMs ?? 10_000, {
    jsonrpc: "2.0",
    id: 0,
    method: "initialize",
    params: {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "llm-wiki-autopilot", version: "0.4.0" },
    },
  });
  if (init.status >= 400) throw new Error(`MCP initialize failed: ${init.status}`);
  if (init.sessionHeader) sessionId = init.sessionHeader;
  await post(opts.url, opts.token, opts.timeoutMs ?? 10_000, { jsonrpc: "2.0", method: "notifications/initialized" }, sessionId);
  initialized = true;
}

function responseFor(messages: any[], id: number): any | null {
  return messages.find((m) => m?.id === id && (m.result || m.error)) ?? null;
}

/** Call an MCP tool by name; returns the first text content block. */
export async function callTool(tool: string, args: Record<string, unknown>, opts: McpCallOptions): Promise<string> {
  await ensureInitialized(opts);
  const id = Date.now();
  const res = await post(
    opts.url,
    opts.token,
    opts.timeoutMs ?? 15_000,
    { jsonrpc: "2.0", id, method: "tools/call", params: { name: tool, arguments: args } },
    sessionId,
  );
  let match = responseFor(res.messages, id);
  if (res.status === 404 || !match) {
    // stale session — re-initialize once and retry
    resetSession();
    await ensureInitialized(opts);
    const retryId = id + 1;
    const retry = await post(
      opts.url,
      opts.token,
      opts.timeoutMs ?? 15_000,
      { jsonrpc: "2.0", id: retryId, method: "tools/call", params: { name: tool, arguments: args } },
      sessionId,
    );
    match = responseFor(retry.messages, retryId);
    return extractText(retry.status, match);
  }
  return extractText(res.status, match);
}

function extractText(status: number, match: any | null): string {
  if (status >= 400) throw new Error(`MCP tools/call failed: ${status}`);
  if (!match) throw new Error("MCP tools/call: no JSON-RPC response in body");
  if (match.error) throw new Error(`MCP error: ${match.error.message ?? JSON.stringify(match.error)}`);
  const content = match.result?.content;
  if (Array.isArray(content)) {
    const text = content.find((c: any) => c.type === "text")?.text;
    if (typeof text === "string") return text;
  }
  return JSON.stringify(match.result ?? {});
}
