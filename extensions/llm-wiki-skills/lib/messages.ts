export interface DirectiveMessage {
  customType: string;
  content: string;
  display: boolean;
}

export function buildBootstrapDirective(skillPath: string): DirectiveMessage {
  return {
    customType: "llm-wiki-bootstrap",
    display: true,
    content: [
      "## Wiki orientation (bootstrap)",
      "",
      "Before responding to the user, orient to this project's wiki:",
      `1. Read the bootstrap skill at \`${skillPath}\``,
      "2. Follow it: call `wiki_info`, review the config and types, read hub pages.",
      "3. Report a one-line orientation summary, then continue with the user's request.",
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
