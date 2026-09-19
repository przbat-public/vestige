#!/usr/bin/env bash
# Consistency gate for the MCPB bundle: manifest.json ↔ launch.sh ↔ build.sh.
#
# Why this exists: the bundle is assembled by hand (`./build.sh && mcpb pack`)
# and nothing checked that the three files agree. That is how the ARM64 Linux
# binary ended up downloaded but unreachable — `manifest.json` pointed at the
# x86_64 binary on every Linux host, so Linux ARM64 users got `exec format
# error` from a bundle that shipped the right binary.
#
# The checks are static (no network, no binaries), so they run in CI on every
# PR; `build.sh` calls the same script with --verify-server once the archives
# are on disk, which additionally asserts that every path the manifest points
# at really exists.
#
# Usage:
#   ./check-manifest.sh                  # static consistency (CI)
#   ./check-manifest.sh --verify-server  # + server/ must contain every path

set -euo pipefail

cd "$(dirname "$0")"

MANIFEST="manifest.json"
LAUNCHER="launch.sh"
BUILDER="build.sh"

VERIFY_SERVER=0
case "${1:-}" in
  "") ;;
  --verify-server) VERIFY_SERVER=1 ;;
  *) echo "usage: $0 [--verify-server]" >&2; exit 2 ;;
esac

fail() {
  echo "❌ $1" >&2
  exit 1
}

note() {
  echo "✅ $1"
}

for f in "$MANIFEST" "$LAUNCHER" "$BUILDER"; do
  [[ -f "$f" ]] || fail "missing ${f}"
done

# ----------------------------------------------------------------------------
# 1. Paths the manifest points at
# ----------------------------------------------------------------------------
# Every `"command"` / `"entry_point"` value, with the `${__dirname}/` prefix
# stripped so it is comparable with the paths build.sh writes.
manifest_paths="$(
  grep -oE '"(command|entry_point)"[[:space:]]*:[[:space:]]*"[^"]+"' "$MANIFEST" \
    | sed -E 's/^[^:]*:[[:space:]]*"//; s/"$//' \
    | sed -E 's#^\$\{[A-Za-z_]+\}/##' \
    | sort -u
)"
manifest_count="$(printf '%s\n' "$manifest_paths" | grep -c . || true)"
[[ "$manifest_count" -ge 2 ]] || fail \
  "parsed only ${manifest_count} path(s) out of ${MANIFEST}. Did the JSON shape change?"
note "${MANIFEST} points at ${manifest_count} in-bundle paths"

while IFS= read -r path; do
  [[ -n "$path" ]] || continue
  rel="${path#server/}"
  grep -qF "server/${rel}" "$BUILDER" || fail \
    "${MANIFEST} points at '${path}' but ${BUILDER} never writes it. Add the download/install step."
done <<< "$manifest_paths"
note "Every ${MANIFEST} path is produced by ${BUILDER}"

# ----------------------------------------------------------------------------
# 2. Binaries the bundle ships
# ----------------------------------------------------------------------------
# Destinations of `mv` / `install` inside build.sh — the files that end up in
# the packed bundle.
bundled="$(
  grep -oE '(mv|install)[^|;&]*server/[A-Za-z0-9._-]+' "$BUILDER" \
    | grep -oE 'server/[A-Za-z0-9._-]+$' \
    | sed 's#^server/##' \
    | sort -u
)"
bundled_servers="$(printf '%s\n' "$bundled" | grep -E '^vestige-mcp-' || true)"
[[ -n "$bundled_servers" ]] || fail "could not extract any vestige-mcp-* binary from ${BUILDER}"
note "${BUILDER} bundles $(printf '%s\n' "$bundled_servers" | grep -c .) vestige-mcp-* binaries"

# ----------------------------------------------------------------------------
# 3. Launcher targets
# ----------------------------------------------------------------------------
# Only `exe="$dir/..."` assignments count: the launcher's prose also mentions
# binary names, and matching those would make this check vacuous.
launcher_targets="$(grep -oE '\$dir/vestige-mcp-[A-Za-z0-9._-]+' "$LAUNCHER" | sed 's#^\$dir/##' | sort -u)"
launcher_count="$(printf '%s\n' "$launcher_targets" | grep -c . || true)"
[[ "$launcher_count" -ge 3 ]] || fail \
  "parsed only ${launcher_count} target(s) out of ${LAUNCHER} — did the dispatch table change shape?"
note "${LAUNCHER} dispatches to ${launcher_count} binaries"

while IFS= read -r target; do
  [[ -n "$target" ]] || continue
  printf '%s\n' "$bundled" | grep -qxF "$target" || fail \
    "${LAUNCHER} dispatches to '${target}' but ${BUILDER} never downloads it."
done <<< "$launcher_targets"
note "Every ${LAUNCHER} target is downloaded by ${BUILDER}"

# ----------------------------------------------------------------------------
# 4. Reachability: no bundled server binary may be dead weight
# ----------------------------------------------------------------------------
# The launcher only counts as a route when the manifest actually runs it —
# otherwise its whole dispatch table is unreachable and we are back to "the
# bundle ships a binary no host will ever pick".
launcher_installed="$(
  grep -oE 'install[^|;&]*server/[A-Za-z0-9._-]+' "$BUILDER" \
    | grep -oE 'server/[A-Za-z0-9._-]+$' | head -n 1 || true
)"
if [[ "$launcher_count" -gt 0 ]]; then
  [[ -n "$launcher_installed" ]] || fail \
    "${LAUNCHER} dispatches to binaries but ${BUILDER} never installs it into server/."
  printf '%s\n' "$manifest_paths" | grep -qxF "$launcher_installed" || fail \
    "${MANIFEST} never points at '${launcher_installed}', so the ${launcher_count} binaries \
${LAUNCHER} dispatches to are unreachable. Point mcp_config.command at it."
  note "${MANIFEST} runs ${LAUNCHER} (${launcher_installed})"
fi

while IFS= read -r bin; do
  [[ -n "$bin" ]] || continue
  if printf '%s\n' "$manifest_paths" | grep -qxF "server/${bin}"; then
    continue
  fi
  if printf '%s\n' "$launcher_targets" | grep -qxF "$bin"; then
    continue
  fi
  fail "${BUILDER} downloads '${bin}' but neither ${MANIFEST} nor ${LAUNCHER} can ever select it. \
Teach ${LAUNCHER} about it or drop the download."
done <<< "$bundled_servers"
note "Every bundled server binary is reachable (manifest or launcher)"

# ----------------------------------------------------------------------------
# 5. platform_overrides keys must be platforms MCPB understands
# ----------------------------------------------------------------------------
# The spec keys this object by `darwin` / `win32` / `linux` only — there is no
# architecture dimension, which is exactly why launch.sh exists.
override_keys="$(
  awk '/"platform_overrides"/{inside=1; next} inside && /^[[:space:]]*}[[:space:]]*$/{exit} inside{print}' "$MANIFEST" \
    | grep -oE '"[A-Za-z0-9_]+"[[:space:]]*:[[:space:]]*\{' \
    | sed -E 's/^"([A-Za-z0-9_]+)".*/\1/' || true
)"
while IFS= read -r key; do
  [[ -n "$key" ]] || continue
  case "$key" in
    darwin | win32 | linux) ;;
    *) fail "${MANIFEST} has platform_overrides key '${key}'; MCPB only knows darwin/win32/linux. \
Architecture-specific selection belongs in ${LAUNCHER}." ;;
  esac
done <<< "$override_keys"
if [[ -n "$override_keys" ]]; then
  note "platform_overrides keys are MCPB platform names only"
fi

# ----------------------------------------------------------------------------
# 6. Optional: the bundle being packed really contains those files
# ----------------------------------------------------------------------------
if [[ "$VERIFY_SERVER" -eq 1 ]]; then
  while IFS= read -r path; do
    [[ -n "$path" ]] || continue
    [[ -f "$path" ]] || fail "missing from the bundle: ${path}"
    [[ -x "$path" ]] || fail "not executable: ${path}"
  done <<< "$manifest_paths"
  note "server/ contains every path ${MANIFEST} points at"
fi

echo
echo "MCPB manifest checks passed."
