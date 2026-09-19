//! # User Journey E2E Tests
//!
//! ## What is in this directory
//!
//! **Every module here now has at least one real, SQLite-backed journey.** Each
//! one constructs a `Storage` through
//! `vestige_e2e_tests::harness::TestDatabaseManager`, drives the production
//! code path, and asserts state that survives a cold restart (drop the
//! `Storage`, build a fresh one from the same file — the boot path the server
//! runs).
//!
//! | Module | Real journey |
//! |---|---|
//! | `ingest_recall_review` | `smart_ingest` → `recall` (FTS5 + hybrid) → `mark_reviewed` → restart; recall's Testing-Effect boost must be on disk |
//! | `consolidation_workflow` | `Storage::run_consolidation` on aged memories: decay, emotional promotion, retention snapshots and history rows, all read back after a restart |
//! | `import_export` | `backup_to` snapshot restored into a fresh data dir and searched **there**; JSON export re-imported; merge/re-import idempotence |
//! | `intentions_workflow` | `save_intention` → restart → still active with its trigger payload → the core trigger matcher fires on a matching context only |
//! | `preprocessing_pipeline` | `preprocess` → ingest → restart: rewritten content, entity tags, temporal anchors and provenance are persisted and searchable |
//! | `spreading_activation` | persisted `memory_connections` graph → restart → real BFS (`get_memory_subgraph`, `find_path_between`) → decay/prune without deleting memories |
//! | `storage_persistence` | the original journey (ingest → search → review → reopen) plus the "consolidation never deletes" regression for `608d866` |
//!
//! **The DTO / pure-function contract tests stay too.** They pin input/output
//! contracts (`IntentDetector`, `vestige_core::preprocessing`,
//! `ActivationNetwork`, `FSRSScheduler`, `SleepConsolidation`, `MemoryDreamer`,
//! the `serde` shapes of the DTOs the MCP layer accepts) and none of them claim
//! to be end-to-end. Their names must not be read as proof that a user workflow
//! runs — the table above is the list of workflows that actually run.
//!
//! ## Rules for new tests here
//!
//! * Assert observable behaviour of the system under test. A test that feeds a
//!   value in and asserts it comes back unchanged is not a test.
//! * Nothing may skip silently. These journeys must not spawn the
//!   `vestige-mcp` binary: CI's journey-tests job
//!   (`.github/workflows/test.yml:213-223`) runs this target without building
//!   the server, so a child-process test would report green while covering
//!   nothing. Child-process coverage lives in the `mcp_protocol` target.
//! * One temp data dir per test, mock embeddings
//!   (`enable_mock_embeddings`), no network, no wall-clock assumptions beyond
//!   what `time_travel` provides, and no assumption about a pre-existing
//!   server.

pub mod consolidation_workflow;
pub mod import_export;
pub mod ingest_recall_review;
pub mod intentions_workflow;
pub mod preprocessing_pipeline;
pub mod spreading_activation;
pub mod storage_persistence;
