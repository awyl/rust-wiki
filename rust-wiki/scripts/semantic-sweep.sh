#!/usr/bin/env bash
# Tune the three semantic knobs against the train split (held-out quarantine).
#
# Each constant combo is a separate cargo process with the env knobs set
# (config::get() reads env once per process), running the semantic benchmark's
# sweep mode, which prints one SWEEP line. Output sorted by train canonical@3,
# ties broken by junk rate.
#
# Usage: scripts/semantic-sweep.sh [out.tsv]
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-/tmp/semantic-sweep.tsv}"
: > "$OUT"

for cos in 0.1 0.4 0.6 0.8; do
  for scale in 3 6 10; do
    for weight in 0.3 0.5 0.7; do
      WIKI_RECALL_SEMANTIC_MIN_COSINE=$cos \
      WIKI_RECALL_SEMANTIC_SCALE=$scale \
      WIKI_RECALL_SEMANTIC_WEIGHT=$weight \
      BENCHMARK_SEMANTIC_SWEEP=1 \
      BENCHMARK_SWEEP_OUT="$OUT" \
        cargo test --test recall_benchmark_semantic --quiet >/dev/null 2>&1 || true
    done
  done
done

echo "ranked by train canonical@3 (desc), admissions (asc):"
sort -t$'\t' -k5,5nr -k8,8n "$OUT" | awk -F'\t' '{printf "cos=%s scale=%s weight=%s  can@3=%s evR=%s ctrCov=%s junk=%s adm=%s\n", $2,$3,$4,$5,$6,$7,$8,$9}'
echo "(all combos saved to $OUT)"