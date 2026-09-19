# Contributing to Vestige

Thank you for your interest in contributing to Vestige. This guide covers everything you need to start.

## Project Overview

Vestige is a cognitive memory MCP server written in Rust. It gives AI agents persistent long-term memory using neuroscience-backed algorithms (FSRS-6, prediction error gating, synaptic tagging, spreading activation, memory dreaming).

**Architecture:**

```
vestige/
├── crates/
│   ├── vestige-core/      # Cognitive engine, FSRS-6, search, embeddings, storage
│   ├── vestige-mcp/       # MCP server, Axum dashboard, WebSocket, tool handlers
│   └── vestige-restore/   # Standalone restore binary (no fastembed/USearch deps)
├── apps/
│   └── dashboard/         # React 19 + Vite 6 + React Router 7 + Three.js
├── packages/
│   ├── vestige-init/      # npx vestige-init installer
│   ├── vestige-mcp-npm/   # npm binary wrapper
│   └── vestige-mcpb/      # .mcpb bundle for Claude Desktop
├── tests/
│   └── e2e/               # End-to-end MCP protocol tests (crate: vestige-e2e-tests)
└── benchmarks/
    └── locomo/            # LoCoMo benchmark harness (vestige-locomo-bench)
```

## Development Setup

### Prerequisites

- **Rust** 1.91+ stable: [rustup.rs](https://rustup.rs)
- **Node.js** 22+: [nodejs.org](https://nodejs.org)
- **pnpm** 9+: `npm install -g pnpm`

### Getting Started

```bash
git clone https://github.com/samvallad33/vestige.git
cd vestige

# Build the dashboard (required for include_dir! embedding)
cd apps/dashboard && pnpm install && pnpm build && cd ../..

# Build the Rust workspace
cargo build

# Run tests with mock embeddings (skips ONNX model download)
VESTIGE_TEST_MOCK_EMBEDDINGS=1 cargo test --workspace
```

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `VESTIGE_TEST_MOCK_EMBEDDINGS=1` | Use mock embeddings in tests (skips ONNX model download) |
| `VESTIGE_ENCRYPTION_KEY` | Enable SQLite encryption with the `encryption` Cargo feature |
| `VESTIGE_HTTP_BIND` / `VESTIGE_HTTP_PORT` | HTTP transport bind (default `127.0.0.1:3928`) |
| `VESTIGE_DASHBOARD_PORT` | Dashboard port (default `3927`) |
| `VESTIGE_AUTH_TOKEN` | Override the bearer token for the HTTP transport (auto-generated otherwise) |
| `VESTIGE_MAX_TOKEN_BUDGET` | Upper clamp for the `search` / `session_context` response token budget (default `100000`) |
| `VESTIGE_RETENTION_TARGET` | FSRS-6 retention target override (default `0.8`) |
| `VESTIGE_CONSOLIDATION_INTERVAL_HOURS` | Background consolidation cadence (default `6`) |
| `RUST_LOG` | Tracing filter (e.g. `vestige_mcp=debug,vestige_core=info`) |

Database location is set via the `--data-dir <PATH>` CLI flag, not an env var (see [`docs/STORAGE.md`](docs/STORAGE.md) for default platform paths).

## Running Tests

```bash
# Whole workspace (unit + lib + integration)
VESTIGE_TEST_MOCK_EMBEDDINGS=1 cargo test --workspace

# Core library tests only
VESTIGE_TEST_MOCK_EMBEDDINGS=1 cargo test -p vestige-core --lib

# MCP server tests only
VESTIGE_TEST_MOCK_EMBEDDINGS=1 cargo test -p vestige-mcp --lib

# NLP detector baselines (contradiction / opinion / future-relevance)
cargo test -p vestige-core --test nlp_baseline -- --nocapture

# E2E MCP protocol tests (require a release build)
cargo build --release -p vestige-mcp
cargo test -p vestige-e2e-tests --test mcp_protocol -- --test-threads=1

# Dashboard build + lint + vitest
cd apps/dashboard && pnpm ci
```

Exact pass counts drift release-to-release — `cargo test --workspace` is the canonical "did everything pass" gate. The metadata CI guard (`./scripts/check-version-and-tools.sh`) verifies that the advertised MCP tool count matches the catalog and that every package manifest agrees on the workspace version and licence.

## Building

```bash
# Debug build
cargo build -p vestige-mcp

# Release build (full features: embeddings + vector-search + preprocessing)
cargo build --release -p vestige-mcp

# Release with Apple Silicon Metal acceleration
cargo build --release -p vestige-mcp --features metal

# Encryption build (SQLCipher; mutually exclusive with bundled-sqlite)
cargo build --release -p vestige-mcp \
  --no-default-features \
  --features embeddings,vector-search,preprocessing,encryption

# Size-optimised dist profile (slower runtime, smaller binary)
cargo build --profile dist -p vestige-mcp
```

### Release Profile

The default release profile favours runtime speed on the hot path (search, embeddings, RRF):

- `opt-level = 3`
- `lto = true`
- `codegen-units = 1`
- `panic = "abort"`
- `strip = true`

When you specifically need the smallest possible binary (a curl-piped download, for example), use `--profile dist` which switches to `opt-level = "z"` and `lto = "fat"`.

## Code Style

### Rust

```bash
# Format
cargo fmt --all

# Lint (zero warnings policy)
cargo clippy --workspace --all-targets -- -D warnings
```

- Rust 2024 edition, MSRV 1.91.
- Standard `rustfmt` defaults.
- All public items should have doc comments.
- Tests go in `#[cfg(test)] mod tests` at the bottom of each file.
- Workspace lints in `Cargo.toml` flag `dbg!`, `todo!`, `unimplemented!`, and unjustified `pub(scope)` field visibility. Suppress with a `reason = "..."` annotation when intentional.

### TypeScript / React (Dashboard)

```bash
cd apps/dashboard
pnpm typecheck    # tsc --noEmit
pnpm lint         # biome check
pnpm test         # vitest
pnpm build        # vite build (outputs apps/dashboard/build)
pnpm ci           # all of the above
```

The dashboard's `apps/dashboard/build/` directory is embedded into the release binary via `include_dir!`, so every change in `apps/dashboard/src/` requires `pnpm build` before the Rust binary picks it up.

## Project Structure

### vestige-core

The cognitive engine. Key modules:

| Module | Purpose |
|--------|---------|
| `fsrs/` | FSRS-6 spaced repetition (21 parameters, power-law decay) |
| `neuroscience/` | Synaptic tagging, spreading activation, hippocampal index, importance signals, emotional memory, prospective memory |
| `advanced/` | Prediction error gating, dreaming (6 sub-modules), compression, cross-project learning, speculative retrieval |
| `nlp/` | Pluggable detectors: `ContradictionDetector`, `OpinionDetector`, `FutureRelevanceDetector` (heuristic defaults, ONNX-ready) |
| `preprocessing/` | Content Intelligence Pipeline — entity / coref / temporal / relation / provenance |
| `search/` | Hybrid search (BM25 + semantic), HyDE, Jina Reranker v2, temporal search, compound query decomposition |
| `embeddings/` | fastembed (Nomic Embed v1.5), ONNX inference, Matryoshka 768D → 384D truncation |
| `storage/sqlite/` | Per-concern split: nodes, states, history, intentions, maintenance, embeddings, review, consolidation, search, graph, gdpr, temporal, smart_ingest, insights, records, stats. Migrations v1–v16. |

### vestige-mcp

The MCP server and dashboard. Key modules:

| Module | Purpose |
|--------|---------|
| `protocol/` | stdio + HTTP MCP transports, JSON-RPC messages, auth, timeout |
| `server/catalog.rs` | Canonical list of MCP tools (28) and resources (11). CI guards drift. |
| `cognitive.rs` | `CognitiveEngine` wrapper |
| `tools/` | One file (or sub-module) per MCP tool |
| `dashboard/wire/` | ts-rs DTOs — the wire contract, single source of truth |
| `dashboard/handlers/` | Per-domain REST handlers (memory, search, graph, history, intentions, maintenance, review, cognitive, metacognitive, observability, decisions, hubs, insights, pages) |
| `dashboard/events.rs` | `VestigeEvent` discriminated union (also ts-rs) |
| `telemetry.rs` | OpenTelemetry/OTLP scaffolding behind the `telemetry` feature (no-op by default) |

### apps/dashboard

React 19 + Vite 6 + React Router 7 + Three.js + Tailwind CSS 4 + i18next.

Routes (`apps/dashboard/src/App.tsx`):

`/graph`, `/memories`, `/review`, `/briefing`, `/timeline`, `/feed`, `/explore`, `/reasoning`, `/decisions`, `/insights`, `/hubs`, `/intentions`, `/temporal`, `/stats`, `/settings`, `/tutorial`, plus the `*` 404 fallback.

## Pull Request Process

1. **Fork** the repository and create a feature branch from `main`.
2. **Write tests** for new functionality.
3. **Ensure all checks pass**: `cargo fmt`, `cargo clippy`, `cargo test`, `pnpm ci`.
4. **Build the dashboard** if you modified `apps/dashboard/`.
5. **Keep commits focused**: one logical change per commit, [Conventional Commits](https://www.conventionalcommits.org/) prefixes (`feat`, `fix`, `refactor`, `chore`, `docs`, `style`, `ci`, `perf`, `test`).
6. **Open a PR** with a clear description.

### PR Checklist

- [ ] `cargo fmt --all -- --check` — code is formatted
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] `VESTIGE_TEST_MOCK_EMBEDDINGS=1 cargo test --workspace` — all tests pass
- [ ] `./scripts/check-version-and-tools.sh` — metadata drift gate passes
- [ ] `./scripts/check-generated-types.sh` — ts-rs ↔ TypeScript parity passes (if a `wire/` DTO changed)
- [ ] Dashboard builds (if modified): `cd apps/dashboard && pnpm ci`
- [ ] No secrets, API keys, or credentials in the diff

### Good First Issues

Look for issues labeled `good first issue`. Scoped tasks include:

- Adding tests for existing modules
- Documentation improvements
- Dashboard UI enhancements
- New MCP tool implementations

## Adding a New MCP Tool

1. Create `crates/vestige-mcp/src/tools/your_tool.rs`.
2. Implement `pub fn schema() -> serde_json::Value` and `pub async fn execute(...)`.
3. Register the entry in `crates/vestige-mcp/src/server/catalog.rs::build_tools_list` and wire dispatch in `server/dispatch.rs`.
4. Add tests in the same file (use the `helpers::test_storage` fixtures).
5. Update the expected tool count in `scripts/check-version-and-tools.sh` and the comment in `server/catalog.rs`.
6. Update the tool count in `README.md`, `ARCHITECTURE.md`, `crates/vestige-mcp/README.md`, and `AGENTS.md` (`CLAUDE.md` and `GEMINI.md` are symlinks to `AGENTS.md` — no separate edit needed).

## Adding a New Cognitive Module

1. Add the module under `crates/vestige-core/src/neuroscience/` or `advanced/`.
2. Add the field to `CognitiveEngine` in `crates/vestige-mcp/src/cognitive.rs`.
3. Initialise it in `CognitiveEngine::new()` and `new_with_events()`.
4. Hydrate persistent state from `Storage` if applicable.
5. Write tests (aim for 10+ per module), including a scientific validation case mapped to published research.
6. Document the citation in the module's doc comment.

## Adding a Type-Safe Dashboard Endpoint

Read [AGENTS.md → "Adding a Type-Safe Dashboard Endpoint"](AGENTS.md#adding-a-type-safe-dashboard-endpoint). The dashboard ↔ backend contract is enforced end-to-end via ts-rs + Zod + a CI gate; any new endpoint follows the same template.

## Issue Reporting

Use the issue templates:

- **Bug Report**: include OS, install method, IDE, vestige version (`vestige-mcp --version`), and steps to reproduce.
- **Feature Request**: describe the problem, proposed solution, and alternatives considered.

## Code of Conduct

We are committed to providing a welcoming and inclusive environment. All contributors are expected to be respectful, constructive, and collaborative. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

By contributing, you agree that your contributions will be licensed under **AGPL-3.0-only** ([LICENSE](LICENSE)), the same license as the project.

---

Questions? Open a [discussion](https://github.com/samvallad33/vestige/discussions) or reach out to the maintainers.
