#!/usr/bin/env bash
# CI gate for the wire-DTO type pipeline.
#
# Workflow this enforces:
#   1. Edit a Rust DTO in `crates/vestige-mcp/src/dashboard/wire/`.
#   2. Run `cargo test -p vestige-mcp --lib dashboard` locally.
#   3. Commit the regenerated `apps/dashboard/src/types/generated/*.ts`
#      alongside the Rust change.
#
# This script verifies step 3 by regenerating from a clean checkout and
# diffing against committed files. Drift = the contributor edited Rust
# but forgot to commit the new TypeScript, which we cannot reliably
# detect any other way.
#
# Run locally before pushing or rely on the CI check that calls this
# script.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "→ regenerating ts-rs declarations…"
# `--lib dashboard` covers both `dashboard::wire::*` and
# `dashboard::events` exports. `--quiet` keeps the CI log tidy; we'll
# print our own status if anything fails.
cargo test -p vestige-mcp --lib dashboard --quiet

echo "→ checking for drift in apps/dashboard/src/types/generated/…"

GENERATED_DIR="apps/dashboard/src/types/generated/"

# `git diff` only reports tracked files, so a brand-new DTO declaration that the
# test emitted but nobody committed would slip through this gate silently.
untracked="$(git ls-files --others --exclude-standard -- "$GENERATED_DIR")"
if [[ -n "$untracked" ]]; then
    {
        echo ""
        echo "✗ Generated TypeScript declarations are untracked."
        echo ""
        echo "  The dashboard test emitted files under ${GENERATED_DIR}"
        echo "  that were never committed:"
        echo ""
        echo "$untracked" | sed 's/^/    /'
        echo ""
        echo "  Fix locally:"
        echo "    git add ${GENERATED_DIR}"
        echo ""
    } >&2
    exit 1
fi

if ! git diff --exit-code --stat -- "$GENERATED_DIR"; then
    cat <<'EOF' >&2

✗ Generated TypeScript declarations are out of date.

  The Rust wire DTOs in crates/vestige-mcp/src/dashboard/wire/ changed
  but the regenerated apps/dashboard/src/types/generated/*.ts files
  weren't committed.

  Fix locally:
    cargo test -p vestige-mcp --lib dashboard
    git add apps/dashboard/src/types/generated/
    git commit --amend --no-edit  # or a fresh commit, whatever fits

EOF
    exit 1
fi

echo "✓ generated types are in sync with their Rust sources."
