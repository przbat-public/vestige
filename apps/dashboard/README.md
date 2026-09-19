# Vestige Dashboard

React 19 + Vite 6 dashboard for the Vestige memory server. The production
bundle is committed under `build/` and embedded into the `vestige-mcp` binary
at compile time (`crates/vestige-mcp/src/dashboard/static_files.rs`), so a
rebuilt bundle must be committed alongside any source change — CI fails on
drift.

## Commands

| Command | What it does |
| --- | --- |
| `pnpm dev` | Vite dev server on :5173; proxies `/api` and `/ws` to a Vestige instance on :3927 |
| `pnpm build` | Type-check and emit `build/` (commit the result) |
| `pnpm check` | `tsc --noEmit` |
| `pnpm lint` | Biome |
| `pnpm test` | Vitest unit tests |
| `pnpm ci` | All three gates, the same sequence CI runs |
| `pnpm test:e2e` | Playwright end-to-end suite (see below) |

## End-to-end suite

`e2e/live-materialization.spec.ts` drives the real dashboard against the real
server: page rendering, the memory CRUD flows, keyboard navigation on the graph
page, and the WebSocket round trip that makes a memory written by an external
MCP client appear in an already-open page.

The suite is self-contained. `playwright.config.ts` starts `vestige-mcp`
through `e2e/start-server.mjs`, which:

- uses ports **4327** (dashboard) and **4328** (MCP HTTP) instead of the
  defaults, so a Vestige instance you already have running is never touched —
  the launcher refuses to start when either port is in use;
- creates a fresh temporary data directory per run and deletes it on exit, so
  the suite cannot write to your own database;
- passes the bearer token through `VESTIGE_AUTH_TOKEN` (no token file is read);
- sets `VESTIGE_TEST_MOCK_EMBEDDINGS=1` and points the model cache at an
  uncreatable path, so startup never downloads the ~547 MB embedding model or
  the ~1.11 GB cross-encoder reranker. Set `VESTIGE_E2E_ALLOW_MODEL_DOWNLOAD=1`
  to run against the real models.

Prerequisites, both failing loudly when missing:

```sh
cargo build -p vestige-mcp                        # or --release
cd apps/dashboard && pnpm exec playwright install chromium
```

Run it:

```sh
cd apps/dashboard && pnpm test:e2e
```

Useful overrides:

| Variable | Default | Purpose |
| --- | --- | --- |
| `VESTIGE_E2E_BIN` | `target/debug/vestige-mcp`, else `target/release/…` | Binary under test (relative paths resolve against the repository root) |
| `VESTIGE_E2E_DASHBOARD_PORT` / `VESTIGE_E2E_MCP_PORT` | `4327` / `4328` | Ports for the spawned server |
| `VESTIGE_E2E_TOKEN` | a fixed local-only token | Bearer token for the MCP transport |
| `VESTIGE_E2E_KEEP_DATA=1` | unset | Keep the temporary data directory for post-mortem inspection |
| `VESTIGE_E2E_ALLOW_MODEL_DOWNLOAD=1` | unset | Let the server fetch the real ONNX models |

Every test asserts an observable outcome. Failures keep a trace, a video and a
screenshot under `dist/test-results/`, plus the HTML report in
`dist/playwright-report/` (`dist/` is ignored by Biome and git, so a local
suite run never leaves the lint gate or `git status` dirty); CI uploads both as
the `playwright-report` artifact when the `Dashboard E2E (Playwright)` job
fails.
