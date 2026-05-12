#!/usr/bin/env bash
# Quick dependency audit for the Vestige workspace (c23).
#
# Surface duplicates, audit the lockfile, and report the heaviest crates.
# Run locally before bumping major dependencies (fastembed, tokenizers, axum).
# The CI workflow stays green even if duplicates exist because they almost
# always come from third-party crates we don't control.
#
# Usage:
#   ./scripts/audit-deps.sh            # full audit
#   ./scripts/audit-deps.sh --quick    # duplicates only

set -euo pipefail

cd "$(dirname "$0")/.."

YELLOW=$'\033[1;33m'
GREEN=$'\033[1;32m'
RED=$'\033[1;31m'
RESET=$'\033[0m'

QUICK=false
if [[ "${1:-}" == "--quick" ]]; then
  QUICK=true
fi

echo "${YELLOW}=== Duplicate crate versions ===${RESET}"
DUPES=$(cargo tree --workspace --duplicates 2>&1 | grep -E "^[a-z0-9_-]+ v[0-9]" | sort -u || true)
if [[ -z "$DUPES" ]]; then
  echo "${GREEN}No duplicates.${RESET}"
else
  echo "$DUPES"
  echo
  echo "Most of these come from upstream crates (fastembed, tokenizers, hf-hub)"
  echo "and can only be resolved by bumping those crates."
fi

if [[ "$QUICK" == "true" ]]; then
  exit 0
fi

echo
echo "${YELLOW}=== Unsafe code scan ===${RESET}"
if command -v cargo-geiger >/dev/null 2>&1; then
  cargo geiger --quiet --workspace 2>&1 | tail -20 || true
else
  echo "(skip — install with: cargo install cargo-geiger)"
fi

echo
echo "${YELLOW}=== Outdated dependencies ===${RESET}"
if command -v cargo-outdated >/dev/null 2>&1; then
  cargo outdated --workspace --depth 1 2>&1 | head -30 || true
else
  echo "(skip — install with: cargo install cargo-outdated)"
fi

echo
echo "${YELLOW}=== Audit advisories ===${RESET}"
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit --quiet 2>&1 | tail -20 || true
else
  echo "(skip — install with: cargo install cargo-audit)"
fi

echo
echo "${YELLOW}=== Top 10 dependency tree size ===${RESET}"
cargo tree --workspace --prefix none --no-dedupe 2>&1 \
  | awk '{print $1}' \
  | sort | uniq -c \
  | sort -rn \
  | head -10

echo
echo "${GREEN}Audit complete.${RESET}"
