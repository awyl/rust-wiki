# Retrieval Benchmark (Phase 1, lexical)

Planned 2026-09-11 · mirrors zosmaai/pi-llm-wiki `docs/retrieval-benchmark.md` +
`test/helpers/retrieval-metrics.ts` + `test/fixtures/retrieval-benchmark/fixture.ts`,
adapted to rust-wiki (Rust server, no extension auto-inject, configurable k).

## Why

Recall is the core retrieval surface of this wiki. Since the first chunk-embedding
change we have verified every recall behavior *by hand after the fact*. Upstream
treats retrieval as a graded, held-out discipline: 60 queries, train/holdout split,
metrics computed from ranked ids, a committed baseline, and a CI gate. We have none.

This plan builds the Phase 1 lexical baseline — deterministic, no provider needed,
safe to run in CI. Semantic (embeddings) is Phase 2 (out of scope here; see follow-ups).

## Corpus

`tests/fixtures/benchmark.rs` — ~20 sanitized Markdown pages + 60 graded queries.

Pages: modeled on **our own vault topics** so the benchmark measures our engine on
the content it actually serves:

- `entities/nomic-embed-text-v1-5` (embedding model, aiproxy), `entities/rust-wiki` (our server)
- `concepts/okf-v0-2` (frontmatter/layout), `concepts/embedding-store` (chunk vectors),
  `concepts/semantic-candidate-admission` (additive fusion), `concepts/wikilink-gate`,
  `concepts/relevance-claim` (the multiplier), `concepts/trajectory-packet`,
  `concepts/env-to-config-fallback`, `concepts/folder-guessing` (never infer folder
  from type; `retros/` does not exist), `concepts/karpathy-pattern`
- `sources/pdf-text-extraction`, `sources/wiki-okf-incompatibility`, `sources/luhmann-zettelkasten`
- `retros/git-auto-commit-idle-bug`, `retros/personal-layer-ranking-bug`,
  `retros/retro-window-clamp`, `retros/worker-duplicate-generation`
- `analyses/embeddings-vs-qmd`, `syntheses/worker-structured-output`

Each page: `id`, `type`, `title`, `body`, optional `aliases` (tested via title/id
fallback — the registry only stores id/title/type/excerpt, so aliases become
title synonyms in the judgment prose), optional `tags`.

Queries (60) across the upstream categories, split train/holdout 45/15:

| Category | Meaning | Example |
|---|---|---|
| exact_lookup | exact id/title terms | "wikilink gate" |
| entity_alias | entity known by other name | "nomic v1.5 embedder" |
| paraphrase | same idea, different words | "turn a document into numbers before storing" |
| vague_recollection | fuzzy memory | "the thing that kept saying in sync" |
| conceptual | idea/pattern question | "what is the karpathy pattern?" |
| graph_scope | follows links | "what breaks when pages guess their folder?" |
| evidence_request | needs a source | "what does the luhmann source say about addresses?" |
| temporal | time-bound | "which retro came first?" |
| contradiction | claims both sides | "embedding store size vs semantic admission" |
| conclusion | asks the sealed conclusion | "is serving state in sync proof?" |
| synthesis | cross-cutting | "how do workers avoid duplicate pages?" |
| negative | guaranteed no match | "how to configure postgres replication" |

**One intentional guaranteed miss:** CJK query `什么是卡片盒笔记法` targets the
`luhmann-zettelkasten` page (English-only body). Our tokenizer is ASCII word +
lowercase; a CJK run is one token, lexical score is zero by construction. Do
**not** reword the page to make it pass — the gate *asserts the miss* (query id
`miss-cjk`), and only CJK-aware tokenization (Phase 2) may flip it. It doubles as
the CJK acceptance proof.

Sanitization rules (upstream, adopted): the fixture must not contain raw home paths,
emails, credentials, customer identifiers, or copied private notes. Phrase
representative queries yourself; never copy verbatim vault content that is private.

## Judgments

Per query, a list of `{ page_id, grade, role }`:

- `3` = directly answers / canonical page
- `2` = useful supporting evidence or secondary answer
- `1` = relevant context
- roles: `canonical` | `evidence`
- contradiction queries also carry `expected_conflicts` (ids that must appear together)

Grades are authored once (human judgment) and never changed to make a metric
pass. Held-out judgments are **immutable while tuning** (see Disciplines).

## Metrics (from ranked IDs, never raw scores)

- candidate Recall@20
- MRR
- nDCG@5, nDCG@10
- canonical@3 (was a grade-3 canonical in top 3?)
- evidence Recall@20
- contradiction coverage (all `expected_conflicts` in top-N?)
- reported separately: `all`, `train`, `heldout`

Upstream also tracks `autoFalsePositiveRate` (their extension auto-injects recall
results into the session; ours has no equivalent — dropped deliberately, noted here).

## Harness

New integration test `rust-wiki/tests/recall_benchmark.rs` (cargo auto-compiles;
uses the lib, needs no running server):

1. Build the fixture vault: `bootstrap` + write each page (reuse the inline-test
   pattern: `VaultPaths::new`, `bootstrap`, `fs::write`).
2. `registry::rebuild_metadata` once.
3. For each query: `vault::recall::recall_registry(&vault, &reg, text, /*max_results*/ 20)`.
4. Grade with a metrics module (`RecallMetrics` mirroring upstream names).
5. Compare against the committed baseline
   `rust-wiki/docs/benchmarks/retrieval-phase1-baseline.json`:
   - default run asserts equality (it is a *recall regression gate*);
   - `BENCHMARK_UPDATE=1 cargo test --test recall_benchmark -- --ignored` (ignored
     test, gated by env) regenerates the baseline after an intentional change.

Public surface needed (verify, add `pub` where missing in `lib.rs`/`vault/`):
`VaultPaths::new`, `vault::bootstrap`, `vault::registry::rebuild_metadata`,
`vault::recall::recall_registry`. Integration-test reachability forces clean
public API — a side benefit.

## Commands

```bash
cargo test --test recall_benchmark          # verify baseline (gate)
cargo test --test recall_benchmark -- --ignored --nocapture  # print metric table
BENCHMARK_UPDATE=1 cargo test --test recall_benchmark -- --ignored --nocapture  # regen baseline
```

Report format: compact table `all | train | heldout` per metric, plus the 3 worst
queries by rank (for debugging), printed in the ignored run.

## Disciplines

1. **Held-out quarantine.** Tune only against `train`. Never inspect or alter
   held-out judgments while tuning; inspect them for reporting only.
2. **No rewording to pass.** A failing query is a bug in the engine or a bad
   judgment — not an excuse to rephrase the query. Document `expectedFailures`
   (allowed: the CJK negative, and any query whose judgment is provably wrong).
3. **Baseline is machine-owned.** Never hand-edit the committed baseline JSON.
   Every `BENCHMARK_UPDATE` commit must state why engine or fixture changed.
4. **Fixture stability.** Pages/queries change only by an explicit edit that names
   the reason; a changed judgment is a full re-authoring, not a tweak.

## Steps

1. **Fixture** (~2 h): `tests/fixtures/benchmark.rs` — 20 pages + 60 graded queries
   (12 categories × ~5), sanitized, with the CJK negative. Author judgments by
   reading each page once, top-3 + evidence set per query.
2. **Harness + metrics** (~1.5 h): `tests/recall_benchmark.rs` + metrics module
   (Recall@k, MRR, nDCG@k, canonical@3, evidence recall, contradiction coverage).
   Wire the `--ignored`/env-var update path. Expose any missing `pub` API, add
   `#[cfg(test)]`-style separation so the gate test is fast (<2 s).
3. **Baseline** (~0.5 h): run, read the metric table honestly, fix obvious fixture
   bugs (judgment mistakes, sanitization misses), run clean, commit baseline at
   `docs/benchmarks/retrieval-phase1-baseline.json`. Expect heldout to be solid and
   `train` slightly better — that asymmetry is the metric working.
4. **CI-ify** (optional, ~0.5 h): add the test to the existing gate script so every
   `cargo test` run re-verifies the baseline (default `cargo test` already runs it
   as a normal test — decide if it stays fast enough; else `--test-threads`).

## Acceptance

- `cargo test --test recall_benchmark` passes against the committed baseline with
  zero engine changes since the baseline commit.
- Metric table prints for `--ignored --nocapture`.
- 60/60 queries have ≥1 judgment; heldout 15 are untouched by any tuning.
- Baseline JSON committed; no secret/path leakage in fixture (grep the fixture for
  `/home/`, `@`, `token`, `secret` before commit).

## Out of scope / follow-ups

- **Phase 2 semantic bench:** same fixture, `recall_layered_semantic` with a real
  provider store; needs provider in CI or a fixture-store seam — do not block Phase 1.
- **CJK tokenization:** the negative stays red until Phase 2; it is the acceptance
  proof for CJK-aware lexical handling if we ever build it.
- **Extension nudge precision** (`autoExpectation` upstream): covered by vitest
  hooks tests, not this gate.

Estimated total: ~4 h. All estimates are execution time for one agent pass.