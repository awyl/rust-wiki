export interface DirectiveMessage {
  customType: string;
  content: string;
  display: boolean;
}

/**
 * Lean bootstrap: one `wiki_info` call — ensure the git-derived space
 * exists and the index is healthy, then get out of the way. Static
 * worker instructions live in files; directives stay tiny to keep the
 * dialog and the main session's context small.
 */
export function buildBootstrapDirective(wikiName?: string | null, display = true, wikiRoot = ""): DirectiveMessage {
  const createHint = wikiRoot
    ? `path = \`${wikiRoot.replace(/\/+$/, "")}/${wikiName}\``
    : `path = the parent directory of an existing space's path plus \`/${wikiName}\` (if no space exists yet, ask the user for the parent directory once)`;
  const targeting = wikiName
    ? `Call \`wiki_info\` once: if wiki space \`${wikiName}\` is absent from its spaces list, create it with \`wiki_spaces_create\` (${createHint}); if its index_status is degraded, recover it with \`wiki_index_rebuild\`. Pass \`wiki: "${wikiName}"\` on every wiki tool call this session.`
    : "No project wiki space could be derived from git history — skip setup; use the default space (see `wiki_info`) if you need wiki tools.";
  return {
    customType: "llm-wiki-bootstrap",
    display,
    content: [
      "## Wiki bootstrap (lean)",
      targeting,
      "Do NOT orient further — research and crystallize skills orient themselves when invoked. Continue with the user's request.",
    ].join("\n"),
  };
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

export const RESEARCH_NUDGE = [
  "",
  "## Wiki knowledge",
  "A wiki MCP server is connected to this session. When a question might be answered from wiki knowledge, use the `research` skill: read its SKILL.md, then `wiki_search` / `wiki_content_read` before answering from memory alone.",
].join("\n");
