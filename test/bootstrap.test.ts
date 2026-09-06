import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureWikiReady } from "../extensions/llm-wiki-skills/lib/bootstrap.js";

const URL_ = "http://mcp.test/mcp";

interface StubResponse {
  status?: number;
  body: any;
}

function stubFetch(responses: StubResponse[]) {
  let i = 0;
  const fetchMock = vi.fn(async (_url: any, init: any) => {
    const body = JSON.parse(init.body);
    let result: any;
    if (body.method === "initialize") {
      result = { protocolVersion: "2025-06-18", capabilities: {} };
    } else if (body.method === "tools/call") {
      const user = responses[Math.min(i, responses.length - 1)];
      i += 1;
      result = user.body.result;
    } else {
      result = {};
    }
    return {
      status: 200,
      text: async () => JSON.stringify({ jsonrpc: "2.0", id: body.id, result }),
      headers: new Headers(),
    } as Response;
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});

const INFO_OK = { spaces: ["rust-wiki-cc79119"], index_status: { status: "ok" } };

describe("ensureWikiReady", () => {
  it("does nothing when the space exists and the index is healthy", async () => {
    const fetchMock = stubFetch([{ body: { result: { content: [{ type: "text", text: JSON.stringify(INFO_OK) }] } } }]);
    const result = await ensureWikiReady({ url: URL_, wikiName: "rust-wiki-cc79119" });
    expect(result).toEqual({ space: "ok", index: "ok", detail: "space ok; index ok" });
    const bodies = fetchMock.mock.calls.map((c: any) => JSON.parse(c[1].body));
    expect(bodies.some((b: any) => b.method === "tools/call" && b.params.name === "wiki_info")).toBe(true);
    expect(bodies.some((b: any) => b.method === "tools/call" && b.params.name === "wiki_spaces_create")).toBe(false);
  });

  it("creates the space under wikiRoot when missing", async () => {
    stubFetch([{ body: { result: { content: [{ type: "text", text: JSON.stringify({ spaces: ["main"], index_status: { status: "ok" } }) }] } } }]);
    const result = await ensureWikiReady({ url: URL_, wikiName: "new-proj-abc1234", wikiRoot: "/data" });
    expect(result.space).toBe("created");
  });

  it("reports needs-parent-dir when the space is missing and no wikiRoot is set", async () => {
    stubFetch([{ body: { result: { content: [{ type: "text", text: JSON.stringify({ spaces: ["main"], index_status: { status: "ok" } }) }] } } }]);
    const result = await ensureWikiReady({ url: URL_, wikiName: "new-proj-abc1234" });
    expect(result).toEqual({ space: "needs-parent-dir", index: "n/a", detail: "space missing and no wikiRoot configured" });
  });

  it("rebuilds when the index is degraded", async () => {
    const fetchMock = stubFetch([{ body: { result: { content: [{ type: "text", text: JSON.stringify({ spaces: ["rust-wiki-cc79119"], index_status: { status: "degraded" } }) }] } } }]);
    const result = await ensureWikiReady({ url: URL_, wikiName: "rust-wiki-cc79119" });
    expect(result.index).toBe("rebuilt");
    const bodies = fetchMock.mock.calls.map((c: any) => JSON.parse(c[1].body));
    expect(bodies.some((b: any) => b.method === "tools/call" && b.params.name === "wiki_index_rebuild")).toBe(true);
  });

  it("never throws — failures come back as an error result", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new Error("connection refused"); }));
    const result = await ensureWikiReady({ url: URL_, wikiName: "x" });
    expect(result.space).toBe("error");
    expect(result.detail).toContain("connection refused");
  });

  it("parses SSE-framed responses (data: lines), not just plain JSON", async () => {
    const sse = (msg: any) => "event: message\ndata: " + JSON.stringify(msg) + "\n\n";
    const fetchMock = vi.fn(async (_url: any, init: any) => {
      const body = JSON.parse(init.body);
      let msg: any;
      if (body.method === "initialize") {
        msg = { jsonrpc: "2.0", id: body.id, result: { protocolVersion: "2025-06-18", capabilities: {} } };
      } else {
        msg = { jsonrpc: "2.0", id: body.id, result: { content: [{ type: "text", text: JSON.stringify(INFO_OK) }] } };
      }
      return {
        status: 200,
        text: async () => sse(msg),
        headers: new Headers(),
      } as Response;
    });
    vi.stubGlobal("fetch", fetchMock);
    const result = await ensureWikiReady({ url: URL_, wikiName: "rust-wiki-cc79119" });
    expect(result).toEqual({ space: "ok", index: "ok", detail: "space ok; index ok" });
  });
});
