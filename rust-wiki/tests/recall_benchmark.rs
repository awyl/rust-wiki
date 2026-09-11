//! Retrieval benchmark gate (Phase 1, lexical).
//!
//! Builds the fixture vault, runs every graded query through
//! `vault::recall::recall_registry`, computes metrics from ranked ids, and
//! asserts equality against the committed baseline. `BENCHMARK_UPDATE=1`
//! regenerates the baseline after an intentional engine or fixture change.
//!
//! Disciplines (see docs/plans/2026-09-11-retrieval-benchmark.md):
//! - held-out judgments are never inspected while tuning;
//! - no rewording a query to make it pass; the `miss-cjk` miss is asserted;
//! - the baseline JSON is machine-owned (`BENCHMARK_UPDATE=1` is the only writer).

mod benchmark_fixture {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/benchmark_fixture.rs"));
}
mod bench_metrics {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/bench_metrics.rs"));
}

use rust_wiki::vault::recall::recall_registry;
use rust_wiki::vault::registry::rebuild_metadata;

use bench_metrics::{aggregate, build_vault, check_metrics, row, Metrics, Baseline, TOP_N};
use benchmark_fixture::{BENCHMARK_VERSION, Query, QUERIES};

const BASELINE_PATH: &str = "docs/benchmarks/retrieval-phase1-baseline.json";

fn run_once() -> Vec<(Option<usize>, Metrics)> {
    let (_tmp, v) = build_vault();
    let reg = rebuild_metadata(&v).unwrap();
    QUERIES
        .iter()
        .map(|q| {
            let hits = recall_registry(&v, &reg, q.text, TOP_N as u32, None);
            let ranked: Vec<String> = hits.iter().map(|h| h.id.clone()).collect();
            let (m, _, best) = bench_metrics::evaluate(q, &ranked);
            (best, m)
        })
        .collect()
}

#[test]
fn retrieval_baseline_gate() {
    let runs = run_once();
    let measured: Vec<(&Query, &Metrics)> = QUERIES.iter().zip(runs.iter()).map(|(q, (_, m))| (q, m)).collect();
    let (all, train, held) = aggregate(&measured);

    // Print table always, so `--nocapture` shows it.
    println!("[bench] all   : {}", row(&all));
    println!("[bench] train : {}", row(&train));
    println!("[bench] held  : {}", row(&held));

    // Worst five queries by best-relevant rank (or MISS if never found).
    let mut worst: Vec<(&'static str, &'static str, Option<usize>)> = QUERIES
        .iter()
        .zip(&runs)
        .map(|(q, (best, _))| (q.id, q.category.label(), *best))
        .collect();
    worst.sort_by_key(|(_, _, b)| b.unwrap_or(usize::MAX));
    println!("[bench] worst 5 by first relevant rank:");
    for (id, cat, best) in worst.iter().take(5) {
        println!(
            "  {id:10} {cat:22} first-relevant-rank {}",
            best.map(|r| r.to_string()).unwrap_or_else(|| "MISS".into())
        );
    }

    // miss-cjk must stay a miss (lexical-only path; Phase-2 semantic recovers it).
    let miss = &runs[QUERIES.iter().position(|q| q.id == "miss-cjk").unwrap()];
    assert!(
        miss.0.is_none(),
        "miss-cjk was recovered lexically — only CJK-aware tokenization or the semantic layer may flip this"
    );
    println!("[bench] miss-cjk still a miss: ok");

    if std::env::var("BENCHMARK_UPDATE").as_deref() == Ok("1") {
        let baseline = Baseline {
            version: BENCHMARK_VERSION,
            query_count: QUERIES.len(),
            all,
            train,
            heldout: held,
        };
        let json = serde_json::to_string_pretty(&baseline).unwrap();
        std::fs::write(BASELINE_PATH, json).unwrap();
        println!("[bench] baseline updated at {BASELINE_PATH}");
        return;
    }

    let raw = std::fs::read_to_string(BASELINE_PATH)
        .unwrap_or_else(|_| panic!("no baseline at {BASELINE_PATH} — run with BENCHMARK_UPDATE=1 to create it"));
    let base: Baseline = serde_json::from_str(&raw).unwrap();
    assert_eq!(base.version, BENCHMARK_VERSION, "fixture version changed — baseline must be regenerated");
    assert_eq!(base.query_count, QUERIES.len(), "query count changed — baseline must be regenerated");
    check_metrics("all", &all, &base.all);
    check_metrics("train", &train, &base.train);
    check_metrics("heldout", &held, &base.heldout);
}