//! Retrieval benchmark gate (Phase 2, semantic).
//!
//! Same fixture and metrics as Phase 1, but runs every query through BOTH
//! `recall_registry` (lexical-only) and `recall_layered_semantic` (fused),
//! reports side-by-side, and gates both passes against a committed baseline.
//! The embedding store is deterministic (topic vectors from
//! tests/support/benchmark_fixture.rs) — no provider, CI-safe.
//!
//! Disciplines (docs/plans/2026-09-11-retrieval-benchmark-phase2-semantic.md):
//! - held-out quarantine; no reword-to-pass; machine-owned baseline;
//! - negatives must admit nothing (zero semantic vectors by construction);
//! - miss-cjk: lexical stays a miss (Phase-1 gate) but semantic RECOVERS it —
//!   the demonstration that the semantic layer is language-agnostic.
//!
//! Sweep mode (fine-tuning, train only):
//!   BENCHMARK_SEMANTIC_SWEEP=1 cargo test --test recall_benchmark_semantic -- --ignored --nocapture
//! prints one SWEEP line per process env combo; scripts/semantic-sweep.sh
//! loops combos and ranks them.

mod benchmark_fixture {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/benchmark_fixture.rs"));
}
mod bench_metrics {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/bench_metrics.rs"));
}

use rust_wiki::vault::embeddings::{EmbeddingStore, PageVectors};
use rust_wiki::vault::recall::{recall_layered_semantic, recall_registry};
use rust_wiki::vault::registry::rebuild_metadata;

use bench_metrics::{aggregate, build_vault, check_metrics, evaluate, row, Metrics, Baseline, TOP_N};
use benchmark_fixture::{BENCHMARK_VERSION, PAGE_TOPICS, QUERY_SEMANTIC_TOPICS, QUERIES, Query, TOPICS};

const BASELINE_PATH: &str = "docs/benchmarks/retrieval-phase2-semantic-baseline.json";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Phase2Baseline {
    version: u32,
    query_count: usize,
    lexical: Baseline,
    semantic: Baseline,
}

// ─── deterministic topic embeddings ─────────────────────────────────────────

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n <= 0.0 {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / n).collect()
}

/// Multi-hot mean vector for a topic set, L2-normalized. Empty set => zeros
/// (cosine 0 with everything: never a semantic candidate).
fn topic_vec(topics: &[&str]) -> Vec<f32> {
    let mut v = vec![0.0; TOPICS.len()];
    for (i, t) in TOPICS.iter().enumerate() {
        v[i] = topics.iter().filter(|x| *x == t).count() as f32;
    }
    l2_normalize(&v)
}

fn build_store() -> EmbeddingStore {
    let mut store = EmbeddingStore { model: "bench-topics".into(), pages: Default::default() };
    for (id, ts) in PAGE_TOPICS {
        store.pages.insert(
            id.to_string(),
            PageVectors { hash: id.to_string(), chunks: vec![topic_vec(ts)] },
        );
    }
    store
}

fn query_vec(qid: &str) -> Vec<f32> {
    let ts = QUERY_SEMANTIC_TOPICS.iter().find(|(id, _)| *id == qid).map(|(_, t)| *t).unwrap_or(&[]);
    topic_vec(ts)
}

// ─── harness ────────────────────────────────────────────────────────────────

struct QueryRun {
    best_lex: Option<usize>,
    fused_ranked: Vec<String>,
    admitted: usize,
    junk5: Option<f64>,
    metrics_lex: Metrics,
    metrics_fused: Metrics,
}

fn run_all() -> Vec<QueryRun> {
    let (_tmp, v) = build_vault();
    let reg = rebuild_metadata(&v).unwrap();
    let store = build_store();

    QUERIES
        .iter()
        .map(|q| {
            let qv = query_vec(q.id);
            let lex = recall_registry(&v, &reg, q.text, TOP_N as u32, None);
            let lex_ranked: Vec<String> = lex.iter().map(|h| h.id.clone()).collect();
            let fused = recall_layered_semantic(&v, None, &reg, q.text, TOP_N as u32, Some((&qv, &store))).0;
            let fused_ranked: Vec<String> = fused.iter().map(|h| h.id.clone()).collect();

            let (ml, _, bl) = evaluate(q, &lex_ranked);
            let (mf, _, _) = evaluate(q, &fused_ranked);

            // Admissions: fused ids that the lexical pass never produced.
            let admitted = fused_ranked
                .iter()
                .filter(|id| !lex_ranked.iter().any(|x| x == *id))
                .count();

            // Junk rate: fraction of top-5 fused ids with no judgment.
            let junk5 = if q.judgments.is_empty() {
                None
            } else {
                let top5 = &fused_ranked[..fused_ranked.len().min(5)];
                let junk = top5.iter().filter(|id| !q.judgments.iter().any(|j| j.page_id == *id)).count();
                Some(junk as f64 / top5.len().max(1) as f64)
            };

            QueryRun {
                best_lex: bl,
                fused_ranked,
                admitted,
                junk5,
                metrics_lex: ml,
                metrics_fused: mf,
            }
        })
        .collect()
}

fn aggregate_pair(runs: &[QueryRun]) -> ((Metrics, Metrics, Metrics), (Metrics, Metrics, Metrics)) {
    let lex: Vec<(&Query, &Metrics)> = QUERIES.iter().zip(runs.iter()).map(|(q, r)| (q, &r.metrics_lex)).collect();
    let fused: Vec<(&Query, &Metrics)> = QUERIES.iter().zip(runs.iter()).map(|(q, r)| (q, &r.metrics_fused)).collect();
    (aggregate(&lex), aggregate(&fused))
}

fn print_table(label: &str, lex: (&Metrics, &Metrics, &Metrics), fused: (&Metrics, &Metrics, &Metrics)) {
    println!("[bench-sem] == {label} ==");
    println!("[bench-sem]   lexical : {}", row(lex.0));
    println!("[bench-sem]             {}", row(lex.1));
    println!("[bench-sem]             {}", row(lex.2));
    println!("[bench-sem]   fused   : {}", row(fused.0));
    println!("[bench-sem]             {}", row(fused.1));
    println!("[bench-sem]             {}", row(fused.2));
}

#[test]
fn semantic_baseline_gate() {
    let runs = run_all();
    let (lex_all, fused_all) = aggregate_pair(&runs);

    let admissions: usize = runs.iter().map(|r| r.admitted).sum();
    let junk: Vec<f64> = runs.iter().filter_map(|r| r.junk5).collect();
    let junk_avg = junk.iter().sum::<f64>() / junk.len().max(1) as f64;

    print_table(
        "all | train | heldout",
        (&lex_all.0, &lex_all.1, &lex_all.2),
        (&fused_all.0, &fused_all.1, &fused_all.2),
    );
    println!(
        "[bench-sem] admissions total {admissions} (avg {:.2}/query); top-5 junk rate {:.3}",
        admissions as f64 / runs.len() as f64,
        junk_avg
    );

    // miss-cjk: fused must recover the zettelkasten page; lexical still misses.
    let miss_idx = QUERIES.iter().position(|q| q.id == "miss-cjk").unwrap();
    let miss = &runs[miss_idx];
    assert!(miss.best_lex.is_none(), "miss-cjk lexically recovered — Phase-2 semantic assertion violated");
    assert!(
        miss.fused_ranked.iter().any(|id| id == "sources/luhmann-zettelkasten"),
        "miss-cjk: semantic layer failed to recover the zettelkasten page"
    );
    println!("[bench-sem] miss-cjk: lexical miss, semantic recovered: ok");

    // Negatives must admit nothing (zero query vectors by construction).
    let negs: Vec<_> = QUERIES.iter().enumerate().filter(|(_, q)| q.judgments.is_empty()).collect();
    for (i, q) in &negs {
        assert_eq!(
            runs[*i].admitted, 0,
            "negative query '{}' admitted {} pages — semantic vectors must be empty for unjudged queries",
            q.id, runs[*i].admitted
        );
    }
    println!("[bench-sem] negatives admit nothing: ok ({} queries)", negs.len());

    if std::env::var("BENCHMARK_SEMANTIC_SWEEP").as_deref() == Ok("1") {
        // Fine-tune mode: append a machine row for the current env combo
        // (train split only — held-out quarantine), write no baseline.
        let cos = rust_wiki::config::get().recall_semantic_min_cosine;
        let scale = rust_wiki::config::get().recall_semantic_scale;
        let weight = rust_wiki::config::get().recall_semantic_weight;
        let row = format!(
            "SWEEP\t{cos:.2}\t{scale:.1}\t{weight:.2}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{admissions}",
            fused_all.1.canonical_at3, fused_all.1.evidence_recall20, fused_all.1.contradiction_coverage, junk_avg
        );
        println!("{row}");
        let out = std::env::var("BENCHMARK_SWEEP_OUT").unwrap_or_else(|_| "/tmp/semantic-sweep.tsv".into());
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&out)
            .and_then(|mut f| f.write_all(format!("{row}\n").as_bytes()))
            .expect("cannot append sweep row");
        return;
    }

    if std::env::var("BENCHMARK_UPDATE").as_deref() == Ok("1") {
        let baseline = Phase2Baseline {
            version: BENCHMARK_VERSION,
            query_count: QUERIES.len(),
            lexical: Baseline { version: BENCHMARK_VERSION, query_count: QUERIES.len(), all: lex_all.0, train: lex_all.1, heldout: lex_all.2 },
            semantic: Baseline { version: BENCHMARK_VERSION, query_count: QUERIES.len(), all: fused_all.0, train: fused_all.1, heldout: fused_all.2 },
        };
        let json = serde_json::to_string_pretty(&baseline).unwrap();
        std::fs::write(BASELINE_PATH, json).unwrap();
        println!("[bench-sem] baseline updated at {BASELINE_PATH}");
        return;
    }

    let raw = std::fs::read_to_string(BASELINE_PATH)
        .unwrap_or_else(|_| panic!("no baseline at {BASELINE_PATH} — run with BENCHMARK_UPDATE=1 to create it"));
    let base: Phase2Baseline = serde_json::from_str(&raw).unwrap();
    assert_eq!(base.version, BENCHMARK_VERSION, "fixture version changed — baseline must be regenerated");
    assert_eq!(base.query_count, QUERIES.len(), "query count changed — baseline must be regenerated");
    check_metrics("semantic.all", &fused_all.0, &base.semantic.all);
    check_metrics("semantic.train", &fused_all.1, &base.semantic.train);
    check_metrics("semantic.heldout", &fused_all.2, &base.semantic.heldout);
    check_metrics("lexical.all", &lex_all.0, &base.lexical.all);
    check_metrics("lexical.train", &lex_all.1, &base.lexical.train);
    check_metrics("lexical.heldout", &lex_all.2, &base.lexical.heldout);
}