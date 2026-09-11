// Shared metric machinery for the retrieval benchmark harnesses. Spliced in
// via `mod bench_metrics { include!(...) }` at each harness crate root, where
// `super::benchmark_fixture` is the sibling module of the same crate.

use super::benchmark_fixture::{Judgment, Query, Role, Split, PAGES};

pub const TOP_N: usize = 20;
/// Tolerance for baseline equality; metrics are ratios, 1e-9 is below anything
/// that could mask a real regression.
pub const EPS: f64 = 1e-9;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Metrics {
    pub candidate_recall20: f64,
    pub mrr: f64,
    pub ndcg5: f64,
    pub ndcg10: f64,
    pub canonical_at3: f64,
    pub evidence_recall20: f64,
    pub contradiction_coverage: f64,
    pub negative_empty_rate: f64,
}

impl Metrics {
    pub fn zero() -> Self {
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
    pub fn add(&mut self, o: &Metrics) {
        self.candidate_recall20 += o.candidate_recall20;
        self.mrr += o.mrr;
        self.ndcg5 += o.ndcg5;
        self.ndcg10 += o.ndcg10;
        self.canonical_at3 += o.canonical_at3;
        self.evidence_recall20 += o.evidence_recall20;
        self.contradiction_coverage += o.contradiction_coverage;
        self.negative_empty_rate += o.negative_empty_rate;
    }
    pub fn div(&self, n: usize) -> Metrics {
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
pub struct Baseline {
    pub version: u32,
    pub query_count: usize,
    pub all: Metrics,
    pub train: Metrics,
    pub heldout: Metrics,
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
/// judged ids absent from the top-N, best-graded rank).
pub fn evaluate(q: &Query, ranked: &[String]) -> (Metrics, Vec<&'static str>, Option<usize>) {
    let grade_of = |id: &str| q.judgments.iter().find(|j| j.page_id == id).map(|j| j.grade);
    let mut ranked_graded: Vec<(usize, u8)> = ranked
        .iter()
        .enumerate()
        .filter_map(|(i, id)| grade_of(id).map(|g| (i, g)))
        .collect();
    ranked_graded.sort_by_key(|(i, _)| *i);

    let coverage = if q.expected_conflicts.is_empty() {
        None
    } else {
        Some(q.expected_conflicts.iter().all(|id| ranked.iter().take(TOP_N).any(|r| r == id)))
    };

    let misses: Vec<&'static str> = q
        .judgments
        .iter()
        .filter(|j| !ranked.iter().take(TOP_N).any(|r| r == j.page_id))
        .map(|j| j.page_id)
        .collect();

    let mut m = Metrics::zero();

    if !q.judgments.is_empty() {
        let found = q.judgments.len().saturating_sub(misses.len());
        m.candidate_recall20 = found as f64 / q.judgments.len() as f64;
    }

    if let Some((first, _)) = ranked_graded.first() {
        m.mrr = 1.0 / (*first as f64 + 1.0);
    } else if !q.judgments.is_empty() {
        m.mrr = 0.0;
    }

    m.ndcg5 = if ranked_graded.is_empty() { 0.0 } else { ndcg(&ranked_graded, q.judgments, 5) };
    m.ndcg10 = if ranked_graded.is_empty() { 0.0 } else { ndcg(&ranked_graded, q.judgments, 10) };

    let canonicals: Vec<&Judgment> =
        q.judgments.iter().filter(|j| j.grade == 3 && j.role == Role::Canonical).collect();
    if !canonicals.is_empty() {
        m.canonical_at3 = if canonicals
            .iter()
            .any(|j| ranked.iter().take(3).any(|r| r == j.page_id))
        { 1.0 } else { 0.0 };
    }

    let evidence: Vec<&Judgment> = q.judgments.iter().filter(|j| j.role == Role::Evidence).collect();
    if !evidence.is_empty() {
        let found = evidence
            .iter()
            .filter(|j| ranked.iter().take(TOP_N).any(|r| r == j.page_id))
            .count();
        m.evidence_recall20 = found as f64 / evidence.len() as f64;
    }

    if let Some(cov) = coverage {
        m.contradiction_coverage = if cov { 1.0 } else { 0.0 };
    }

    if q.judgments.is_empty() {
        m.negative_empty_rate = if ranked.is_empty() { 1.0 } else { 0.0 };
    }

    let best = hit_highest(&ranked_graded, q.judgments).map(|(r, _)| r);
    (m, misses, best)
}

/// Sum metrics across every query; return (all, train, heldout) averages.
pub fn aggregate(runs: &[(&Query, &Metrics)]) -> (Metrics, Metrics, Metrics) {
    let mut all = Metrics::zero();
    let mut train = Metrics::zero();
    let mut held = Metrics::zero();
    let (mut na, mut nt, mut nh) = (0, 0, 0);
    for (q, m) in runs {
        all.add(m);
        na += 1;
        match q.split {
            Split::Train => {
                train.add(m);
                nt += 1;
            }
            Split::Heldout => {
                held.add(m);
                nh += 1;
            }
        }
    }
    (all.div(na), train.div(nt), held.div(nh))
}

pub fn row(m: &Metrics) -> String {
    format!(
        "R@20 {:.3} | MRR {:.3} | nDCG@5 {:.3} | nDCG@10 {:.3} | can@3 {:.3} | evR@20 {:.3} | ctrCov {:.3} | negEmpty {:.3}",
        m.candidate_recall20,
        m.mrr,
        m.ndcg5,
        m.ndcg10,
        m.canonical_at3,
        m.evidence_recall20,
        m.contradiction_coverage,
        m.negative_empty_rate
    )
}

fn assert_close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= EPS, "baseline drift in {what}: engine now {a}, baseline {b}");
}

pub fn check_metrics(name: &str, got: &Metrics, base: &Metrics) {
    assert_close(got.candidate_recall20, base.candidate_recall20, &format!("{name}.candidate_recall20"));
    assert_close(got.mrr, base.mrr, &format!("{name}.mrr"));
    assert_close(got.ndcg5, base.ndcg5, &format!("{name}.ndcg5"));
    assert_close(got.ndcg10, base.ndcg10, &format!("{name}.ndcg10"));
    assert_close(got.canonical_at3, base.canonical_at3, &format!("{name}.canonical_at3"));
    assert_close(got.evidence_recall20, base.evidence_recall20, &format!("{name}.evidence_recall20"));
    assert_close(got.contradiction_coverage, base.contradiction_coverage, &format!("{name}.contradiction_coverage"));
    assert_close(got.negative_empty_rate, base.negative_empty_rate, &format!("{name}.negative_empty_rate"));
}

/// Build the fixture vault (bootstrap + every page written).
pub fn build_vault() -> (tempfile::TempDir, rust_wiki::vault::VaultPaths) {
    use rust_wiki::vault::{bootstrap, VaultPaths};
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