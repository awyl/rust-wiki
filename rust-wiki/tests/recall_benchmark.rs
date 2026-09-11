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

use rust_wiki::vault::recall::recall_registry;
use rust_wiki::vault::registry::rebuild_metadata;
use rust_wiki::vault::{bootstrap, VaultPaths};

use benchmark_fixture::{Judgment, Query, Role, Split, BENCHMARK_VERSION, PAGES, QUERIES};

const BASELINE_PATH: &str = "docs/benchmarks/retrieval-phase1-baseline.json";
const TOP_N: usize = 20;
/// Workaround for cross-version float noise; metrics are ratios, 1e-9 is far
/// below anything that could mask a real regression.
const EPS: f64 = 1e-9;

// ─── metrics ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Metrics {
    candidate_recall20: f64,
    mrr: f64,
    ndcg5: f64,
    ndcg10: f64,
    canonical_at3: f64,
    evidence_recall20: f64,
    contradiction_coverage: f64,
    /// Negative queries that returned nothing at all (lexical; positives are
    /// the hook for Phase-2 semantic admission).
    negative_empty_rate: f64,
}

impl Metrics {
    fn zero() -> Self {
        Metrics {
            candidate_recall20: 0.0,
            mrr: 0.0,
            ndcg5: 0.0,
            ndcg10: 0.0,
            canonical_at3: 0.0,
            evidence_recall20: 0.0,
            contradiction_coverage: 0.0,
            negative_empty_rate: 0.0,
        }
    }
    fn add(&mut self, o: &Metrics) {
        self.candidate_recall20 += o.candidate_recall20;
        self.mrr += o.mrr;
        self.ndcg5 += o.ndcg5;
        self.ndcg10 += o.ndcg10;
        self.canonical_at3 += o.canonical_at3;
        self.evidence_recall20 += o.evidence_recall20;
        self.contradiction_coverage += o.contradiction_coverage;
        self.negative_empty_rate += o.negative_empty_rate;
    }
    fn div(&self, n: usize) -> Metrics {
        let n = n.max(1) as f64;
        Metrics {
            candidate_recall20: self.candidate_recall20 / n,
            mrr: self.mrr / n,
            ndcg5: self.ndcg5 / n,
            ndcg10: self.ndcg10 / n,
            canonical_at3: self.canonical_at3 / n,
            evidence_recall20: self.evidence_recall20 / n,
            contradiction_coverage: self.contradiction_coverage / n,
            negative_empty_rate: self.negative_empty_rate / n,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Baseline {
    version: u32,
    query_count: usize,
    all: Metrics,
    train: Metrics,
    heldout: Metrics,
}

fn dcg(grades: &[(usize, u8)], k: usize) -> f64 {
    grades
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, (_, g))| *g as f64 / (i as f64 + 2.0).log2())
        .sum()
}

fn ndcg(ranked: &[(usize, u8)], grades: &[Judgment], k: usize) -> f64 {
    let idcg = {
        let mut gs: Vec<u8> = grades.iter().map(|j| j.grade).collect();
        gs.sort_by(|a, b| b.cmp(a));
        dcg(&gs.into_iter().enumerate().collect::<Vec<_>>(), k)
    };
    if idcg == 0.0 {
        return 0.0;
    }
    dcg(ranked, k) / idcg
}

fn hit_highest(ranked: &[(usize, u8)], grades: &[Judgment]) -> Option<(usize, u8)> {
    let top = grades.iter().map(|j| j.grade).max().unwrap_or(0);
    ranked.iter().copied().find(|(_, g)| *g == top).map(|(r, g)| (r + 1, g))
}

/// Grade one query from the ranked id list. Returns (query_metrics, misses:
/// relevant page ids that never appeared, ranked_hit position of best).
fn evaluate(q: &Query, ranked: &[String]) -> (Metrics, Vec<&'static str>, Option<usize>) {
    let grade_of = |id: &str| q.judgments.iter().find(|j| j.page_id == id).map(|j| j.grade);
    let mut ranked_graded: Vec<(usize, u8)> = ranked
        .iter()
        .enumerate()
        .filter_map(|(i, id)| grade_of(id).map(|g| (i, g)))
        .collect();
    ranked_graded.sort_by_key(|(i, _)| *i);

    // Contradiction coverage: every expected_conflicts id inside TOP_N.
    let coverage = if q.expected_conflicts.is_empty() {
        None
    } else {
        Some(q.expected_conflicts.iter().all(|id| ranked.iter().take(TOP_N).any(|r| r == id)))
    };

    // Misses: judged pages absent from the ranked list (or beyond TOP_N).
    let misses: Vec<&'static str> = q
        .judgments
        .iter()
        .filter(|j| !ranked.iter().take(TOP_N).any(|r| r == j.page_id))
        .map(|j| j.page_id)
        .collect();

    let mut m = Metrics::zero();

    // candidate recall@TOP_N: retrieved judged pages / all judged pages.
    if !q.judgments.is_empty() {
        let found = q.judgments.len().saturating_sub(misses.len());
        m.candidate_recall20 = found as f64 / q.judgments.len() as f64;
    }

    // MRR: 1 / rank of the FIRST hit_graded ever retrieved.
    if let Some((first, _)) = ranked_graded.first() {
        m.mrr = 1.0 / (*first as f64 + 1.0);
    } else if !q.judgments.is_empty() {
        m.mrr = 0.0;
    }

    // nDCG@5/@10 from graded ranks.
    m.ndcg5 = if ranked_graded.is_empty() { 0.0 } else { ndcg(&ranked_graded, q.judgments, 5) };
    m.ndcg10 = if ranked_graded.is_empty() { 0.0 } else { ndcg(&ranked_graded, q.judgments, 10) };

    // canonical@3: a grade-3 canonical page inside the top 3.
    let canonicals: Vec<&Judgment> =
        q.judgments.iter().filter(|j| j.grade == 3 && j.role == Role::Canonical).collect();
    if !canonicals.is_empty() {
        let any_in3 = canonicals.iter().any(|j| ranked.iter().take(3).any(|r| r == j.page_id));
        m.canonical_at3 = if any_in3 { 1.0 } else { 0.0 };
    }

    // evidence recall@20.
    let evidence: Vec<&Judgment> = q.judgments.iter().filter(|j| j.role == Role::Evidence).collect();
    if !evidence.is_empty() {
        let found = evidence.iter().filter(|j| ranked.iter().take(TOP_N).any(|r| r == j.page_id)).count();
        m.evidence_recall20 = found as f64 / evidence.len() as f64;
    }

    if let Some(cov) = coverage {
        m.contradiction_coverage = if cov { 1.0 } else { 0.0 };
    }

    // Negative-empty rate (only meaningful for empty-judgment negatives).
    if q.judgments.is_empty() {
        m.negative_empty_rate = if ranked.is_empty() { 1.0 } else { 0.0 };
    }

    let best = hit_highest(&ranked_graded, q.judgments).map(|(r, _)| r);
    (m, misses, best)
}

// ─── harness ───────────────────────────────────────────────────────────────

fn build_vault() -> (tempfile::TempDir, VaultPaths) {
    let tmp = tempfile::tempdir().unwrap();
    let v = VaultPaths::new(tmp.path(), "bench");
    bootstrap(&v, "t0").unwrap();
    for p in PAGES {
        let path = v.page_path(p.id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, p.markdown).unwrap();
    }
    (tmp, v)
}

fn run_once() -> Vec<(Option<usize>, Metrics)> {
    let (_tmp, v) = build_vault();
    let reg = rebuild_metadata(&v).unwrap();
    QUERIES
        .iter()
        .map(|q| {
            let hits = recall_registry(&v, &reg, q.text, TOP_N as u32, None);
            let ranked: Vec<String> = hits.iter().map(|h| h.id.clone()).collect();
            let (m, _, best) = evaluate(q, &ranked);
            (best, m)
        })
        .collect()
}

fn aggregate(runs: &[(Option<usize>, Metrics)]) -> (Metrics, Metrics, Metrics) {
    let mut all = Metrics::zero();
    let mut train = Metrics::zero();
    let mut held = Metrics::zero();
    let mut n_all = 0;
    let mut n_train = 0;
    let mut n_held = 0;
    for (idx, (_, m)) in runs.iter().enumerate() {
        all.add(m);
        n_all += 1;
        match QUERIES[idx].split {
            Split::Train => {
                train.add(m);
                n_train += 1;
            }
            Split::Heldout => {
                held.add(m);
                n_held += 1;
            }
        }
    }
    (all.div(n_all), train.div(n_train), held.div(n_held))
}

fn row(m: &Metrics) -> String {
    format!(
        "R@20 {:.3} | MRR {:.3} | nDCG@5 {:.3} | nDCG@10 {:.3} | can@3 {:.3} | evR@20 {:.3} | ctrCov {:.3} | negEmpty {:.3}",
        m.candidate_recall20, m.mrr, m.ndcg5, m.ndcg10, m.canonical_at3, m.evidence_recall20, m.contradiction_coverage, m.negative_empty_rate
    )
}

fn assert_close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= EPS, "baseline drift in {what}: engine now {a}, baseline {b}");
}

fn check_metrics(name: &str, got: &Metrics, base: &Metrics) {
    assert_close(got.candidate_recall20, base.candidate_recall20, &format!("{name}.candidate_recall20"));
    assert_close(got.mrr, base.mrr, &format!("{name}.mrr"));
    assert_close(got.ndcg5, base.ndcg5, &format!("{name}.ndcg5"));
    assert_close(got.ndcg10, base.ndcg10, &format!("{name}.ndcg10"));
    assert_close(got.canonical_at3, base.canonical_at3, &format!("{name}.canonical_at3"));
    assert_close(got.evidence_recall20, base.evidence_recall20, &format!("{name}.evidence_recall20"));
    assert_close(got.contradiction_coverage, base.contradiction_coverage, &format!("{name}.contradiction_coverage"));
    assert_close(got.negative_empty_rate, base.negative_empty_rate, &format!("{name}.negative_empty_rate"));
}

#[test]
fn retrieval_baseline_gate() {
    let runs = run_once();
    let (all, train, held) = aggregate(&runs);

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

    // miss-cjk must stay a miss.
    let miss = &runs[QUERIES.iter().position(|q| q.id == "miss-cjk").unwrap()];
    assert!(
        miss.0.is_none(),
        "miss-cjk was recovered lexically — only CJK-aware tokenization (Phase 2) may flip this"
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