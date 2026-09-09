# AGENTS.md

## Workflow

- Evidence first — hard evidence before conclusions; never guess repo state or others' work.
- Don't decide alone — important decisions come to the user (options + recommendation).
- Commits gated — explicit approval, one-time-only, per-job scope; blankets expire when the job's done.

## Code discipline

- Clean up, verify, simplify, optimize after coding — DRY, KISS, then clippy fix and `cargo fmt`; correctness + efficiency.
- DRY, KISS, YAGNI, TDD — tests first, no speculative machinery.
- Write highly efficient code — avoid clones, use zero-cost abstractions and references.
- Define clear traits for intermodule communication.
- Use latest stable crates.

## Structure

- Single responsibility per module.
- Small files, each with single responsibility.

*Source: portable master copy `concepts/project-agent-rules-agents-md` (personal wiki layer).*
