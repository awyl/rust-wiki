# rust-wiki — remote zosmaai-style wiki MCP server (design spec)

**Date:** 2026-09-06 · **Status:** approved direction, pre-implementation
**Replaces:** geronimo-iia/llm-wiki + its 17 vendored skills (full cutover, no coexistence)

## Decision context (2026-09-06)

Ditch geronimo-iia/llm-wiki and its vendored skills entirely. Adapt
zosmaai/pi-llm-wiki's engine model as closely as possible, re-hosted as a
**remote Rust MCP server** so every agent on every container shares one
centralized knowledge service, with **spaces** for multi-project isolation
(the one feature zosmaai lacks).

Evidence driving the decision:

- geronimo pain: ingestion and page-writing are an LLM minefield — slug vs
  filesystem paths, strict enums, type-mismatch frontmatter edges, opaque
  degraded-stamp status. Recurring errors every session.
- zosmaai pain: none observed in hands-on use; works fine.
- zosmaai gaps for us: embedded/local only (no remote MCP), no spaces.

Decisions locked by the owner (2026-09-06):

| Question | Decision |
|---|---|
| Name | **rust-wiki** |
| Vault root | Default: local to the executable; override via env (`WIKI_VAULT_ROOT`) |
| Migration | **None** — fresh vaults, nothing carried from geronimo |
| geronimo coexistence | **None** — hard cutover |
| Auth | **None** (trusted network) |

## Key discovery that shapes everything

zosmaai's `wiki_ingest` **does not synthesize**. It returns a batch of
extracted source packets; *the calling agent* reads them, writes source
summaries, and creates entity/concept pages via `wiki_ensure_page` and
standard markdown. The engine is 100% mechanical: vault, registry,
backlinks, chunk-scored recall, lint, projections. No server-side LLM, ever.

Consequences:

- A Rust server needs **no model infrastructure** — the agents are the models.
- The LLM-friendliness comes from cooperative tool design, forgiving
  validation, and mechanical guardrails — all portable.

## Architecture

One Rust binary (`rust-wiki`), streamable-HTTP MCP (rmcp crate), no auth,
serving N spaces:

```
VAULT_ROOT/                  # default: <exe_dir>/vaults, override: WIKI_VAULT_ROOT
├── <space-name>/            # one space = one flattened zosmaai-style vault (below)
├── personal/                # reserved cross-project layer (auto-created at boot)
└── config.toml              # server config (minimal)
```

Vault layout per space (flattened — no .llm-wiki nesting; change approved 2026-09-07):

```
<space-name>/
├── config.json              # vault config
├── templates/               # page templates
├── raw/sources/SRC-*/       # immutable source packets (extension-owned)
├── wiki/                    # editable knowledge pages
│   ├── sources/             # one summary per source
│   ├── entities/            # people, orgs, tools, products
│   ├── concepts/            # ideas, patterns, frameworks
│   ├── syntheses/           # cross-cutting analyses
│   └── analyses/            # durable query answers
├── meta/                    # events + generated projections
│   ├── registry.json        # master page catalog
│   ├── backlinks.json       # inbound link map
│   ├── index.md             # human-readable catalog
│   ├── log.md               # activity log
│   └── events.jsonl         # append-only authoritative event stream
├── outputs/                 # generated artifacts (lint reports)
└── .discoveries/            # discovery/gap tracking
```

Ownership guardrails ported verbatim: `raw/**` immutable, `meta/**`
server-owned, `events.jsonl` append-only authoritative, `wiki/**`
agent+user editable. Writes outside `wiki/**` are rejected.

### Space resolution (replaces zosmaai's cwd-based layers)

1. Every tool accepts an optional `space` argument.
2. `wiki_use_space(space)` pins a per-connection default — agents call it
   once at session start; later calls omit `space`.
3. Reserved space `personal` = cross-project layer: `wiki_recall` merges
   active-space hits (priority) + `personal` hits (labeled), dedup by page
   ID — direct port of zosmaai's layered recall, server-side.

## Tool surface (v1 — 15 tools)

Adapted from zosmaai's 14; semantics preserved:

| Tool | Notes for remote adaptation |
|---|---|
| `wiki_use_space` | **new** — pin per-connection space; returns space summary |
| `wiki_bootstrap` | + required `space` arg; creates the vault dir |
| `wiki_capture_source` | url/text as-is; `file_path` resolves server-side only (agents pass `text` or `url`) |
| `wiki_ingest` | cooperative batch: returns uningested packets, agent synthesizes (default batch 3, max 5) |
| `wiki_ensure_page` | unchanged (create-no-overwrite, template fallback) |
| `wiki_read_page` | **new** — read a `wiki/**` page by ID (remote replaces local file reads) |
| `wiki_write_page` | **new** — guarded update of an existing `wiki/**` page; metadata rebuild automatic |
| `wiki_recall` | layered: active space + `personal`; chunk scoring, weighted fields, PRF, links-first gate (default threshold 50 pages) |
| `wiki_search` | registry keyword search |
| `wiki_retro` | atomic insight file + immediate metadata rebuild (wikilink gate ported) |
| `wiki_observe` | timestamped relevance-rated observation |
| `wiki_lint` | orphans / missing / contradictions / gaps; `auto_fix` stubs; report returned in-call |
| `wiki_status` | counts by type, orphans, gaps, health verdict — from registry |
| `wiki_rebuild_meta` | full projection rebuild (synchronous) |
| `wiki_log_event` | append to `events.jsonl` |

**Deferred (v2+):** embeddings, trajectory trio (working memory),
`wiki_watch`, OKF v0.2 projections, Obsidian integration.

## Engine behaviors to port

- Recall: chunk-level scoring, weighted field matching, pseudo-relevance
  feedback, links-first gate above page-count threshold (default 50),
  vault-source labels.
- Registry: `meta/registry.json` master catalog; backlinks; index/log are
  projections rebuilt from `wiki/**` + events; ingest state tracked
  per-source in the registry.
- Lint: orphan detection, missing-page detection, contradiction markers
  (⚠️ Contradiction), gap tracking, auto-stub when a gap is cited in ≥2 pages.
- Capture: URL fetch + HTML→md, text passthrough, server-local file path;
  PDF via MarkItDown when available (clean error when absent).
- Wikilink gate: `[[folder/page]]` legacy readable; canonical links are
  standard markdown `[label](/folder/page.md)`; validation modes
  off | validate | normalize.
- Templates: page templates per type ship in the vault at bootstrap.

## Skill adaptation (replaces geronimo's 17 vendored skills)

zosmaai ships **one** 17 KB `SKILL.md` + page templates = the entire agent
brain. Port plan: vendor it, adapt two things — (1) remote tool notes (no
local paths; use `wiki_read_page`/`wiki_write_page`), (2) drop host-specific
sections (`/wiki-model`, `/wiki-settings`, `/wiki-dashboard` — pi-host
features). The autopilot's vendored `skills/` set becomes `llm-wiki/` +
templates.

## Autopilot retarget (pi-llm-wiki-autopilot)

- Bootstrap (`ensureWikiReady`): retarget to `wiki_use_space` +
  `wiki_status`; `wikiMcpUrl` points at rust-wiki. The extension's
  JSON-RPC client survives — URL + tool names change only.
- Research nudge: swap tool names (`wiki_recall`, `wiki_search`,
  `wiki_read_page`).
- Crystallize worker: same flow, new tool names; extraction lands via
  `wiki_retro` / `wiki_ensure_page`.
- `lib/wikiName.ts` unchanged (git-derived space name feeds
  `wiki_use_space` + `wiki_bootstrap`).

## Server config + ops

`config.toml` next to the binary: `port` (default chosen at impl),
`vault_root` (default `<exe_dir>/vaults`), env override `WIKI_VAULT_ROOT`.
No auth (trusted network binding). Logging: tracing to stdout. Deploy:
same container host as the geronimo engine it replaces; agents' MCP
configs repoint at it; geronimo containers decommissioned at cutover.

## Testing

Port zosmaai's test philosophy: vault-format roundtrips,
registry/projection rebuilds, recall scoring fixtures, lint cases,
wikilink gate, MCP parity across connections, space isolation (no
cross-space leakage). Mechanical core target ~85% coverage like upstream.

## Implementation outline

1. **M1 — vault core:** vault layout, bootstrap, config, templates,
   registry + projections, guardrails. Crate-internal tests.
2. **M2 — read path:** recall (scoring + PRF + layering), search, status,
   read_page, lint (+ auto_fix).
3. **M3 — write path:** capture_source (url/text), ingest batch,
   ensure_page, write_page, retro, observe, log_event, wikilink gate.
4. **M4 — MCP server:** rmcp streamable-HTTP, per-connection space,
   config/env, e2e tool tests.
5. **M5 — cutover:** autopilot retarget, skill vendoring + adaptation,
   repoint agents, decommission geronimo.

## OKF v0.2 support (2026-09-07, pulled forward from deferred)

Adopted subset of zosmaai's OKF Foundation spec:

- `knowledge_format` field in vault config.json: new bootstraps persist
  `okf-0.2`; absent/`legacy` = legacy mode; unknown values fail closed
  (`config_invalid_knowledge_format`).
- Deterministic `wiki/index.md` + per-directory `index.md` projections
  in OKF mode (root frontmatter `okf_version: "0.2"`, Directories before
  Concepts, path-sorted, ` — description` only when non-empty, stale
  indexes pruned).
- Deterministic `wiki/log.md` from events.jsonl (grouped by UTC date,
  newest first, canonical sorted-key JSON details, malformed events
  omitted). Event field renamed `ts` -> `timestamp` for OKF alignment.
- Guardrails: `wiki/index.md`, `wiki/**/index.md`, `wiki/log.md` are
  generated projections — direct writes rejected in OKF mode.
- Registry entries carry OKF `description` (frontmatter, optional).

Deviations from zosmaai Foundation (documented, deliberate):

- Lenient frontmatter parsing (house LLM-friendly style) instead of
  fail-closed YAML diagnostics.
- Regex link extraction instead of a CommonMark AST parser.
- ASCII slugs instead of NFC-normalized identity.

## wiki_watch (built-in scheduler, 2026-09-07)

The server owns the clock — no crontab, no human steps, no daemon
process. At startup it arms a background thread running a mechanical
maintenance cycle (lint + auto_fix + status) across every bootstrapped
space. Default interval: hourly; `WIKI_CRON_INTERVAL_SECS` tunes it,
`0` disables. Chose a plain thread over the `cron_tab` crate: fixed
interval covers the need, cron expressions are machinery we don't
need (KISS/YAGNI).

- `wiki_watch` (MCP, no args) -> scheduler status `{enabled, interval_secs}`.
- `wiki_watch {run: true}` -> immediate all-spaces cycle, returns reports.
- `rust-wiki cron --space <name>` -> manual single-space cycle (kept).

## Obsidian compatibility (2026-09-07)

The OKF projections ARE the Obsidian story: `wiki/index.md` gives a
clickable entry page, standard markdown links open natively, plain
folders open as a vault. Space bootstrap additionally writes a README.md
at the space root documenting the layout. No .obsidian/ generation —
Obsidian creates its own config on first open.

## Embeddings (2026-09-07, pulled forward from deferred)

Optional semantic layer over the lexical engine:

- Provider: OpenAI-compatible `POST {WIKI_EMBEDDING_URL}/embeddings`
  with `WIKI_EMBEDDING_MODEL` (+ optional `WIKI_EMBEDDING_TOKEN`).
  Configured via env only; absent = feature off with a clean no-op from
  `wiki_reindex_embeddings`.
- Store: `meta/embeddings.json` — `{model, pages: {id: [f32]}}`, one
  vector per page over title+id+excerpt.
- Blend: `wiki_recall` embeds the query (one call) when provider AND
  store exist; score *= 1 + max(0, cosine) * 0.5, re-sorted. No store or
  no provider -> pure lexical, silently.
- Chunk-level vectors and trust-weighted scoring remain future work.

## Non-goals (v1)

Auth, trajectories/working-memory (deferred again 2026-09-07),
multi-user/quotas, data migration.
