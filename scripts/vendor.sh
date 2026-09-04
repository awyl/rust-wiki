#!/usr/bin/env bash
# Re-sync skills/ from upstream llm-wiki-skills. Usage: scripts/vendor.sh [ref]
set -euo pipefail
REPO="geronimo-iia/llm-wiki-skills"
REF="${1:-main}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL "https://github.com/${REPO}/archive/${REF}.tar.gz" | tar -xz -C "$TMP"
SRC="$(echo "$TMP"/llm-wiki-skills-*/skills)"
rm -rf skills && mkdir -p skills
cp -R "$SRC"/. skills/
echo "Vendored ${REPO}@${REF} -> skills/ ($(ls skills | wc -l) skills)"
