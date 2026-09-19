#!/usr/bin/env bash
# Build a self-contained tarball for a colleague: binaries, setup helper,
# and the minimum docs they need. The archive is run-in-place — no copy
# to /usr/local/bin, no sudo. The colleague unpacks, runs setup.sh to
# get a ready-to-paste MCP config, and wires it into their IDE.
#
# Usage: ./scripts/pack-for-team.sh
# Output: vestige-<version>-<platform>.tar.gz in the repo root.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${ROOT}"

VERSION="$(awk -F'"' '/^\[workspace\.package\]/{f=1} f && /^version[[:space:]]*=/{print $2; exit}' Cargo.toml)"

# Detect host platform for the archive name
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  TARGET="darwin-arm64" ;;
  Darwin-x86_64) TARGET="darwin-x86_64" ;;
  Linux-x86_64)  TARGET="linux-x86_64" ;;
  Linux-aarch64) TARGET="linux-arm64" ;;
  *) echo "Unsupported host: $(uname -s)-$(uname -m)"; exit 1 ;;
esac

DIST_ROOT="dist/vestige-${VERSION}-${TARGET}"
ARCHIVE="vestige-${VERSION}-${TARGET}.tar.gz"

echo "==> Building dashboard (embedded into the binary via include_dir!)"
(cd apps/dashboard && pnpm install --ignore-scripts && pnpm build)

echo "==> Building Rust release (this takes a few minutes — LTO + codegen-units=1)"
cargo build --release -p vestige-mcp
cargo build --release -p vestige-restore

echo "==> Assembling ${DIST_ROOT}/"
rm -rf "${DIST_ROOT}"
mkdir -p "${DIST_ROOT}/bin" "${DIST_ROOT}/docs"
cp target/release/vestige-mcp     "${DIST_ROOT}/bin/"
cp target/release/vestige         "${DIST_ROOT}/bin/"
cp target/release/vestige-restore "${DIST_ROOT}/bin/"
cp README.md AGENTS.md SECURITY.md LICENSE "${DIST_ROOT}/"
cp docs/CONFIGURATION.md docs/STORAGE.md docs/SCIENCE.md "${DIST_ROOT}/docs/"

cat > "${DIST_ROOT}/setup.sh" <<'SETUP'
#!/usr/bin/env bash
# Prepares Vestige for HTTP-mode operation:
#   1. Ensures a Bearer auth token exists in the platform data dir.
#   2. Prints ready-to-paste MCP configs (with token baked in) for
#      Cursor, Claude Code, Claude Desktop.
#   3. Tells the user how to run the server.
#
# No copying, no sudo, no daemons. The server runs in the foreground of
# whichever terminal you start it in. HTTP MCP (port 3928) and the 3D
# dashboard (port 3927) come up automatically.

set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="${HERE}/bin/vestige-mcp"

if [[ ! -x "${BIN}" ]]; then
  echo "ERROR: ${BIN} not found or not executable." >&2
  exit 1
fi

# Platform-specific data directory (must match what vestige-mcp uses —
# see crates/vestige-mcp/src/protocol/auth.rs)
case "$(uname -s)" in
  Darwin) DATA_DIR="${HOME}/Library/Application Support/com.vestige.core" ;;
  Linux)  DATA_DIR="${XDG_DATA_HOME:-${HOME}/.local/share}/core" ;;
  *)      DATA_DIR="${HOME}/.vestige" ;;
esac

TOKEN_PATH="${DATA_DIR}/auth_token"

# Ensure data dir exists with owner-only permissions
mkdir -p "${DATA_DIR}"
chmod 700 "${DATA_DIR}" 2>/dev/null || true

# Read or generate the Bearer token
if [[ -s "${TOKEN_PATH}" ]]; then
  TOKEN="$(tr -d '[:space:]' < "${TOKEN_PATH}")"
  TOKEN_STATUS="existing token reused"
else
  # Prefer uuidgen (always present on macOS, common on Linux); fall back
  # to /dev/urandom hex if not available.
  if command -v uuidgen >/dev/null 2>&1; then
    TOKEN="$(uuidgen | tr '[:upper:]' '[:lower:]')"
  else
    TOKEN="$(head -c 32 /dev/urandom | xxd -p -c 32)"
  fi
  # Atomic write with restrictive permissions from creation
  umask 077
  printf '%s' "${TOKEN}" > "${TOKEN_PATH}"
  chmod 600 "${TOKEN_PATH}"
  TOKEN_STATUS="new token generated"
fi

# Detect macOS quarantine and warn
if [[ "$(uname -s)" == "Darwin" ]]; then
  if xattr "${BIN}" 2>/dev/null | grep -q "com.apple.quarantine"; then
    echo "⚠  macOS quarantine flag detected on the binary."
    echo "   Clear it once with:"
    echo "     xattr -dr com.apple.quarantine '${HERE}'"
    echo ""
  fi
fi

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "  Vestige is ready."
echo ""
echo "  Server binary:  ${BIN}"
echo "  Auth token:     ${TOKEN_PATH}  (${TOKEN_STATUS})"
echo "  HTTP MCP:       http://127.0.0.1:3928/mcp"
echo "  Dashboard:      http://127.0.0.1:3927/dashboard"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "STEP 1 — Start the server (keep this terminal open):"
echo ""
echo "    ${BIN}"
echo ""
echo "STEP 2 — Paste one of the configs below into your IDE and"
echo "         restart it (Cmd+Q on Cursor, not just close window)."
echo ""

cat <<EOF
─── Cursor ────────────────────────────────────────────────────────

Edit  ~/.cursor/mcp.json :

{
  "mcpServers": {
    "vestige": {
      "url": "http://127.0.0.1:3928/mcp",
      "headers": {
        "Authorization": "Bearer ${TOKEN}"
      }
    }
  }
}

─── Claude Desktop ────────────────────────────────────────────────

Edit  ~/Library/Application Support/Claude/claude_desktop_config.json
(macOS) or  %APPDATA%\Claude\claude_desktop_config.json  (Windows):

{
  "mcpServers": {
    "vestige": {
      "url": "http://127.0.0.1:3928/mcp",
      "headers": {
        "Authorization": "Bearer ${TOKEN}"
      }
    }
  }
}

─── Claude Code (CLI) ─────────────────────────────────────────────

Run once:

  claude mcp add --transport http vestige http://127.0.0.1:3928/mcp \\
    --header "Authorization: Bearer ${TOKEN}"

═══════════════════════════════════════════════════════════════════

EOF

# Write the Cursor snippet to disk so the user can copy without
# escaping the JSON
SNIPPET="${HERE}/mcp-cursor-snippet.json"
cat > "${SNIPPET}" <<EOF
{
  "mcpServers": {
    "vestige": {
      "url": "http://127.0.0.1:3928/mcp",
      "headers": {
        "Authorization": "Bearer ${TOKEN}"
      }
    }
  }
}
EOF
chmod 600 "${SNIPPET}"

echo "Cursor snippet (with token) written to:"
echo "  ${SNIPPET}"
echo ""
echo "First server start downloads ~1.3 GB of models from Hugging Face."
echo "Behind a corporate proxy, export HTTPS_PROXY first."
echo ""
echo "Database lives at:"
echo "  ${DATA_DIR}/vestige.db"
echo ""
echo "Want a per-project DB? Start the server with:"
echo "  ${BIN} --data-dir /absolute/path/to/.vestige"
echo ""
SETUP
chmod +x "${DIST_ROOT}/setup.sh"

cat > "${DIST_ROOT}/README-TEAM.md" <<TEAM
# Vestige ${VERSION} — internal share

Cognitive memory MCP server. **You will not get anyone else's
memories** — the database is created empty on your machine the first
time the server runs. Run in place from this folder; no install, no
sudo, no daemon.

## How it works

\`vestige-mcp\` is a single process that, when started, exposes three
things at the same time:

| Endpoint | Port | What it is |
|----------|------|------------|
| Stdio MCP | n/a | subprocess pipe (for IDEs that spawn the binary themselves) |
| **HTTP MCP** | **3928** | **\`POST http://127.0.0.1:3928/mcp\` — what your IDE connects to** |
| Dashboard | 3927 | \`http://127.0.0.1:3927/dashboard\` — the 3D memory graph |

We use the **HTTP transport** because it lets every IDE on your laptop
(Cursor, Claude Code, Claude Desktop, …) share one server process and
one model load (~1.3 GB in RAM). It also gives you the dashboard for
free.

## 60-second setup

\`\`\`bash
tar -xzf vestige-${VERSION}-${TARGET}.tar.gz
cd vestige-${VERSION}-${TARGET}
./setup.sh
\`\`\`

\`setup.sh\` does two things:

1. Reads (or generates) your Bearer auth token in the platform data
   directory. The token survives reboots and IDE restarts — generated
   once, used forever.
2. Prints ready-to-paste MCP configs for Cursor / Claude Code /
   Claude Desktop, with the URL and Bearer token already filled in.
   The Cursor JSON is also written to \`mcp-cursor-snippet.json\` next
   to this README.

Then start the server in a terminal of your choice:

\`\`\`bash
./bin/vestige-mcp
\`\`\`

Keep that terminal open. Stop with \`Ctrl+C\` (the WAL gets a clean
checkpoint on shutdown — your data is safe).

## Cursor — step by step

1. Run \`./setup.sh\` — note the Bearer token it prints.
2. Open \`~/.cursor/mcp.json\` and paste the printed snippet, or copy
   the JSON from \`mcp-cursor-snippet.json\` directly into it.
3. Fully quit Cursor (\`Cmd+Q\`) and reopen.
4. Start the server: \`./bin/vestige-mcp\` (in any terminal).
5. In Cursor chat ask: *"what MCP tools do you have access to?"* —
   you should see \`search\`, \`smart_ingest\`, \`memory\`, …

## Where the Bearer token comes from

\`setup.sh\` writes (or reads) the token at the same path
\`vestige-mcp\` uses natively — so server, IDE config, and CLI all
agree:

| Platform | Token file |
|----------|------------|
| macOS | \`~/Library/Application Support/com.vestige.core/auth_token\` |
| Linux | \`~/.local/share/core/auth_token\` |

File permissions are \`0600\` (owner read/write only). The directory is
\`0700\`. If you want to rotate the token, delete the file, re-run
\`./setup.sh\`, and update your IDE config with the new value.

To override entirely, export \`VESTIGE_AUTH_TOKEN\` before starting the
server — that wins over the file.

## First server start

Downloads two models from Hugging Face:

| Model | Size | Purpose |
|-------|------|---------|
| Nomic Embed Text v1.5 | ~130 MB | Embeddings |
| Jina Reranker v2 Base Multilingual | ~1.2 GB | Cross-encoder rerank |

Cache:

- macOS: \`~/Library/Caches/vestige.vestige/fastembed\`
- Linux: \`~/.cache/vestige/fastembed\`

Behind a corporate egress proxy? \`export HTTPS_PROXY=...\` before
starting the server.

## Your data lives here

| Platform | Path |
|----------|------|
| macOS | \`~/Library/Application Support/com.vestige.core/vestige.db\` |
| Linux | \`~/.local/share/core/vestige.db\` |

Permissions \`0700\`/\`0600\`. Owner-only. **Nothing is shared.**

Per-project DB instead of global? Start the server with
\`./bin/vestige-mcp --data-dir /absolute/path/to/.vestige\`.

## Dashboard

Once \`vestige-mcp\` is running, open
\`http://127.0.0.1:3927/dashboard\` in your browser. Live 3D graph,
real-time WebSocket events, FSRS retention curves, dream visualisation.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| "Connection refused" in Cursor | The server isn't running. Start \`./bin/vestige-mcp\` in a terminal. |
| "401 Unauthorized" in Cursor | Token mismatch. Re-run \`./setup.sh\` and re-copy the snippet — the token in your IDE config must match the one in \`auth_token\`. |
| "killed: 9" on macOS first run | macOS quarantine. Run \`xattr -dr com.apple.quarantine .\` from this folder. |
| Server hangs on first call (>1 min) | Model download in progress. Watch the \`fastembed\` cache directory grow; check \`HTTPS_PROXY\`. |
| Port 3928 already in use | Start with \`./bin/vestige-mcp --http-port 4928\` and update the URL in your IDE config. |
| Want to start fresh | Delete \`vestige.db\` from the path above. Next run will recreate. |

## Full docs

\`README.md\`, \`AGENTS.md\`, \`docs/CONFIGURATION.md\`,
\`docs/STORAGE.md\`, \`docs/SCIENCE.md\` — all in this archive.

## Questions

Ping Przemek.
TEAM

echo "==> Packing ${ARCHIVE}"
rm -f "${ARCHIVE}"
(cd dist && tar -czf "../${ARCHIVE}" "vestige-${VERSION}-${TARGET}")

BINARY_SIZE="$(du -h "${ARCHIVE}" | cut -f1)"

# ---------------------------------------------------------------------------
# Source snapshot — everything not gitignored (tracked + untracked).
# Used by reviewers who want to read / build the code themselves; the
# binary tarball above is enough for runtime users.
# ---------------------------------------------------------------------------
SRC_ROOT="dist/vestige-${VERSION}-source"
SRC_ARCHIVE="vestige-${VERSION}-source.tar.gz"

echo "==> Staging source snapshot (working tree minus .gitignore)"
rm -rf "${SRC_ROOT}"
mkdir -p "${SRC_ROOT}"

if ! command -v rsync >/dev/null 2>&1; then
  echo "ERROR: rsync not found — required for source packaging" >&2
  exit 1
fi

# `git ls-files --cached --others --exclude-standard` enumerates tracked +
# untracked files while honouring .gitignore. We then strip three
# categories of files that shouldn't ride along in a "for review" source
# snapshot:
#   - the runtime tarball we just produced (lives in repo root, untracked)
#   - upstream prebuilt binaries dropped into packages/vestige-mcp-npm/bin/
#     by an old npm postinstall script (~64 MB of stale Mar-2026 binaries)
#   - benchmarks/locomo/{retrieval_results,locomo_scores}*.json — large
#     evaluation outputs that regenerate on every benchmark run
# The existence filter at the end handles dashboard build files whose hash
# changed during this build (the cached git index still names the old ones).
git ls-files --cached --others --exclude-standard \
  | grep -vE '^vestige-[0-9].*\.tar\.gz$' \
  | grep -vE '^packages/vestige-mcp-npm/bin/' \
  | grep -vE '^benchmarks/.*(retrieval_results|locomo_scores).*\.json$' \
  | while IFS= read -r f; do [[ -e "$f" ]] && printf '%s\n' "$f"; done \
  | rsync -a --files-from=- ./ "${SRC_ROOT}/"

# Drop a top-level README so the recipient knows what they're holding
cat > "${SRC_ROOT}/SOURCE-README.md" <<SOURCEREADME
# Vestige ${VERSION} — source snapshot

Working-tree snapshot taken on $(date -u +"%Y-%m-%d %H:%M:%S UTC").
This is **not** a clean release tarball — it contains every file in
the repository working tree that is not \`.gitignore\`-d, including any
uncommitted changes the maintainer had at the time of packaging.

## What's in here

- \`crates/vestige-core/\`, \`crates/vestige-mcp/\`, \`crates/vestige-restore/\`
  — the Rust workspace.
- \`apps/dashboard/\` — React 19 + Vite 6 + Three.js + React Router 7
  dashboard, built into the binary via \`include_dir!\`.
- \`tests/e2e/\`, \`benchmarks/locomo/\` — auxiliary crates.
- \`packages/\` — npm wrapper + .mcpb bundle scaffolding.
- \`docs/\`, \`scripts/\`, \`.github/\` — docs, helper scripts, CI workflows.

## Just want to run it?

Use the binary tarball that ships next to this archive:
\`vestige-${VERSION}-${TARGET}.tar.gz\`. Unpack, run \`./setup.sh\`, follow
the printed instructions. No build step needed.

## Want to build from source?

Read \`CONTRIBUTING.md\` at the top of this archive. TL;DR:

\`\`\`bash
# Prereqs: Rust 1.91+, Node 22+, pnpm 9+
(cd apps/dashboard && pnpm install && pnpm build)
cargo build --release -p vestige-mcp
cargo build --release -p vestige-restore
\`\`\`

Then the binaries land in \`target/release/{vestige-mcp,vestige,vestige-restore}\`.

## Architecture entry points

- \`AGENTS.md\` — canonical operational instructions for AI coding agents
  working in this repo.
- \`ARCHITECTURE.md\` — system topology, search pipeline, REST routes,
  cognitive engine layout.
- \`docs/SCIENCE.md\` — neuroscience references behind every algorithm.
- \`README.md\` — public-facing overview.

## License

AGPL-3.0-only. See \`LICENSE\`.
SOURCEREADME

echo "==> Packing ${SRC_ARCHIVE}"
rm -f "${SRC_ARCHIVE}"
(cd dist && tar -czf "../${SRC_ARCHIVE}" "vestige-${VERSION}-source")

SRC_SIZE="$(du -h "${SRC_ARCHIVE}" | cut -f1)"
FILE_COUNT="$(find "${SRC_ROOT}" -type f | wc -l | tr -d ' ')"

echo ""
echo "════════════════════════════════════════════════════════════════"
echo "  Done. Two archives ready:"
echo ""
printf "    %-44s  %s\n" "${ARCHIVE}" "(${BINARY_SIZE})  — runtime"
printf "    %-44s  %s\n" "${SRC_ARCHIVE}" "(${SRC_SIZE})  — ${FILE_COUNT} files, source"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "Send both to the team. Runtime users only need the first one;"
echo "reviewers / would-be contributors take the second too."
