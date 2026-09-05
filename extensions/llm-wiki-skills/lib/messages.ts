export interface DirectiveMessage {
  customType: string;
  content: string;
  display: boolean;
}

/**
 * Research nudge footer for the system prompt. Session-static by design:
 * the wiki name is derived once at session_start, so the footer is
 * byte-identical every turn within a session — provider prompt cache
 * stays stable. See prompt-cache-safety in the wiki.
 */
export function buildResearchNudge(wikiName?: string | null): string {
  const lines = [
    "",
    "## Wiki knowledge",
    "A wiki MCP server is connected to this session. When a question might be answered from wiki knowledge, use the `research` skill: read its SKILL.md, then `wiki_search` / `wiki_content_read` before answering from memory alone.",
  ];
  if (wikiName) {
    lines.push(
      `Project wiki space: \`${wikiName}\` — pass \`wiki: "${wikiName}"\` on every wiki tool call. A background worker already ensured the space exists and the index is healthy.`,
    );
  }
  return lines.join("\n");
}

/**
 * Background crystallize: the main agent writes its session extraction
 * to a temp file, then fires one detached headless worker. Static
 * worker instructions live in `worker-crystallize.md` inside the
 * package — the directive only carries paths, keeping the dialog and
 * the main session's context small.
 */
export function buildCrystallizeDirective(
  skillPath: string,
  workerPromptPath: string,
  wikiName?: string | null,
  display = true,
): DirectiveMessage {
  const name = wikiName ?? "default";
  const extractionPath = `/tmp/llm-wiki-extraction-${name}.md`;
  const logPath = `/tmp/llm-wiki-crystallize-${name}.log`;
  const wikiScope = wikiName ? `Wiki: ${wikiName}.` : "No wiki space derivable — use the default space (see wiki_info).";
  return {
    customType: "llm-wiki-crystallize",
    display,
    content: [
      "## Crystallize (background)",
      `1. Write a compact extraction of this session's durable knowledge (2-6 items: what, type, confidence, target slug — the worker cannot see this session) to \`${extractionPath}\`.`,
      `2. Run ONE command as a background bash task: \`LLM_WIKI_AUTOPILOT_DISABLE=1 pi -p "Read ${workerPromptPath} and follow it. Extraction file: ${extractionPath}. Crystallize skill: ${skillPath}. ${wikiScope}" > ${logPath} 2>&1 &\``,
      `3. Tell the user crystallize was delegated (log at \`${logPath}\`). Do nothing else — no wiki calls in this session.`,
    ].join("\n"),
  };
}
