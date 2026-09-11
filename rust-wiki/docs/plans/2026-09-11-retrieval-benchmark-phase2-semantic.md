# Retrieval Benchmark — Phase 2: Semantic (fusion + admission)

Planned 2026-09-11 · Phase 1 (lexical baseline) landed as `0cdd269c`.
Same fixture, same metrics, same held-out discipline — the semantic layer
gets measured, tuned, and gated like the lexical one.

## What Phase 2 measures

Three claims the semantic layer makes, each needs a number:

1. **Admission works** — pages the lexical pass missed are admitted when
   their best-chunk cosine clears `SEMANTIC_MIN_COSINE` (`recall.rs:303`).
   This is the mechanism that makes paraphrase and CJK queries answerable.
2. **Fusion ranks, it doesn't flood** — a semantic boost raises real answers
   without letting unrelated pages crowd out the lexical winner.
3. **The tunables steer** — `SEMANTIC_MIN_COSINE` (0.2), `SEMANTIC_SCALE`
   (6.0), `SEMANTIC_WEIGHT` (0.5) are hand-picked constants; the sweep mode
   below is the instrument for choosing them, not gut feel.

## Embedding seam: deterministic, no provider

Phase 2 must run in CI like Phase 1 — no aiproxy, no 54 s cold start, no
token. The seam is exactly what production already has: `recall_layered_semantic`
takes a precomputed `EmbeddingStore` (`recall.rs:264`), and `EmbeddingStore`
(`embeddings.rs:186`) is a plain public struct — the benchmark constructs it
directly, no embedder call.

To make the vectors *mean* something, the fixture gains an explicit tiny
semantic model:

- **Topic vocabulary** (~10 topics, e.g. `recall`, `storage`, `cards`,
  `dedup`, `links`, `cjk`, `capture`, `workers`, `ranking`, `synthesis`).
- Each page: `topics: &[&str]` (authored once; zero or more).
- Each query: `semantic_topics: &[&str]` — empty by default (pure lexical
  query); only paraphrases/contradictions/CJK-vague author a non-empty set.
- Vector math: per-page vector = mean of its topic one-hot unit vectors,
  L2-normalized, per chunk (build chunks via `embeddings::chunk_text`, the
  same function the engine uses). Query vector likewise from
  `semantic_topics`. Zero topic set ⇒ zero vector ⇒ cosine 0 ⇒ never
  admitted (this is why authored negatives stay clean).

**Honest caveat, stated in the doc:** the stub encodes *assumed* semantic
relations (topic membership). Phase 2 measures **our fusion machinery**, not
the real model's embed quality (that is the provider's job in production).
If real-model behavior ever needs a gate, snapshot a frozen vector set from
the provider once — overkill today.

## Metrics

Everything from Phase 1, plus semantic-specific rows. Always reported
**side-by-side: lexical-only vs lexical+semantic** on the same fixture:

- recall@20, MRR, nDCG@5/10, canonical@3, evidence recall@20,
  contradiction coverage (Phase-1 set — the wounded rows are exactly where
  semantic is supposed to heal: canonical@3 0.60, evidence 0.33,
  contradiction ~0.03).
- `admission_count` — admissions per query (proves the layer fires;
  0.0 would mean the seam is broken, not "nothing needed admitting").
- `semantic_junk_rate` — for judged queries, fraction of top-5 ids with no
  judgment (measures flood; target: not much higher than the lexical
  baseline).
- negative queries: assert **zero** admitted pages by construction
  (zero vectors).
- `miss-cjk` gets **two assertions now**: lexical-only stays a miss
  (Phase-1 gate unchanged); semantic recovers it (authored topics overlap
  the luhmann/zettelkasten page). That is the honest demonstration that the
  semantic layer is language-agnostic — CJK becomes answerable without any
  CJK tokenizer, at the cost of a provider needing the query's meaning.

Baseline records both passes. Default gate run = current constants
(0.2 / 6.0 / 0.5) on train+held-out, asserting equality like Phase 1.

## Sweep mode (fine-tuning)

`BENCHMARK_SEMANTIC_SWEEP=1 cargo test --test recall_benchmark_semantic -- --ignored --nocapture`
runs the fixture over a grid of the three constants **on train only** and
prints a ranking table (canonical@3 / evidence recall / contradiction
coverage / junk rate per combo) — no baseline write, no held-out
inspection while tuning. The chosen constants then become the new
baseline via `BENCHMARK_UPDATE=1`.

Disciplines from Phase 1 carry over verbatim: held-out quarantine, no
reword-to-pass, machine-owned baseline, fixture stability. Tuning against
train with the held-out untouched is the whole point of the split.

## Files

- `tests/support/benchmark_fixture.rs` — + topic vocabulary, per-page
  `topics`, per-query `semantic_topics` (authored, not derived).
- `tests/recall_benchmark_semantic.rs` — new harness: builds the store
  from topics, runs `recall_layered_semantic`, side-by-side metrics,
  sweep mode, miss-cjk semantic assertion.
- `docs/benchmarks/retrieval-phase2-semantic-baseline.json` — committed.
- `docs/design-spec.md` — dated section with the side-by-side reading.

## Steps

1. **Fixture topics** (~1 h): vocabulary, author per-page topics for all 20
   pages, `semantic_topics` for the paraphrase/contradiction/vague/CJK
   queries (≈15 queries). Keep lexical-only queries at empty.
2. **Harness** (~1.5 h): store construction from topics + chunk_text,
   side-by-side metric computation, admission/junk metrics, miss-cjk
   dual assertion, sweep mode reading the three constants.
3. **First honest run + sweep** (~1 h): run with current constants, run the
   train sweep, read both honestly, fix fixture auth errors (impossible
   topic assignments, over-connected topics that flood).
4. **Baseline + docs + gates** (~0.5 h): commit baseline at current
   constants, design-spec section, clippy/fmt/full-suite green.

## Acceptance

- Gate passes on the committed baseline with current constants, zero engine
  changes; side-by-side shows semantic heals at least the paraphrase-class
  queries (canonical@3 and contradiction coverage rise) **without** recall@20
  dropping or junk rate spiking.
- Sweep on train prints a sane ordering (higher floor ⇒ fewer admissions;
  higher scale ⇒ stronger boosts) — the constants respond to the knob,
  which proves the harness senses tuning.
- miss-cjk: lexical miss unchanged, semantic recovered.
- Negatives admit nothing; held-out untouched by any tuning.
- Fixture greps clean of secrets (inherited Phase-1 rule).

## Out of scope

- **Real-provider smoke**: a one-off against aiproxy/nomic to sanity-check
  the stub's directionality — gated behind an env var, never the gate.
- **Frozen real-vector baseline**: only if real-model quality needs
  guarding.
- **CJK tokenization**: the lexical-only miss stays red by design;
  semantic recovery is the point.

Estimated total: ~4 h, one agent pass.
## Executed 2026-09-11

- Config knobs live: `WIKI_RECALL_SEMANTIC_MIN_COSINE|SCALE|WEIGHT`
  (env > config.toml > default), consumed by recall via `config::get()` —
  per-process env makes the sweep loop a plain shell loop over cargo runs.
- Deterministic store: `recall_layered_semantic` consumed the precomputed
  `EmbeddingStore` built from the topic tables — no provider, CI-safe, and
  it directly exercises the same code path production uses.
- Sweep (36 combos, train only) found the floor quantized: this topic space
  only produces cosines {0.333, 0.408, 0.5, 0.577, 0.667, 0.707, 0.816, 1.0},
  so floors 0.1–0.4 behave identically. The informative band is 0.4–0.9.
- **Decision:** default `recall_semantic_min_cosine` 0.2 → **0.6**. Train:
  identical quality (canonical@3 0.729, evidence 0.354) with admissions
  21 → 7 and top-5 junk 0.697 → 0.685. Held-out verified identical
  (canonical@3 0.667, evidence 0.333, coverage 0.083, recall@20 1.0).
  Scale/weight showed no metric movement at these magnitudes — admitted
  pages boost uniformly, rank order preserved; left at 6.0 / 0.5.
- Findings recorded: canonical@3 0.60 → 0.72 fused; MRR 0.61 → 0.71;
  held-out recall 0.875 → 1.0. Contradiction ct-1/ct-5 pass in both layers
  (the 0.03 coverage figure is 2/60 on the all-queries denominator).
- Fixture honesty limits: the stub encodes assumed topic relations — real
  provider quality is out of scope; a frozen real-vector baseline was not
  built (overkill today).
