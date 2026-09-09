---
description: Initialize a new wiki space on the server. Creates the full vault structure, config, and template files.
argument-hint: "<space>"
section: LLM Wiki
topLevelCli: true
---

# /wiki-init

Initialize a new wiki space using the `wiki_bootstrap` tool.

## User Arguments

$ARGUMENTS

## Steps

1. Take the space name from `$ARGUMENTS` (usually derived per project; the autopilot normally handles this — see the `research` skill).
2. Call `wiki_bootstrap(space=<space>)` to create the vault. Idempotent — existing spaces are untouched.
3. Pin it: `wiki_use_space(space=<space>)`.
4. Report the result and suggest next steps:
   - "Use `wiki_capture_source` to add your first source (URL or text)."
   - "Run `/wiki-ingest` after capturing sources to synthesize them into knowledge pages."

**Do NOT create directories or files.** The `wiki_bootstrap` tool handles all scaffolding including `raw/`, `wiki/`, `meta/`, `config.json`, and `templates/`.
