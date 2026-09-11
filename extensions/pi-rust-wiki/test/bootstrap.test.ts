import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureWikiReady } from "../llm-wiki-skills/lib/bootstrap.js";

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

function useSpaceOut(exists: boolean) {
  return { content: [{ type: "text", text: JSON.stringify({ space: "s", exists, total_pages: exists ? 3 : null }) }] };
}
function bootstrapOut() {
  return { content: [{ type: "text", text: JSON.stringify({ created: true, space: "s", root: "/v/s" }) }] };
}
function calledTools(fetchMock: any): string[] {
  return fetchMock.mock.calls
    .map((c: any) => JSON.parse(c[1].body))
    .filter((b: any) => b.method === "tools/call")
    .map((b: any) => b.params.name);
}

describe("ensureWikiReady (rust-wiki)", () => {
  it("tops up templates when the space exists", async () => {
    const fetchMock = stubFetch([{ body: { result: useSpaceOut(true) } }, { body: { result: bootstrapOut() } }, { body: { result: useSpaceOut(true) } }]);
    const result = await ensureWikiReady({ url: URL_, wikiName: "proj-x" });
    expect(result).toEqual({ space: "ok", index: "ok", detail: "space ok" });
    expect(calledTools(fetchMock)).toEqual(["wiki_use_space", "wiki_bootstrap", "wiki_use_space"]);
  });

  it("bootstraps when the space is missing and verifies it", async () => {
    const fetchMock = stubFetch([{ body: { result: useSpaceOut(false) } }, { body: { result: bootstrapOut() } }, { body: { result: useSpaceOut(true) } }]);
    const result = await ensureWikiReady({ url: URL_, wikiName: "proj-x" });
    expect(result).toEqual({ space: "created", index: "ok", detail: "space created" });
    expect(calledTools(fetchMock)).toEqual(["wiki_use_space", "wiki_bootstrap", "wiki_use_space"]);
  });

  it("reports create-verify failure honestly", async () => {
    stubFetch([{ body: { result: useSpaceOut(false) } }, { body: { result: bootstrapOut() } }, { body: { result: useSpaceOut(false) } }]);
    const result = await ensureWikiReady({ url: URL_, wikiName: "proj-x" });
    expect(result.detail).toContain("verify failed");
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
        msg = { jsonrpc: "2.0", id: body.id, result: useSpaceOut(true) };
      }
      return {
        status: 200,
        text: async () => sse(msg),
        headers: new Headers(),
      } as Response;
    });
    vi.stubGlobal("fetch", fetchMock);
    const result = await ensureWikiReady({ url: URL_, wikiName: "proj-x" });
    expect(result).toEqual({ space: "ok", index: "ok", detail: "space ok" });
  });
});
