---
description: Capture the just-completed task's tool-call trajectory into the wiki as agent working-memory, then optionally distill it into a reusable skill.
argument-hint: "<title> [--outcome success|failure|partial]"
section: LLM Wiki
topLevelCli: true
---

# /wiki-record

Capture the trajectory of the task you just completed — the sequence of tool calls that solved it — into the wiki's working-memory layer. Records what you *did* so the wiki compounds over your own work.

## User Arguments

$ARGUMENTS

Read the LLM Wiki skill first for conventions. The session nudge names the active space; pass it as `space`.

## Steps

1. Call `wiki_capture_trajectory` with:
   - `title`: short descriptive phrase for the task (≤60 chars, noun phrase)
   - `outcome`: optional — `success` (default), `failure`, or `partial`
   - `steps`: the meaningful tool-call record (names + key arguments + results + errors — not every call, just the ones that matter)
   - `summary`: self-contained prose recap (what was requested, key decisions, outcome)
2. Open the generated skeleton case page (`cases/...`) and flesh it out via `wiki_write_page` (fence mandatory):
   - **Task** — what was requested
   - **Approach** — key steps and decisions
   - **Outcome** — result, reuse/avoid notes
3. If the task taught a reusable pattern, run `wiki_distill_skills` and create a `skill` page via `wiki_ensure_page(type="skill", title="...")` citing the trajectory id, then `wiki_distill_skills(mark_distilled=[<id>])`.
4. Future sessions surface the skill/case via `wiki_recall_skill`.

**Rules:**
- Only record tasks worth learning from — non-trivial debugging, refactors, integrations. Skip trivial one-shots.
- The packet under `raw/trajectories/` is immutable. Edit case/skill pages, never the packet.
- One trajectory per `wiki_capture_trajectory` call.
