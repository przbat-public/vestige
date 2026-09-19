//! # User Journey E2E Tests
//!
//! ## What is in this directory
//!
//! **1. The real end-to-end journey — [`storage_persistence`]**
//!
//! `storage_persistence.rs` is the only journey that constructs a `Storage`
//! instance and drives the production persistence path: ingest → `get_node` →
//! `keyword_search` → `hybrid_search` → `mark_reviewed` → `promote_memory` →
//! reopen the database and verify the state survived. It also holds the
//! regression test for the worst bug in this repo's history (consolidation
//! silently deleting low-retention memories, fixed in `608d866`).
//!
//! **2. DTO / pure-function contract tests — everything else**
//!
//! The remaining modules (`consolidation_workflow`, `import_export`,
//! `ingest_recall_review`, `intentions_workflow`, `preprocessing_pipeline`,
//! `spreading_activation`) are **contract tests, not end-to-end tests**. They
//! exercise real product code — `IntentDetector`, `vestige_core::preprocessing`,
//! `ActivationNetwork`, `FSRSScheduler`, `SleepConsolidation`, `MemoryDreamer`,
//! and the `serde` shapes of the DTOs the MCP layer accepts and returns — but
//! they never touch `Storage`, SQLite or the network. Their value is pinning
//! input/output contracts; their names must not be read as proof that a user
//! workflow runs.
//!
//! Do not add new "journey" tests here without a `Storage`; put them in
//! `storage_persistence.rs` (or a sibling that constructs `Storage` via
//! `vestige_e2e_tests::harness::TestDatabaseManager`).

pub mod consolidation_workflow;
pub mod import_export;
pub mod ingest_recall_review;
pub mod intentions_workflow;
pub mod preprocessing_pipeline;
pub mod spreading_activation;
pub mod storage_persistence;
