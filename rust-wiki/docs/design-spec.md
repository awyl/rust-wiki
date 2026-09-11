# rust-wiki — remote zosmaai-style wiki MCP server (design spec)

**Date:** 2026-09-06 · **Status:** shipped — server v0.8.0 (2026-09-11).
Sections below are dated as each landed; the v1 design text is kept for
context, superseded where a dated section says otherwise.
**Replaces:** zosmaai/pi-llm-wiki + its 17 vendored skills (full cutover, no coexistence)

## Decision context (2026-09-06)

Ditch zosmaai/pi-llm-wiki and its vendored skills entirely. Adapt
zosmaai/pi-llm-wiki's engine model as closely as possible, re-hosted as a
**remote Rust MCP server** so every agent on every container shares one
centralized knowledge service, with **spaces** for multi-project isolation
(the one feature zosmaai lacks).

Evidence driving the decision:

- legacy pain: ingestion and page-writing are an LLM minefield — slug vs
  filesystem paths, strict enums, type-mismatch frontmatter edges, opaque
  degraded-stamp status. Recurring errors every session.
- zosmaai pain: none observed in hands-on use; works fine.
- zosmaai gaps for us: embedded/local only (no remote MCP), no spaces.

Decisions locked by the owner (2026-09-06):

| Question | Decision |
|---|---|
| Name | **rust-wiki** |
| Vault root | Default: local to the executable; override via env (`WIKI_VAULT_ROOT`) |
| Migration | **None** — fresh vaults, nothing carried from the legacy engine |
| legacy coexistence | **None** — hard cutover |
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
└── personal/                # reserved cross-project layer (auto-created at boot)

<exe_dir>/config.toml        # server config, next to the binary (see Server config)
```

Vault layout per space (flattened — no .llm-wiki nesting; change approved 2026-09-07):

```
<space-name>/
├── config.json              # vault config
├── templates/               # page templates
├── raw/sources/SRC-*/       # immutable source packets (agent-captured)
├── raw/trajectories/TRJ-*/  # immutable trajectory packets
├── wiki/                    # editable knowledge pages
│   ├── sources/             # one summary per source
│   ├── entities/            # people, orgs, tools, products
│   ├── concepts/            # ideas, patterns, frameworks
│   ├── syntheses/           # cross-cutting analyses
│   ├── analyses/            # durable query answers
│   ├── requirements/        # requirement lifecycle pages
│   ├── skills/              # distilled skills
│   └── cases/               # trajectory case pages
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

## Tool surface (24 tools)

The original v1 surface (15) adapted from zosmaai's 14; semantics preserved:

| Tool | Notes for remote adaptation |
|---|---|
| `wiki_use_space` | **new** — pin per-connection space; returns space summary |
| `wiki_bootstrap` | + required `space` arg; creates the vault dir |
| `wiki_capture_source` | url/text/file: bytes → markdown via one `Converter` (html/text/pdf), 10 MB cap, 30 s fetch timeout, magic bytes override a lying content-type. `file_path` resolves server-side only |
| `wiki_ingest` | cooperative batch: returns uningested packets, agent synthesizes (default batch 3, max 5) |
| `wiki_ensure_page` | unchanged (create-no-overwrite, template fallback) |
| `wiki_read_page` | **new** — read a `wiki/**` page by ID (remote replaces local file reads) |
| `wiki_write_page` | **new** — guarded update of an existing `wiki/**` page; metadata rebuild automatic |
| `wiki_delete_page` | **new** — only irreversible op, so three guards: `allow_delete` config (default false), `confirm` must repeat `id`, and refusal while any page still links to it (`force` overrides). Also drops the page's chunk vectors, or semantic recall would keep admitting a deleted id |
| `wiki_recall` | layered: active space + `personal`; chunk scoring, weighted fields, PRF, links-first gate (default threshold 50 pages) |
| `wiki_search` | registry keyword search |
| `wiki_retro` | atomic insight file + immediate metadata rebuild (wikilink gate ported) |
| `wiki_observe` | timestamped relevance-rated observation |
| `wiki_lint` | orphans / missing / contradictions / gaps; `auto_fix` stubs; report returned in-call |
| `wiki_status` | counts by type, orphans, gaps, health verdict (from registry), `server_version`, and `allow_delete` (deletion opt-in) |
| `wiki_rebuild_meta` | full projection rebuild (synchronous) |
| `wiki_log_event` | append to `events.jsonl` |

Shipped after v1 — all live as of v0.6.2, described in the dated sections
below: `wiki_template`, `wiki_ensure_personal_page`,
`wiki_write_personal_page`, `wiki_watch`, `wiki_reindex_embeddings`,
`wiki_capture_trajectory`, `wiki_distill_skills`, `wiki_recall_skill`.

**Still unbuilt:** host screens (`/wiki-model`, `/wiki-settings`,
`/wiki-dashboard` — dropped in the port), OKF Interchange (bundle
import/export, trust scoring), embeddings staleness skipping.

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

## Skill adaptation (replaces the 17 vendored upstream skills)

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

One labeled key per knob, single precedence per key: **environment →
`<exe_dir>/config.toml` → default** (2026-09-10). Every key is settable in
the file and overridable per key by env; `config.toml.example` labels each.

| Key | Env | Default |
|---|---|---|
| `port` | `WIKI_PORT` | `8484` |
| `vault_root` | `WIKI_VAULT_ROOT` | `<exe_dir>/vaults` |
| `cron_interval_secs` | `WIKI_CRON_INTERVAL_SECS` | `3600` (0 = off) |
| `embedding_url` | `WIKI_EMBEDDING_URL` | unset (feature off) |
| `embedding_model` | `WIKI_EMBEDDING_MODEL` | `text-embedding-3-small` |
| `embedding_token` | `WIKI_EMBEDDING_TOKEN` | unset |
| `recall_links_first_threshold` | `WIKI_RECALL_LINKS_FIRST_THRESHOLD` | `50` |
| `git_interval_secs` | `WIKI_GIT_INTERVAL_SECS` | `60` (0 = off) |
| `git_idle_secs` | `WIKI_GIT_IDLE_SECS` | `300` |
| `allow_delete` | `WIKI_ALLOW_DELETE` | `false` (deletion refused; accepts `1/true/yes/on`) |

No auth (trusted network binding). Diagnostics go to **stderr** — in the
default stdio transport stdout carries protocol only. The binary logs
`rust-wiki v<version>` at boot and `wiki_status` reports `server_version`,
so a deploy is verifiable from the log or a tool call (2026-09-10) without
trusting the client; `allow_delete` is reported the same way. Deploy:
same container host as the legacy engine it replaces; agents' MCP configs
repoint at it. Note: replacing the binary is not enough — the running MCP
server process must restart to pick it up.

### Git backing (auto-commit) — fixed 2026-09-10

The vault root is one repo, shell `git`, driven by a thread every
`git_interval_secs`. A commit needs the vault idle for `git_idle_secs` **and**
dirty. That gate was unreachable: `tick` rewrites `meta/git.json` every cycle,
and `idle_secs` counted that bookkeeping file as content activity, so idle
reset to the tick interval (60s) below the idle window (300s) forever. With
`git_interval_secs` 0 the thread never starts; `wiki_status.git` reported `ok`
throughout either way, because it tracks failures (init/pull/push), not whether
a commit ever happened.

- `idle_secs` now skips `.git` **and** `meta/git.json`. Two tests cover it
  (both fail if the exclusion is removed): an hour-idle dirty vault commits,
  and the tick's own state write does not reset the idle clock.
- `meta/git.json` joined `meta/embeddings.json` in `.gitignore`, and
  `ensure_repo` now tops up missing ignore lines in an **existing** repo (it
  used to return early). Without this a tracked `git.json` would be dirty
  every cycle and produce an auto-commit per idle window containing nothing but
  a timestamp — `bookkeeping_churn_alone_never_commits` pins that.
- If a vault already has `meta/git.json` tracked, `git rm --cached
  meta/git.json` once; the ignore line alone will not untrack it.

### Source capture: one converter for every door (2026-09-11)

Bytes become markdown in exactly one place (`vault::convert`), reached by both
capture paths — URL fetches and server-local files — through the `Converter`
trait. What a document *is* comes from the HTTP `content-type` or the file
extension, and magic bytes (`%PDF-`) override both, because a PDF served as
`text/plain` used to be stored as lossy-UTF-8 mojibake with no error at all.

| Limit | Value | Why |
|---|---|---|
| Capture size | 10 MB | a declared `content-length` is rejected before the body is read; the read itself is capped too |
| Fetch timeout | 30 s | `reqwest::blocking::get` had none, so a hung host hung the capture |
| Supported | html, text/markdown, json, xml, yaml, pdf | anything else is refused **by name** |
| Not supported | OCR | a scanned PDF has no text layer; the error says so instead of writing an empty page |

PDF text uses pure-Rust `pdf-extract` (no external binary to deploy) and is
tidied: wrapped lines joined, end-of-line hyphenation undone, blank-line runs
collapsed. `sources/` is shared by `source`, `retro` and `observation` pages, so
`PAGE_TYPES` names all three and the published tool schema is generated from it
— a folder must never be inferred from a page type (see
`resolve_guessed_folders`).

## Testing

Port zosmaai's test philosophy: vault-format roundtrips,
registry/projection rebuilds, recall scoring fixtures, lint cases,
wikilink gate, MCP parity across connections, space isolation (no
cross-space leakage). Mechanical core target ~85% coverage like upstream.

## Implementation outline

All five milestones landed (v0.5.x → v0.6.0, 2026-09-07 … 2026-09-09).
Kept as the original plan of record:

1. **M1 — vault core:** vault layout, bootstrap, config, templates,
   registry + projections, guardrails. Crate-internal tests.
2. **M2 — read path:** recall (scoring + PRF + layering), search, status,
   read_page, lint (+ auto_fix).
3. **M3 — write path:** capture_source (url/text), ingest batch,
   ensure_page, write_page, retro, observe, log_event, wikilink gate.
4. **M4 — MCP server:** rmcp streamable-HTTP, per-connection space,
   config/env, e2e tool tests.
5. **M5 — cutover:** autopilot retarget, skill vendoring + adaptation,
   repoint agents, decommission the legacy engine.

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

Frontmatter, links, identity (2026-09-07, hardened full support):

- Frontmatter: full YAML via yaml-rust2 (nested maps/seqs, flow
  collections, Karpathy-style tags/dates/aliases, OKF nested
  provenance) behind a fail-closed security shell: anchors/aliases,
  custom tags, multi-doc rejected by pre-scan; 128 KiB / depth-32
  caps; duplicate keys rejected; frontmatter OPTIONAL (Karpathy-style
  plain pages scan fine, title falls back to heading/stem).
- Links: CommonMark event walk (pulldown-cmark) — code spans/blocks,
  images, autolinks, raw HTML never produce backlinks; fragment/query
  stripped, percent-decoded, dot-segments resolved against the source
  page; bundle escape -> link_path_escape diagnostic. Wikilinks kept.
- Identity: page ids NFC-normalized at scan; NFC+casefold collisions
  rejected (concept_identity_collision), page excluded.
- Diagnostics live in registry.json; rejected pages never partially
  enter the registry. ensure_page no longer double-fences content that
  already carries frontmatter.

## wiki_watch (built-in scheduler, 2026-09-07)

The server owns the clock — no crontab, no human steps, no daemon
process. At startup it arms a background thread running a mechanical
maintenance cycle (lint + auto_fix + status) across every bootstrapped
space. Default interval: hourly; `cron_interval_secs` in `config.toml` or
`WIKI_CRON_INTERVAL_SECS` tunes it, `0` disables. Chose a plain thread over
the `cron_tab` crate: fixed interval covers the need, cron expressions are
machinery we don't need (KISS/YAGNI).

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

- Provider: any OpenAI-compatible embeddings endpoint. `embedding_url` is
the **full endpoint** — the server POSTs to it verbatim and does not
append `/embeddings`, so configure e.g. `http://host/v1/embeddings`.
Settable in `config.toml` or via `WIKI_EMBEDDING_URL` +
`WIKI_EMBEDDING_MODEL` (+ optional `WIKI_EMBEDDING_TOKEN`); absent =
feature off with a clean no-op from `wiki_reindex_embeddings`.
- Client timeout 180s: a self-hosted model can take ~54s to answer the
  first, cold call (warm ~12ms). Verified live 2026-09-10 against aiproxy
  `embeddings-local/nomic-embed-text-v1.5` (768 dims, HTTP 200).
- Store: `meta/embeddings.json` —
  `{model, pages: {id: {hash, chunks: [[f32]]}}}`: one vector per **chunk** of
  the page body (target 800 chars, packed by paragraph, hard-split if a
  paragraph is longer, capped at 24 chunks per page). Each chunk carries the
  page title so a bare chunk still has context. Frontmatter is never embedded.
  The per-page ceiling bounds the cost of a very long page — raise it, or add
  a vector index, only if long pages start losing real content.
- Staleness: each page entry carries an FNV-1a hash of the embedded chunk text
  (hand-rolled because `DefaultHasher` is documented as unstable across
  releases and this value is persisted — change detection only, not security).
  Neither `upsert_page` nor `wiki_reindex_embeddings` re-embeds a page whose
  hash and model are unchanged. Verified live 2026-09-10: reindex of an
  unchanged vault = `embedded 0 / skipped 2`, and after editing one page file
  directly on disk (bypassing the server) = `embedded 1 / skipped 1`. A page
  write also leaves the store already current, so the following reindex skips
  it. `reindex` embeds in bounded batches of 64 texts.
- Store format has no migration path (per the hard-cutover rule). A store
  written by an older server does not deserialize, so `upsert_page` **skips**
  instead of lazily creating a fresh store — overwriting would silently drop
  every other page's vectors. One `wiki_reindex_embeddings` rebuilds it.
- Trust-weighted scoring remains future work.
- Blend (additive, not multiplicative — a page with no lexical score has
  nothing to multiply, so only an additive term can admit it):
  `score + 0.5 * 6.0 * max(0, best-chunk cosine)`. The lexical score keeps its
  absolute scale and cosine <= 0 is the identity, so the pure-lexical path is
  unchanged. `SEMANTIC_SCALE = 6.0` is calibrated against this engine's own
  weights: a perfect semantic match (0.5 x 6.0 = 3.0) lands level with a title
  hit (`W_TITLE = 3.0`), so it can reach the top-N but cannot outrank a real
  title match by itself; a strong paraphrase (~0.84) is worth 2.5.
- **Semantic candidates:** a page whose best-chunk cosine clears
  `SEMANTIC_MIN_COSINE = 0.2` is admitted even with **no lexical match at all**,
  scored on the semantic term alone. Before this the layer could only re-rank
  lexical hits, so a query matching a page's body but none of its
  title/id/type/excerpt returned nothing — verified live 2026-09-10 before and
  after: `topic3word7` went from `[]` to `concepts/long-page 2.02`
  (0.5 x 6.0 x 0.674), with the lexical and mixed queries ranking sensibly.
  Candidates come from the space store; the personal layer keeps its lexical
  path (a per-space store holds only that space's pages).
- Staleness: covered above — unchanged pages cost no provider call, and a
  page write leaves the store already current.

## Working memory: trajectories + requirements (2026-09-09)

- `wiki_capture_trajectory` writes an immutable packet under
  `raw/trajectories/TRJ-*` plus a skeleton `cases/` page. **Steps are
  caller-supplied** — the server never sees a live session.
- `wiki_distill_skills` lists undistilled trajectories and marks them
  distilled; `wiki_recall_skill` searches skill/case pages.
- Page types `requirement`, `skill` and `case` extend `PAGE_TYPES` with
  their own directories and templates. Requirements carry lifecycle
  frontmatter (`status`, `priority`, `source_id`, `depends_on`).

## OKF rollout (2026-09-10)

- New spaces bootstrap as `okf-0.2`. The one-time rollout to the existing
  vaults is complete, so there is no migration command: a vault that
  somehow appears in legacy mode is flipped by setting `knowledge_format`
  in its `config.json` by hand.
- Empty OKF vaults keep their root `wiki/index.md`; only per-directory
  indexes are pruned when a directory loses its concepts.

## Page-type resolution (fixed 2026-09-10)

A page without a `type:` frontmatter field takes its type from the
directory via `PAGE_TYPES`. The previous naive "strip the trailing `s`"
fallback produced `analyse` / `entitie` / `synthese` for `analyses` /
`entities` / `syntheses`, skewing status counts and type-filtered search.
Unknown directories keep the naive fallback.

## Background workers (extension, 2026-09-10)

Two unattended workers run outside the main session's context (detached
`pi -p`, `LLM_WIKI_AUTOPILOT_DISABLE=1`, completion surfaced as a UI notice
and written to a log; **all completion notices go through a guarded
`notify()`** because `ctx.ui` throws once the session is replaced and a
re-throw from the catch handler takes pi down).

- **Retro** (`worker-retro.md`) — every 8 settled runs: distills the session
  into wiki pages, judging non-triviality from the transcript tail.
- **Discovery** (`worker-discover.md`) — every 24 settled runs, **on by
  default**: picks ONE gap-linked topic, searches with the MCP web tools,
  skips URLs already captured, captures up to `maxCaptures` (3), then
  synthesizes source + concept + **entity** pages (one per named person,
  organization, tool, or product — the zosmaai entity behaviour).
  `dryRun: true` reports without writing; `discover.enabled: false` turns it
  off. Shares a single-flight guard with retro.

Worker logs are **appended** to `/tmp/llm-wiki-<kind>-<space>.log` with a
`=== run <timestamp> ===` header — truncating raced when two runs shared a
path and overwrote each other's report.

## Non-goals

Auth, multi-user/quotas, data migration (hard cutover — no path from the
legacy engine). Trajectories/working-memory were deferred twice and then
**shipped 2026-09-09**; see *Working memory* above.
