//! Per-turn recall injection (autoInject). Before each agent turn, search
//! the session's wiki space keyed on the user's prompt and deliver strong
//! hits as a hidden tail message. Cache safety: the system prompt is never
//! touched with per-turn content — volatile blocks ride a `display: false`
//! conversation message so the provider's prompt-cache prefix stays stable.
//!
//! Gated in config (`autoInject`, default false — feature off unless the
//! operator turns it on).

/** Minimum wiki_recall score for a hit to inject (title=3, id=2, type=1.5, body=1 per matching token). */
export const MIN_SCORE = 2.0;
/** Cap injected hits — recall context must stay small. */
export const MAX_HITS = 3;
/** Hidden message customType. */
export const RECALL_MESSAGE_TYPE = "rust-wiki-recall-context";
/** Preview length per hit, chars. */
const PREVIEW_CHARS = 160;

export interface RecallMatchDTO {
  id: string;
  title: string;
  type: string;
  score: number;
  preview?: string;
  /** "personal" when the hit came from the personal layer. */
  layer?: string;
}

/**
 * Format space-layer hits above MIN_SCORE into an injection block.
 * Personal-layer hits are dropped from AUTO injection (cross-project noise);
 * explicit wiki_recall still covers them. Returns undefined when nothing
 * clears the gate — no block, no message, no cache churn.
 */
export function buildRecallBlock(matches: RecallMatchDTO[]): string | undefined {
  const hits = matches
    .filter((m) => m.layer !== "personal" && m.score >= MIN_SCORE)
    .slice(0, MAX_HITS);
  if (hits.length === 0) return undefined;
  const lines = hits.map((m) => {
    const preview = (m.preview ?? "").split(/\s+/).filter(Boolean).join(" ").slice(0, PREVIEW_CHARS);
    const tail = preview ? ` — ${preview}` : "";
    return `- ${m.title || m.id} (${m.id})${tail}`;
  });
  return [
    "## Wiki recall (auto — relevant pages for this prompt)",
    "",
    ...lines,
    "",
    "Read any of these with wiki_read_page before answering if they look relevant.",
  ].join("\n");
}

export interface RecallMessage {
  customType: typeof RECALL_MESSAGE_TYPE;
  content: string;
  display: false;
}

/** Build the hidden injection message, or undefined when nothing to inject. */
export function buildRecallMessage(matches: RecallMatchDTO[]): RecallMessage | undefined {
  const block = buildRecallBlock(matches);
  if (!block) return undefined;
  return { customType: RECALL_MESSAGE_TYPE, content: block, display: false };
}

/**
 * Search the space via the direct MCP client. Any failure resolves to [] —
 * injection must never break or delay the turn it decorates.
 */
export async function recallForPrompt(
  url: string,
  token: string,
  space: string,
  query: string,
): Promise<RecallMatchDTO[]> {
  try {
    const { callTool } = await import("./mcpClient.js");
    const raw = await callTool("wiki_recall", { space, query, max_results: 5 }, { url, token });
    const parsed = JSON.parse(raw) as { matches?: RecallMatchDTO[] };
    return Array.isArray(parsed.matches) ? parsed.matches : [];
  } catch {
    return [];
  }
}
