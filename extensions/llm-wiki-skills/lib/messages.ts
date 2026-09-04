export interface DirectiveMessage {
  customType: string;
  content: string;
  display: boolean;
}

export function buildBootstrapDirective(skillPath: string, wikiName?: string | null): DirectiveMessage {
  const targeting = wikiName
    ? `0. Ensure wiki space \`${wikiName}\` exists: call \`wiki_spaces_list\`; if it is absent, create it with \`wiki_spaces_create\` — use the parent directory of an existing space's path plus \`/${wikiName}\` as the path (if no space exists yet, ask the user for the parent directory once). Pass \`wiki: "${wikiName}"\` on every wiki tool call this session.`
    : "0. No project wiki space could be derived from git history — orient against the default space (see `wiki_info`).";
  return {
    customType: "llm-wiki-bootstrap",
    display: true,
    content: [
      "## Wiki orientation (bootstrap)",
      "",
      "Before responding to the user, orient to this project's wiki:",
      targeting,
      `1. Read the bootstrap skill at \`${skillPath}\``,
      "2. Follow it: call `wiki_info`, review the config and types, read hub pages.",
      "3. All page writes MUST use the `wiki_content_write` MCP tool — the agent filesystem and the wiki server filesystem are separate; local file writes are invisible to the server (verify with `wiki_content_read`).",
      "4. Report a one-line orientation summary, then continue with the user's request.",
    ].join("\n"),
  };
}

export function buildCrystallizeDirective(skillPath: string): DirectiveMessage {
  return {
    customType: "llm-wiki-crystallize",
    display: true,
    content: [
      "## Crystallize proposal",
      "",
      `This session is long enough to crystallize. Read the crystallize skill at \`${skillPath}\` and follow it:`,
      "- Extract decisions, findings, and open questions from this session.",
      "- **Propose** the pages you would write to the user and wait for confirmation before any write.",
    ].join("\n"),
  };
}

export const RESEARCH_NUDGE = [
  "",
  "## Wiki knowledge",
  "A wiki MCP server is connected to this session. When a question might be answered from wiki knowledge, use the `research` skill: read its SKILL.md, then `wiki_search` / `wiki_content_read` before answering from memory alone.",
].join("\n");
