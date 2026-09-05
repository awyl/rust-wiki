export interface DirectiveMessage {
  customType: string;
  content: string;
  display: boolean;
}

/**
 * Lean bootstrap (upstream "cheap suggestion" pattern): make the wiki usable
 * in 1-2 tool calls, then get out of the way. No deep orientation — the
 * research and crystallize skills orient themselves when actually invoked.
 */
export function buildBootstrapDirective(wikiName?: string | null): DirectiveMessage {
  const targeting = wikiName
    ? `0. Ensure wiki space \`${wikiName}\` exists: call \`wiki_spaces_list\`; if it is absent, create it with \`wiki_spaces_create\` — use the parent directory of an existing space's path plus \`/${wikiName}\` as the path (if no space exists yet, ask the user for the parent directory once). Pass \`wiki: "${wikiName}"\` on every wiki tool call this session.`
    : "0. No project wiki space could be derived from git history — skip setup; use the default space (see `wiki_info`) if you need wiki tools.";
  return {
    customType: "llm-wiki-bootstrap",
    display: true,
    content: [
      "## Wiki bootstrap (lean)",
      "",
      "Before responding to the user, make sure the project wiki is usable — then stop:",
      targeting,
      "1. Call `wiki_info`: if this wiki's index_status is degraded, recover it with `wiki_index_rebuild`.",
      "2. Do NOT orient further — research and crystallize skills orient themselves when invoked. Continue with the user's request.",
    ].join("\n"),
  };
}

/**
 * Background crystallize: the main agent only fires one detached headless
 * worker; the worker does all wiki work unattended (auto-write approved by
 * config) and writes a summary to the log file.
 */
export function buildCrystallizeDirective(skillPath: string, wikiName?: string | null): DirectiveMessage {
  const logPath = `/tmp/llm-wiki-crystallize-${wikiName ?? "default"}.log`;
  const wikiScope = wikiName
    ? `- Wiki space: pass \`wiki: "${wikiName}"\` on every wiki tool call.`
    : "- Wiki space: none derivable — use the default space (see `wiki_info`).";
  return {
    customType: "llm-wiki-crystallize",
    display: true,
    content: [
      "## Crystallize (background)",
      "",
      "This session is long enough to crystallize. Do NOT crystallize inline — delegate to a headless worker:",
      "",
      "1. Run ONE command as a background bash task:",
      "",
      "```",
      `LLM_WIKI_AUTOPILOT_DISABLE=1 pi -p "<WORKER_PROMPT>" > ${logPath} 2>&1 &`,
      "```",
      "",
      "2. Tell the user crystallize was delegated to the background worker (log at `" + logPath + "`). Do nothing else — no wiki calls in this session.",
      "",
      "WORKER_PROMPT (pass to pi -p, handle quoting):",
      "```",
      "Read the crystallize skill at `" + skillPath + "` and follow it with these overrides:",
      wikiScope,
      "- AUTO-WRITE: do not propose or wait for user confirmation — write pages directly, tagging each with a calibrated confidence value.",
      "- Full flow: map (wiki_list format llms), extraction plan, wiki_content_new + wiki_content_write per page, wiki_ingest (dry run, then real), wiki_lint (broken-link,orphan), verify via wiki_content_read.",
      "- Respect the accumulation contract when updating existing pages.",
      "- Finish with a summary printed to stdout: pages written (slugs + confidence), lint result, open questions.",
      "```",
    ].join("\n"),
  };
}

export const RESEARCH_NUDGE = [
  "",
  "## Wiki knowledge",
  "A wiki MCP server is connected to this session. When a question might be answered from wiki knowledge, use the `research` skill: read its SKILL.md, then `wiki_search` / `wiki_content_read` before answering from memory alone.",
].join("\n");
