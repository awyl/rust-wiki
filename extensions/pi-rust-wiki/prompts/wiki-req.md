---
description: Capture and decompose a concept into atomic, traceable wiki requirements. Clarifies ambiguous requirements, splits them into atomic pieces, and persists them as wiki/requirements/ pages with status tracking.
argument-hint: "<concept description>"
section: LLM Wiki
topLevelCli: true
---

# /wiki-req

Capture a concept and decompose it into atomic, traceable requirements in the wiki.

Transforms natural language descriptions into structured `wiki/requirements/` pages, preserving the original clarified concept as an immutable source packet in `raw/sources/`.

## User Arguments

$ARGUMENTS

Read the LLM Wiki skill first for conventions. The session nudge names the active space; pass it as `space`.

## Steps

1. **Clarify the concept**
   - Discuss with the user: unpack ambiguous terms, surface implicit assumptions, identify scope boundaries
   - Ask targeted questions to resolve unknowns
   - Reach mutual clarity before proceeding

2. **Capture the clarified concept**
   - Call `wiki_capture_source(text=...)` with the clarified conversation as markdown
   - This creates an immutable record — the original intent verbatim, no interpretation

3. **Decompose into atomic requirements**
   - Break into the smallest independently verifiable behaviors
   - Scaffold each with `wiki_template(type="requirement")` — never from frozen copies
   - For each: `wiki_ensure_page(type="requirement", title="...", content="...")` with:
     - `status: draft` in frontmatter (`draft` → `clarified` → `active` → `implemented` → `deferred` → `rejected`)
     - `priority`: `p0` (blocking), `p1` (critical), `p2` (important), `p3` (nice-to-have)
     - `## Description` and `## Acceptance Criteria` checkbox list
     - `source_id` linking the capture, `depends_on` for prerequisites
     - `[label](/folder/page.md)` links to relevant entities, concepts, pages

4. **Cross-link and finalize**
   - Bidirectional links on every requirement page
   - Update referenced entity/concept pages (`wiki_read_page` first, keep the fence)
   - Report: count, priorities, source capture ID

**Rules:**
- One atomic requirement per call — each independently testable
- Always capture first, decompose second
- Inside Markdown table cells, never use `[[target|alias]]` pipes (unsupported)
