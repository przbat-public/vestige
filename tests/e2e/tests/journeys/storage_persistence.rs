//! # Real Storage journeys (SQLite-backed)
//!
//! This file holds the original persistence journeys: a `Storage` instance on a
//! temp SQLite file, the actual FTS5 + vector search paths, FSRS review state,
//! and a full close/reopen cycle. Its siblings in this directory have since
//! gained their own real journeys (see `journeys/mod.rs` for the table); the
//! DTO / pure-function contract tests live on inside those same files.
//!
//! Two journeys live here:
//!
//! 1. [`test_storage_journey_ingest_search_review_persist`] — ingest → read
//!    back → keyword search → hybrid search → `mark_reviewed` → `promote_memory`
//!    → reopen the database and verify every piece of state survived.
//! 2. [`test_consolidation_never_deletes_low_retention_memories`] — the
//!    regression test for the worst bug in this repo's history (commit
//!    `608d866`: consolidation silently deleted low-retention memories).
//!
//! …plus [`test_journey_uses_a_file_backed_database`], a cheap guard that the
//! journeys really run against a file on disk rather than an in-memory store.
//!
//! The deterministic offline embedder is enabled for both (see
//! `vestige_e2e_tests::harness::enable_mock_embeddings`): without it every
//! `Storage::ingest` lazily initializes the real ONNX model, which downloads
//! ~547 MB on a cold cache — unacceptable for a unit-speed test.

use chrono::{Duration, Utc};
use rusqlite::Connection;
use tempfile::TempDir;
use vestige_core::{IngestInput, Rating};
use vestige_e2e_tests::harness::{
    TestDatabaseManager, enable_mock_embeddings, enable_mock_embeddings_below_retention_target,
};

/// Build an `IngestInput` for a memory (the struct is `#[non_exhaustive]`).
fn memory(content: &str, node_type: &str, tags: &[&str]) -> IngestInput {
    IngestInput {
        content: content.to_string(),
        node_type: node_type.to_string(),
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        ..Default::default()
    }
}

/// Reopen a database file into a fresh `TestDatabaseManager`
/// (`new_at_path` never deletes the file, so this is a real cold start).
fn reopen(db_path: &std::path::Path) -> TestDatabaseManager {
    TestDatabaseManager::new_at_path(db_path.to_path_buf())
}

// ============================================================================
// JOURNEY 1: ingest → search → review → persist
// ============================================================================

#[test]
fn test_storage_journey_ingest_search_review_persist() {
    // The mock embedder must be on before `Storage::new` (the ingest path
    // probes the model, and a cold cache would try to download it).
    let _mock = enable_mock_embeddings();

    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("journey.db");
    let db = TestDatabaseManager::new_at_path(db_path.clone());
    assert!(db.is_empty(), "a fresh database must start empty");

    // ---- 1. Ingest: three memories through the production write path ------
    let fsrs_memory = "The FSRS-6 scheduler schedules the next review from retrievability.";
    let gc_memory =
        "Consolidation never deletes memories; it only reports low retention candidates.";
    let graph_memory = "Spreading activation walks the knowledge graph from a seed memory.";

    let a = db
        .storage
        .ingest(memory(fsrs_memory, "fact", &["fsrs", "scheduling"]))
        .expect("ingest must succeed");
    let b = db
        .storage
        .ingest(memory(gc_memory, "decision", &["consolidation", "safety"]))
        .expect("ingest must succeed");
    let c = db
        .storage
        .ingest(memory(graph_memory, "concept", &["graph", "activation"]))
        .expect("ingest must succeed");

    assert_eq!(
        db.node_count(),
        3,
        "every ingest must create exactly one row"
    );

    // ---- 2. Read back (get_node) ------------------------------------------
    let fetched = db
        .storage
        .get_node(&b.id)
        .expect("get_node must not error")
        .expect("the ingested memory must exist");
    assert_eq!(fetched.content, gc_memory);
    assert_eq!(fetched.node_type, "decision");
    assert_eq!(fetched.tags, vec!["consolidation", "safety"]);
    assert!(
        fetched.has_embedding.unwrap_or(false),
        "ingest must persist an embedding vector (mock embedder is enabled)"
    );
    assert_eq!(
        fetched.embedding_model.as_deref(),
        Some(vestige_core::embeddings::embedding_model_tag()),
        "the stored vector must be tagged with the regime that produced it"
    );
    for (label, id) in [("fsrs", &a.id), ("gc", &b.id), ("graph", &c.id)] {
        assert!(
            db.storage
                .get_node(id)
                .expect("get_node must not error")
                .is_some(),
            "the {label} memory must be retrievable by id"
        );
    }

    // ---- 3. Keyword search (FTS5 path) -----------------------------------
    let keyword_hits = db
        .storage
        .keyword_search("consolidation", 10, 0.0)
        .expect("keyword_search must succeed");
    assert!(
        keyword_hits.iter().any(|n| n.id == b.id),
        "keyword search must find the memory containing 'consolidation', got {:?}",
        keyword_hits.iter().map(|n| &n.id).collect::<Vec<_>>()
    );

    // ---- 4. Hybrid search (FTS5 + embeddings + RRF) ----------------------
    let (kw_weight, sem_weight) = vestige_core::default_hybrid_weights();
    let hybrid_hits = db
        .storage
        .hybrid_search(
            "which memory says reviews are scheduled from retrievability",
            5,
            kw_weight,
            sem_weight,
        )
        .expect("hybrid_search must succeed");
    assert!(
        hybrid_hits.iter().any(|r| r.node.id == a.id),
        "hybrid search must find the FSRS memory, got {:?}",
        hybrid_hits
            .iter()
            .map(|r| (&r.node.id, r.combined_score))
            .collect::<Vec<_>>()
    );

    // ---- 5. Review: mark_reviewed moves the FSRS schedule ------------------
    let reviewed = db
        .storage
        .mark_reviewed(&a.id, Rating::Good)
        .expect("mark_reviewed must succeed");
    assert_eq!(
        reviewed.reps, 1,
        "a Good review must count as one repetition"
    );
    let next_review = reviewed
        .next_review
        .expect("a reviewed memory must be scheduled for review");
    assert!(
        next_review > Utc::now(),
        "the next review must be scheduled in the future, got {next_review}"
    );
    assert!(
        reviewed.storage_strength > a.storage_strength,
        "a Good review must strengthen storage ({} -> {})",
        a.storage_strength,
        reviewed.storage_strength
    );

    // ---- 6. Promote: user feedback on top of a review ---------------------
    let promoted = db
        .storage
        .promote_memory(&a.id)
        .expect("promote_memory must succeed");
    assert!(
        promoted.reps > reviewed.reps,
        "promotion runs a review under the hood ({} -> {})",
        reviewed.reps,
        promoted.reps
    );
    assert!(
        promoted.retention_strength > reviewed.retention_strength,
        "promotion must raise retention ({} -> {})",
        reviewed.retention_strength,
        promoted.retention_strength
    );

    // ---- 7. Cold start: state must survive closing the database ----------
    let before = db.node_count();
    let promoted_state = promoted.clone();
    drop(db);

    let reopened = reopen(&db_path);
    assert_eq!(
        reopened.node_count(),
        before,
        "reopening must not lose (or duplicate) memories"
    );
    let stats = reopened
        .storage
        .get_stats()
        .expect("get_stats must succeed");
    assert_eq!(stats.total_nodes, 3);
    assert_eq!(
        stats.nodes_with_embeddings, 3,
        "all three vectors must be persisted, not just held in memory"
    );

    let after = reopened
        .storage
        .get_node(&a.id)
        .expect("get_node after reopen must not error")
        .expect("the reviewed memory must survive a restart");
    assert_eq!(after.content, promoted_state.content);
    assert_eq!(after.tags, promoted_state.tags);
    assert_eq!(after.node_type, promoted_state.node_type);
    assert_eq!(
        after.reps, promoted_state.reps,
        "FSRS repetition count must be durable"
    );
    assert_eq!(
        after.storage_strength, promoted_state.storage_strength,
        "FSRS storage strength must be durable"
    );
    assert_eq!(
        after.next_review, promoted_state.next_review,
        "the scheduled review date must be durable"
    );
    assert!(
        after.next_review.expect("next_review") > Utc::now(),
        "the persisted schedule must still be in the future"
    );

    // The reopened storage must rebuild its vector index from SQLite, so
    // semantic retrieval keeps working without the previous process.
    let hybrid_after = reopened
        .storage
        .hybrid_search(
            "consolidation deletes memories report",
            5,
            kw_weight,
            sem_weight,
        )
        .expect("hybrid_search after reopen must succeed");
    assert!(
        hybrid_after.iter().any(|r| r.node.id == b.id),
        "search must work after a cold start (vector index rebuilt from SQLite)"
    );
}

// ============================================================================
// JOURNEY 2: consolidation must never delete memories (regression, 608d866)
// ============================================================================

/// Age the given rows and give them a fully-decayed FSRS state, so that the
/// product's own `apply_decay` computes a retention below the 0.3 threshold
/// used by `gc_below_retention`.
///
/// The columns are written directly because `Storage` has no way to backdate a
/// memory (no injectable clock — see the audit note on `TimeTravelEnvironment`);
/// everything *read* in the assertions below still comes from `Storage`.
fn backdate_and_decay(db_path: &std::path::Path, ids: &[String]) {
    let conn = Connection::open(db_path).expect("raw connection for test setup");
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .expect("busy_timeout");
    let old = (Utc::now() - Duration::days(45)).to_rfc3339();
    for id in ids {
        conn.execute(
            "UPDATE knowledge_nodes SET
                created_at = ?1,
                last_accessed = ?1,
                stability = 0.0001,
                storage_strength = 1.0,
                retrieval_strength = 0.05,
                retention_strength = 0.05
             WHERE id = ?2",
            rusqlite::params![old, id],
        )
        .expect("test setup UPDATE must succeed");
    }
}

/// Five memories with disjoint vocabularies, all of them decayed past every
/// threshold the historical auto-GC used.
///
/// Disjoint vocabularies on purpose: consolidation also *merges* memories that
/// are ≥ 0.85 similar (a separate, intended feature). Sharing tokens would make
/// the mock embedder score these as near-duplicates, and the merge path — not
/// the removed auto-GC — would shrink the store.
const DECAYED_CONTENTS: [&str; 5] = [
    "alpha bravo charlie delta echo",
    "foxtrot golf hotel india juliet",
    "kilo lima mike november oscar",
    "papa quebec romeo sierra tango",
    "uniform victor whiskey xray yankee",
];

/// Ingest `contents` into a fresh store, then give every row a decayed,
/// 45-day-old FSRS state. Returns the store and the ids in ingest order.
fn seed_and_backdate(
    db_path: &std::path::Path,
    contents: &[&str],
) -> (TestDatabaseManager, Vec<String>) {
    let db = TestDatabaseManager::new_at_path(db_path.to_path_buf());
    let mut ids = Vec::with_capacity(contents.len());
    for (i, content) in contents.iter().enumerate() {
        let node = db
            .storage
            .ingest(memory(content, "fact", &[&format!("decayed-{i}")]))
            .expect("ingest must succeed");
        ids.push(node.id);
    }
    backdate_and_decay(db_path, &ids);
    (db, ids)
}

/// The worst bug in this repo's history: `Storage::run_consolidation` called
/// `gc_below_retention(0.3, 30)`, so a background cycle silently hard-deleted
/// memories below 30% retention older than 30 days whenever the store average
/// dipped under `VESTIGE_RETENTION_TARGET`. Commit `608d866` removed the call
/// and changed the block to *report* candidates — and nothing in the suite
/// pinned that, so a re-introduction would have shipped unnoticed.
///
/// This test recreates the exact pre-fix conditions:
///
/// * 5 memories with retention below 0.3 (produced by the decay formula),
/// * all older than 30 days (so `gc_below_retention(0.3, 30)` matches them),
/// * `VESTIGE_RETENTION_TARGET=1.0` (so the below-target branch is taken).
///
/// …and asserts consolidation leaves every single one of them in place. A
/// second, deliberately destructive phase proves the rows really were GC
/// candidates, so the test cannot pass vacuously.
#[test]
fn test_consolidation_never_deletes_low_retention_memories() {
    let _env = enable_mock_embeddings_below_retention_target(1.0);

    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("consolidation_regression.db");
    let (db, ids) = seed_and_backdate(&db_path, &DECAYED_CONTENTS);
    assert_eq!(db.node_count(), 5);

    // Precondition: these memories are GC candidates *and* old enough for the
    // historical `gc_below_retention(0.3, 30)` call to have matched them.
    let below_before = db
        .storage
        .count_memories_below_retention(0.3)
        .expect("count_memories_below_retention must succeed");
    assert_eq!(
        below_before, 5,
        "test setup must produce 5 memories below the 0.3 retention threshold"
    );
    let aged = db
        .storage
        .get_node(&ids[0])
        .expect("get_node")
        .expect("node must exist");
    assert!(
        aged.created_at < Utc::now() - Duration::days(30),
        "test setup must backdate the memories past the 30-day GC age gate, got {}",
        aged.created_at
    );
    assert!(
        db.storage.get_avg_retention().expect("avg retention") < 1.0,
        "forcing the retention target to 1.0 must put the store 'below target'"
    );
    let ids_before: std::collections::BTreeSet<String> = ids.iter().cloned().collect();

    // ---- The act: a full consolidation cycle ------------------------------
    //
    // The pipeline used to abort in step 1 — `apply_decay` →
    // `helpers::begin_write_transaction`, which ran `BEGIN IMMEDIATE` and then
    // called `Connection::unchecked_transaction()`, issuing a *second* `BEGIN`
    // and failing with "cannot start a transaction within a transaction". That
    // is fixed (`helpers.rs:311-313` now uses `transaction_with_behavior`), so
    // the `Ok` arm is the live path and the assertions inside it run. The arm
    // that tolerates that exact message is kept as a regression guard: the
    // invariant under test (consolidation never deletes) must hold either way,
    // but any *other* failure fails the test.
    let cycle_completed = match db.storage.run_consolidation() {
        Ok(result) => {
            assert_eq!(
                result.nodes_pruned, 0,
                "consolidation reported pruning memories: {result:?}"
            );
            true
        }
        Err(error) => {
            let message = error.to_string();
            assert!(
                message.contains("cannot start a transaction within a transaction"),
                "run_consolidation failed with an unexpected error: {message}"
            );
            eprintln!(
                "[REGRESSION] run_consolidation aborted before reaching the retention block \
                 (helpers::begin_write_transaction issued a second BEGIN again): {message}\n\
                 [REGRESSION] the node-deletion invariant is still asserted below."
            );
            false
        }
    };

    // ---- Nothing may be deleted ------------------------------------------
    let ids_after: std::collections::BTreeSet<String> = db
        .storage
        .get_all_nodes(1000, 0)
        .expect("get_all_nodes must succeed")
        .into_iter()
        .map(|n| n.id)
        .collect();
    for id in &ids_before {
        assert!(
            ids_after.contains(id),
            "consolidation deleted {id}; survivors: {ids_after:?}"
        );
    }
    assert!(
        ids_after.len() >= ids_before.len(),
        "consolidation shrank the store: {} -> {}",
        ids_before.len(),
        ids_after.len()
    );
    assert!(
        db.node_count() >= 5,
        "node count must not decrease (was 5, now {})",
        db.node_count()
    );

    // The candidates are still there *after* the cycle — proof that the
    // below-target branch really had work to (not) do, rather than the setup
    // having quietly evaporated during decay.
    assert_eq!(
        db.storage
            .count_memories_below_retention(0.3)
            .expect("count_memories_below_retention must succeed"),
        5,
        "the low-retention candidates must survive consolidation"
    );

    if cycle_completed {
        for _ in 0..2 {
            let result = db
                .storage
                .run_consolidation()
                .expect("repeat consolidation must succeed");
            assert_eq!(result.nodes_pruned, 0, "repeat cycle pruned memories");
        }
        assert_eq!(
            db.node_count(),
            5,
            "repeat cycles must not delete anything either"
        );
        assert_ne!(
            db.storage
                .get_retention_trend()
                .expect("get_retention_trend must succeed"),
            "insufficient_data",
            "consolidation must persist retention snapshots (the reporting half of the fix)"
        );
    }

    // ---- Anti-vacuity: the same state is a genuine GC target --------------
    // `gc_below_retention(0.3, 30)` is the primitive the removed auto-path
    // called. That it still deletes exactly these rows shows the assertions
    // above are meaningful: the old code would have wiped the store here.
    //
    // This runs against a second, identically prepared database on purpose: a
    // failed `begin_write_transaction` leaves the writer connection inside the
    // `BEGIN IMMEDIATE` it managed to issue, so everything written on that
    // connection afterwards stays uncommitted (and invisible to readers). The
    // fresh store has no such history, so the deletion really commits.
    let anti_vacuity_path = dir.path().join("anti_vacuity.db");
    let (target, _) = seed_and_backdate(&anti_vacuity_path, &DECAYED_CONTENTS);
    let deleted = target
        .storage
        .gc_below_retention(0.3, 30)
        .expect("explicit gc_below_retention must succeed");
    assert_eq!(
        deleted, 5,
        "the backdated, low-retention memories must be exactly what the old \
         automatic gc_below_retention(0.3, 30) call targeted"
    );
    assert_eq!(
        target.node_count(),
        0,
        "explicit GC is the only thing that deletes"
    );
}

// ============================================================================
// Guard: this file drives a real file-backed store
// ============================================================================

/// Cheap sanity check that the journey tests above really used a file-backed
/// store (a regression here would silently turn them into in-memory tests that
/// prove nothing about persistence).
#[test]
fn test_journey_uses_a_file_backed_database() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("file_backed.db");
    let db = TestDatabaseManager::new_at_path(db_path.clone());
    assert!(
        db_path.exists(),
        "Storage must create the database file on disk at {db_path:?}"
    );
    assert_eq!(db.path(), &db_path);
    drop(db);
    // Reopening the same path must work (no exclusive lock left behind).
    let reopened = reopen(&db_path);
    assert_eq!(reopened.node_count(), 0);
}
