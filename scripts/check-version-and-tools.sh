#!/usr/bin/env bash
# Verifies version metadata is consistent across the workspace, and that the
# documented MCP tool count matches what `server.rs` actually exposes.
#
# Designed to run in CI on every PR (`./scripts/check-version-and-tools.sh`).
# Exits non-zero with a human-readable diff on mismatch.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "❌ $1" >&2
  exit 1
}

note() {
  echo "✅ $1"
}

# ----------------------------------------------------------------------------
# 1. Version inheritance
# ----------------------------------------------------------------------------
WS_VERSION="$(awk -F'"' '/^\[workspace\.package\]/{found=1} found && /^version[[:space:]]*=/{print $2; exit}' Cargo.toml)"
if [[ -z "${WS_VERSION:-}" ]]; then
  fail "Could not read [workspace.package].version from Cargo.toml"
fi
note "Workspace version: ${WS_VERSION}"

PKG_JSON_VERSION="$(awk -F'"' '/"version"/{print $4; exit}' package.json)"
[[ "$PKG_JSON_VERSION" == "$WS_VERSION" ]] || fail \
  "package.json version (${PKG_JSON_VERSION}) ≠ workspace version (${WS_VERSION})"
note "package.json version matches"

DASHBOARD_VERSION="$(awk -F'"' '/"version"/{print $4; exit}' apps/dashboard/package.json)"
[[ "$DASHBOARD_VERSION" == "$WS_VERSION" ]] || fail \
  "apps/dashboard/package.json version (${DASHBOARD_VERSION}) ≠ workspace version (${WS_VERSION})"
note "apps/dashboard/package.json version matches"

# Crates MUST inherit from workspace (they should NOT carry their own version).
for crate in crates/vestige-core crates/vestige-mcp crates/vestige-restore; do
  if grep -qE '^version[[:space:]]*=[[:space:]]*"' "$crate/Cargo.toml"; then
    fail "$crate/Cargo.toml has a hardcoded version. Use 'version.workspace = true' instead."
  fi
done
note "All crates inherit version from workspace"

# ----------------------------------------------------------------------------
# 2. Tool count (advertised in tools/list)
# ----------------------------------------------------------------------------
# Counts the ToolDescription entries in build_tools_list — the canonical list
# of MCP tools exposed to clients (post-b15 split, lives in server/catalog.rs).
# Each entry is constructed via the `tool(name, title, ...)` helper, so we
# count occurrences of `tool(` at the start of a (whitespace-indented) line
# inside the function. This survives both the legacy literal-struct format
# and the post-Wave-6 helper-based format.
TOOL_COUNT="$(awk '
  /fn build_tools_list/{capture=1; next}
  capture && /^[[:space:]]+tool\(/{count++}
  capture && /^\}[[:space:]]*$/ && count > 0 {print count; exit}
' crates/vestige-mcp/src/server/catalog.rs)"

if [[ -z "${TOOL_COUNT:-}" || "$TOOL_COUNT" -eq 0 ]]; then
  fail "Could not extract tool count from server.rs"
fi
note "server.rs exposes ${TOOL_COUNT} tools"

EXPECTED_TOOL_COUNT=28
[[ "$TOOL_COUNT" == "$EXPECTED_TOOL_COUNT" ]] || fail \
  "server.rs exposes ${TOOL_COUNT} tools but the docs claim ${EXPECTED_TOOL_COUNT}. Update either."

# README must mention the same number.
if ! grep -qE "${EXPECTED_TOOL_COUNT}[[:space:]]+(MCP[[:space:]]+)?[Tt]ools" README.md; then
  fail "README.md does not advertise ${EXPECTED_TOOL_COUNT} tools"
fi
note "README.md advertises ${EXPECTED_TOOL_COUNT} tools"

# ARCHITECTURE must mention the same number.
if ! grep -qE "${EXPECTED_TOOL_COUNT}[[:space:]]+(MCP[[:space:]]+)?[Tt]ools" ARCHITECTURE.md; then
  fail "ARCHITECTURE.md does not advertise ${EXPECTED_TOOL_COUNT} tools"
fi
note "ARCHITECTURE.md advertises ${EXPECTED_TOOL_COUNT} tools"

# ----------------------------------------------------------------------------
# 3. License consistency
# ----------------------------------------------------------------------------
# All packages that declare a license must declare the SAME license string as
# the workspace. The MCPB manifest historically drifted to "MIT" even though
# the codebase is AGPL-3.0-only; this check stops that from happening again.
WS_LICENSE="$(awk -F'"' '/^license[[:space:]]*=/{print $2; exit}' Cargo.toml)"
[[ -n "${WS_LICENSE:-}" ]] || fail "Could not read [workspace.package].license from Cargo.toml"
note "Workspace license: ${WS_LICENSE}"

declare -a license_files=(
  "packages/vestige-init/package.json"
  "packages/vestige-mcp-npm/package.json"
  "packages/vestige-mcpb/manifest.json"
)
for f in "${license_files[@]}"; do
  pkg_license="$(awk -F'"' '/"license"/{print $4; exit}' "$f")"
  [[ "$pkg_license" == "$WS_LICENSE" ]] || fail \
    "$f license ($pkg_license) ≠ workspace license ($WS_LICENSE)"
done
note "All package manifests match workspace license"

# ----------------------------------------------------------------------------
# 4. Rust toolchain
# ----------------------------------------------------------------------------
if [[ -f rust-toolchain.toml ]]; then
  TOOLCHAIN_CHANNEL="$(awk -F'"' '/^channel/{print $2; exit}' rust-toolchain.toml)"
  RUST_VERSION="$(awk -F'"' '/^\[workspace\.package\]/{found=1} found && /^rust-version[[:space:]]*=/{print $2; exit}' Cargo.toml)"
  if [[ -n "$RUST_VERSION" && "$TOOLCHAIN_CHANNEL" != "${RUST_VERSION}"* ]]; then
    fail "rust-toolchain.toml channel (${TOOLCHAIN_CHANNEL}) does not start with rust-version (${RUST_VERSION})"
  fi
  note "rust-toolchain.toml channel matches workspace rust-version"
fi

echo
echo "All metadata checks passed."
