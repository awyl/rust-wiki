# rust-wiki

A remote, OKF-shaped wiki MCP server: multi-space vaults of plain Markdown
pages, a fully mechanical engine (no LLM, no RAG plumbing), semantic recall
via an optional embedding provider, and a built-in maintenance scheduler.

Single binary, no auth (trusted-network deployment), HTTP MCP transport so
any number of agents — across containers and machines — can share one
knowledge base.

## Quick start

```bash
cargo build --release
./target/release/rust-wiki            # MCP over stdio (default — for MCP hosts)
./target/release/rust-wiki serve      # MCP over HTTP on 0.0.0.0:8484
```

**stdio (default):** newline-delimited JSON-RPC on stdin/stdout — the mode
for MCP hosts that spawn the binary themselves (aiproxy, Claude Desktop,
pi). Protocol owns stdout; all diagnostics go to stderr.

**HTTP (`serve`):** streamable HTTP at `http://<host>:8484/mcp`
(JSON-RPC; responses may be SSE-framed) — the mode for sharing one vault
across containers and machines.

## Configuration

Precedence: **environment → `<exe_dir>/config.toml` → defaults**.

| Setting | Env | config.toml | Default |
|---|---|---|---|
| Vault root | `WIKI_VAULT_ROOT` | `vault_root` | `<exe_dir>/vaults` |
| Port (HTTP mode) | `WIKI_PORT` | `port` | `8484` |
| Maintenance interval | `WIKI_CRON_INTERVAL_SECS` | — | `3600` (hourly) |
| Embedding endpoint | `WIKI_EMBEDDING_URL` | — | unset (feature off) |
| Embedding model | `WIKI_EMBEDDING_MODEL` | — | `text-embedding-3-small` |
| Embedding bearer token | `WIKI_EMBEDDING_TOKEN` | — | unset |
| Recall links-first threshold | `WIKI_RECALL_LINKS_FIRST_THRESHOLD` | — | `50` (0 = always links-first) |
| Git commit interval | `WIKI_GIT_INTERVAL_SECS` | — | `60` (0 = disabled) |
| Git idle threshold | `WIKI_GIT_IDLE_SECS` | — | `300` |

Example `config.toml` (must sit next to the binary):

```toml
port = 8484
vault_root = "/var/lib/rust-wiki/vaults"
```

### Vault root

Every space is a directory under the vault root:

```
<vault_root>/
├── personal/                  # auto-created at boot; cross-project layer
└── <space-name>/
    ├── config.json            # knowledge_format: "okf-0.2", created_at
    ├── README.md              # orientation page (written at bootstrap)
    ├── wiki/                  # editable pages: concepts/ entities/ sources/
    │   ├── index.md           # GENERATED — never hand-edit
    │   ├── log.md             # GENERATED
    │   └── ...
    ├── raw/                   # immutable captured sources (SRC-*)
    ├── meta/                  # registry.json, events.jsonl, embeddings.json
    ├── templates/
    └── outputs/
```

Files under `raw/` and `meta/`, and the generated `wiki/index.md`,
`wiki/*/index.md`, `wiki/log.md`, are server-owned — direct writes are
rejected by guardrails.

### Built-in maintenance scheduler

At startup the server arms a background thread that runs a mechanical
cycle across every bootstrapped space: lint (with auto-fix stubs) →
status, one log line per space. No crontab, no human steps.

- default: hourly
- `WIKI_CRON_INTERVAL_SECS=1800` — every 30 minutes
- `WIKI_CRON_INTERVAL_SECS=0` — disable

Manual one-space cycle: `./target/release/rust-wiki cron --space <name>`.

### Semantic recall (optional)

Without an embedding provider the engine is purely lexical (in-process,
microseconds, no network). To enable semantic recall:

```bash
export WIKI_EMBEDDING_URL="https://api.openai.com/v1/embeddings"   # any OpenAI-compatible endpoint
export WIKI_EMBEDDING_MODEL="text-embedding-3-small"
export WIKI_EMBEDDING_TOKEN="sk-..."                               # optional
```

Then per space, once and after bulk imports, call the
`wiki_reindex_embeddings` tool. Vectors are stored in
`meta/embeddings.json`; `wiki_recall` embeds the query and blends cosine
similarity into lexical scores. No provider → clean no-op message.

### Spaces

- Create on demand: `wiki_bootstrap` tool with a `space` name (or let the
  pi-rust-wiki autopilot do it automatically per project).
- `personal` is reserved and created automatically at server start —
  pages there surface in every space's recall (layered recall).
- Pin a connection: `wiki_use_space` (per-connection, server-side).

## Tools (21)

`wiki_bootstrap`, `wiki_use_space`, `wiki_capture_source`, `wiki_ingest`,
`wiki_ensure_page`, `wiki_template`, `wiki_read_page`, `wiki_write_page`, `wiki_recall`,
`wiki_search`, `wiki_retro`, `wiki_observe`, `wiki_lint`, `wiki_status`,
`wiki_rebuild_meta`, `wiki_watch`, `wiki_reindex_embeddings`,
`wiki_log_event`, `wiki_capture_trajectory`, `wiki_distill_skills`,
`wiki_recall_skill`.

`wiki_watch` with no arguments reports scheduler status;
`{"run": true}` triggers an immediate all-spaces maintenance cycle.

## Frontmatter and links

Pages are plain Markdown. Frontmatter is optional full YAML (nested maps,
flow lists, Karpathy-style `tags`/`date`/`source_count`, OKF v0.2 nested
`sources:`/`verified:`) parsed behind a fail-closed shell: anchors,
aliases, custom tags, multiple documents, duplicate keys, >128 KiB,
depth >32 → the page is excluded from the registry with a diagnostic in
`meta/registry.json` (`diagnostics[]`). Links: CommonMark links and
`[[folder/page]]` wikilinks both produce backlinks; page ids are
NFC-normalized with collision detection.

## Security model

- **No auth.** Deploy on a trusted network only (LAN, VPN, or behind a
  proxy that enforces auth). Anyone with network access to the port can
  read and write all spaces.
- The embedding token (`WIKI_EMBEDDING_TOKEN`) stays server-side and is
  only sent to the endpoint you configure.
- Events (`log.md` details) may contain caller-supplied content — don't
  put secrets in tool arguments you wouldn't want in the wiki.

## Companion: pi-rust-wiki extension

The `extensions/pi-rust-wiki/` package gives the
[pi](https://github.com/badlogic/pi-mono) coding agent an autopilot:
silent per-project space bootstrap, a research-nudge footer, and an
automatic retro worker. See its README.
