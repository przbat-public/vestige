//! Storage layer integration tests.

use chrono::{Duration, Utc};
use tempfile::tempdir;

use crate::fsrs::Rating;
use crate::memory::IngestInput;

use super::Storage;
use super::StorageError;
use super::init::{
    ENCRYPTION_KEY_ENV, EncryptionConfig, REQUIRE_ENCRYPTION_ENV, VectorIndexSource,
};
use super::records::{ConnectionRecord, DreamHistoryRecord, InsightRecord};

fn create_test_storage() -> Storage {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    Storage::new(Some(db_path)).unwrap()
}

#[test]
fn test_storage_creation() {
    let storage = create_test_storage();
    let stats = storage.get_stats().unwrap();
    assert_eq!(stats.total_nodes, 0);
}

#[test]
fn test_ingest_and_get() {
    let storage = create_test_storage();

    let input = IngestInput {
        content: "Test memory content".to_string(),
        node_type: "fact".to_string(),
        ..Default::default()
    };

    let node = storage.ingest(input).unwrap();
    assert!(!node.id.is_empty());
    assert_eq!(node.content, "Test memory content");

    let retrieved = storage.get_node(&node.id).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().content, "Test memory content");
}

#[test]
fn test_typed_memory_roundtrip() {
    use crate::memory::MemoryKind;

    let storage = create_test_storage();
    let event_time = chrono::Utc::now();

    // Default ingest keeps memory_kind = Raw.
    let raw = storage
        .ingest(IngestInput {
            content: "Raw chunk".to_string(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(raw.memory_kind, MemoryKind::Raw);
    assert!(raw.subject.is_none());

    // Typed ingest preserves kind + subject + episodic_at through the
    // SQLite roundtrip (validates migration v11 wired columns end-to-end).
    let episodic = storage
        .ingest(IngestInput {
            content: "Caroline attended the LGBTQ group on 7 May 2023.".to_string(),
            node_type: "event".to_string(),
            memory_kind: MemoryKind::Episodic,
            subject: Some("Caroline".to_string()),
            episodic_at: Some(event_time),
            ..Default::default()
        })
        .unwrap();
    let fetched = storage.get_node(&episodic.id).unwrap().unwrap();
    assert_eq!(fetched.memory_kind, MemoryKind::Episodic);
    assert_eq!(fetched.subject.as_deref(), Some("Caroline"));
    assert_eq!(
        fetched.episodic_at.map(|t| t.timestamp()),
        Some(event_time.timestamp())
    );

    // Semantic triple (subject, predicate, object) also roundtrips.
    let semantic = storage
        .ingest(IngestInput {
            content: "Caroline lives in Berlin.".to_string(),
            memory_kind: MemoryKind::Semantic,
            subject: Some("Caroline".to_string()),
            predicate: Some("lives_in".to_string()),
            object: Some("Berlin".to_string()),
            ..Default::default()
        })
        .unwrap();
    let fetched = storage.get_node(&semantic.id).unwrap().unwrap();
    assert_eq!(fetched.memory_kind, MemoryKind::Semantic);
    assert_eq!(fetched.predicate.as_deref(), Some("lives_in"));
    assert_eq!(fetched.object.as_deref(), Some("Berlin"));

    // Procedural with frequency descriptor.
    let proc = storage
        .ingest(IngestInput {
            content: "Caroline goes to therapy every Tuesday.".to_string(),
            memory_kind: MemoryKind::Procedural,
            subject: Some("Caroline".to_string()),
            procedural_frequency: Some("every Tuesday".to_string()),
            ..Default::default()
        })
        .unwrap();
    let fetched = storage.get_node(&proc.id).unwrap().unwrap();
    assert_eq!(fetched.memory_kind, MemoryKind::Procedural);
    assert_eq!(
        fetched.procedural_frequency.as_deref(),
        Some("every Tuesday")
    );
}

#[test]
fn test_search() {
    let storage = create_test_storage();

    let input = IngestInput {
        content: "The mitochondria is the powerhouse of the cell".to_string(),
        node_type: "fact".to_string(),
        ..Default::default()
    };

    storage.ingest(input).unwrap();

    let results = storage.search("mitochondria", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("mitochondria"));
}

#[test]
fn test_review() {
    let storage = create_test_storage();

    let input = IngestInput {
        content: "Test review".to_string(),
        node_type: "fact".to_string(),
        ..Default::default()
    };

    let node = storage.ingest(input).unwrap();
    assert_eq!(node.reps, 0);

    let reviewed = storage.mark_reviewed(&node.id, Rating::Good).unwrap();
    assert_eq!(reviewed.reps, 1);
}

#[test]
fn test_delete() {
    let storage = create_test_storage();

    let input = IngestInput {
        content: "To be deleted".to_string(),
        node_type: "fact".to_string(),
        ..Default::default()
    };

    let node = storage.ingest(input).unwrap();
    assert!(storage.get_node(&node.id).unwrap().is_some());

    let deleted = storage.delete_node(&node.id).unwrap();
    assert!(deleted);
    assert!(storage.get_node(&node.id).unwrap().is_none());
}

#[test]
fn test_dream_history_save_and_get_last() {
    let storage = create_test_storage();
    let now = Utc::now();

    let record = DreamHistoryRecord {
        dreamed_at: now,
        duration_ms: 1500,
        memories_replayed: 50,
        connections_found: 12,
        insights_generated: 3,
        memories_strengthened: 8,
        memories_compressed: 2,
        phase_nrem1_ms: None,
        phase_nrem3_ms: None,
        phase_rem_ms: None,
        phase_integration_ms: None,
        summaries_generated: None,
        emotional_memories_processed: None,
        creative_connections_found: None,
    };

    let id = storage.save_dream_history(&record).unwrap();
    assert!(id > 0);

    let last = storage.get_last_dream().unwrap();
    assert!(last.is_some());
    // Timestamps should be within 1 second (RFC3339 round-trip)
    let diff = (last.unwrap() - now).num_seconds().abs();
    assert!(diff <= 1, "Timestamp mismatch: diff={}s", diff);
}

#[test]
fn test_dream_history_empty() {
    let storage = create_test_storage();
    let last = storage.get_last_dream().unwrap();
    assert!(last.is_none());
}

#[test]
fn test_count_memories_since() {
    let storage = create_test_storage();
    let before = Utc::now() - Duration::seconds(10);

    for i in 0..5 {
        storage
            .ingest(IngestInput {
                content: format!("Count test memory {}", i),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap();
    }

    let count = storage.count_memories_since(before).unwrap();
    assert_eq!(count, 5);

    let future = Utc::now() + Duration::hours(1);
    let count_future = storage.count_memories_since(future).unwrap();
    assert_eq!(count_future, 0);
}

#[test]
fn test_get_last_backup_timestamp_no_panic() {
    // Static method should not panic even if no backups exist
    let _ = Storage::get_last_backup_timestamp();
}

/// Empty DBs should report `Empty`, not `Rebuilt`. Conflating the two would
/// confuse the dashboard "Vector index source" badge.
/// SQL-side filtering for the dashboard memories list. The naive
/// "load all rows, filter in Rust" path scales linearly with the table and
/// blows the 200-row LIMIT contract once a tag covers more than 200 rows.
/// We need a single helper that pushes `node_type`, `tag`, and
/// `min_retention` into the SQL WHERE so the LIMIT is meaningful.
#[test]
fn test_get_all_nodes_filtered_by_node_type() {
    use crate::memory::IngestInput;

    let storage = create_test_storage();
    for (kind, tag) in [
        ("fact", "alpha"),
        ("note", "alpha"),
        ("fact", "beta"),
        ("note", "beta"),
    ] {
        storage
            .ingest(IngestInput {
                content: format!("{}-{}", kind, tag),
                node_type: kind.to_string(),
                tags: vec![tag.to_string()],
                ..Default::default()
            })
            .unwrap();
    }

    let facts = storage
        .get_all_nodes_filtered(50, 0, Some("fact"), None, None)
        .unwrap();
    assert_eq!(facts.len(), 2);
    for n in &facts {
        assert_eq!(n.node_type, "fact");
    }
}

#[test]
fn test_get_all_nodes_filtered_by_tag() {
    use crate::memory::IngestInput;

    let storage = create_test_storage();
    storage
        .ingest(IngestInput {
            content: "tagged".into(),
            node_type: "fact".into(),
            tags: vec!["acme".into(), "personal".into()],
            ..Default::default()
        })
        .unwrap();
    storage
        .ingest(IngestInput {
            content: "untagged".into(),
            node_type: "fact".into(),
            ..Default::default()
        })
        .unwrap();

    let acme = storage
        .get_all_nodes_filtered(50, 0, None, Some("acme"), None)
        .unwrap();
    assert_eq!(acme.len(), 1);
    assert_eq!(acme[0].content, "tagged");
}

#[test]
fn test_get_all_nodes_filtered_respects_limit_post_filter() {
    use crate::memory::IngestInput;

    let storage = create_test_storage();
    for i in 0..10 {
        storage
            .ingest(IngestInput {
                content: format!("memory {}", i),
                node_type: if i < 5 { "fact" } else { "note" }.into(),
                tags: vec!["common".into()],
                ..Default::default()
            })
            .unwrap();
    }

    // Asking for 3 rows of `note` must return exactly 3 — pushdown lets the
    // engine STOP scanning after 3. Without pushdown the Rust caller would
    // load all 5 `note` rows and slice to 3, defeating the LIMIT contract.
    let notes = storage
        .get_all_nodes_filtered(3, 0, Some("note"), None, None)
        .unwrap();
    assert_eq!(notes.len(), 3);
    for n in &notes {
        assert_eq!(n.node_type, "note");
    }
}

/// Pagination correctness: callers need `total` separately from
/// `page.len()`. Without a count helper the dashboard reported
/// "Total: 100" while the actual population was 3000+ — see audit
/// 2026-05-22. The count must respect the *same* WHERE filters as
/// `get_all_nodes_filtered`, otherwise the UI's "X of Y" becomes lies.
#[test]
fn test_count_nodes_filtered_matches_filters() {
    use crate::memory::IngestInput;

    let storage = create_test_storage();
    for i in 0..10 {
        storage
            .ingest(IngestInput {
                content: format!("memory {}", i),
                node_type: if i < 7 { "fact" } else { "note" }.into(),
                tags: vec!["common".into()],
                ..Default::default()
            })
            .unwrap();
    }

    // No filter → 10
    assert_eq!(storage.count_nodes_filtered(None, None, None).unwrap(), 10);

    // node_type filter → 7 facts
    assert_eq!(
        storage
            .count_nodes_filtered(Some("fact"), None, None)
            .unwrap(),
        7
    );

    // tag filter → 10 (all tagged with "common")
    assert_eq!(
        storage
            .count_nodes_filtered(None, Some("common"), None)
            .unwrap(),
        10
    );

    // unknown tag → 0
    assert_eq!(
        storage
            .count_nodes_filtered(None, Some("missing"), None)
            .unwrap(),
        0
    );

    // The count is independent of LIMIT — confirm that a small page
    // does NOT cap the count.
    let page = storage
        .get_all_nodes_filtered(3, 0, Some("fact"), None, None)
        .unwrap();
    assert_eq!(page.len(), 3);
    assert_eq!(
        storage
            .count_nodes_filtered(Some("fact"), None, None)
            .unwrap(),
        7,
        "count must reflect the population, not the page size"
    );
}

#[test]
fn test_vector_index_source_empty_on_fresh_db() {
    let storage = create_test_storage();
    assert_eq!(storage.vector_index_source(), VectorIndexSource::Empty);
}

/// First boot with embeddings: should be `Rebuilt` and the sidecar should
/// be written. Second boot from the same DB path: should be `Loaded` and
/// the sidecar should still be there. If this regresses we silently lose
/// the persistence — exactly the failure mode the audit caught.
#[test]
fn test_vector_index_persistence_roundtrip() {
    use crate::memory::IngestInput;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // ── First boot ──
    let storage = Storage::new(Some(db_path.clone())).unwrap();
    // Ingesting writes an embedding row via the production path.
    storage
        .ingest(IngestInput {
            content: "memory under test — must persist into HNSW sidecar".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();
    // No memories at startup → Empty. After ingesting we'd normally need
    // another boot to see Rebuilt, so explicitly force the persist now.
    assert_eq!(storage.vector_index_source(), VectorIndexSource::Empty);
    storage.persist_vector_index().unwrap();

    let sidecar = storage.vector_index_path();
    let meta = storage.vector_index_meta_path();
    assert!(sidecar.exists(), "HNSW binary should be persisted");
    assert!(meta.exists(), "HNSW meta should be persisted");
    drop(storage);

    // ── Second boot ──
    let storage2 = Storage::new(Some(db_path.clone())).unwrap();
    assert_eq!(
        storage2.vector_index_source(),
        VectorIndexSource::Loaded,
        "second boot must restore HNSW from sidecar, not rebuild"
    );
}

/// If the sidecar's row count disagrees with the DB (e.g. someone wrote
/// embeddings while the process was off), we must rebuild — using a stale
/// HNSW would silently make recall worse for any new memory.
#[test]
fn test_vector_index_stale_sidecar_triggers_rebuild() {
    use crate::memory::IngestInput;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // Boot 1: ingest one memory and persist.
    let storage = Storage::new(Some(db_path.clone())).unwrap();
    storage
        .ingest(IngestInput {
            content: "first memory".into(),
            node_type: "fact".into(),
            ..Default::default()
        })
        .unwrap();
    storage.persist_vector_index().unwrap();
    drop(storage);

    // Boot 2: corrupt meta to claim a different row count.
    let meta_path = {
        let s = Storage::new(Some(db_path.clone())).unwrap();
        s.vector_index_meta_path()
    };
    let stale = serde_json::json!({"schema": 1, "row_count": 999, "dimensions": 384});
    std::fs::write(&meta_path, stale.to_string()).unwrap();

    // Boot 3: should detect stale meta and rebuild.
    let storage3 = Storage::new(Some(db_path)).unwrap();
    assert_eq!(
        storage3.vector_index_source(),
        VectorIndexSource::Rebuilt,
        "stale sidecar metadata must force a rebuild from SQLite"
    );
}

// ============================================================================
// FSRS-6 personalized weights (t3-fsrs)
//
// The optimizer writes w20 into `fsrs_config` during consolidation, but
// without these wires the scheduler stays at the FSRS-6 default forever.
// Tests below pin the round-trip: w20 persists, reloads on the next boot,
// and the in-memory scheduler reflects it on `params().weights[20]`.
// ============================================================================

#[test]
fn load_personalized_w20_is_none_on_fresh_db() {
    let storage = create_test_storage();
    // Fresh DB seeded the default via migration; "personalized" means
    // explicitly overridden via the public setter — not the seed value.
    let w20 = storage.load_personalized_w20().unwrap();
    assert_eq!(
        w20, None,
        "no override yet — caller should fall back to FSRS6_WEIGHTS[20]"
    );
}

#[test]
fn save_then_load_personalized_w20_roundtrip() {
    let storage = create_test_storage();
    storage.save_personalized_w20(0.42).unwrap();
    let w20 = storage
        .load_personalized_w20()
        .unwrap()
        .expect("just wrote a value");
    assert!(
        (w20 - 0.42).abs() < 1e-9,
        "expected 0.42, got {w20} — persistence layer corrupted the value"
    );
}

// ============================================================================
// GDPR right_to_erasure — table-name bug (gdpr-erasure)
//
// `gdpr.rs` historically called `DELETE FROM access_log` while every other
// path uses `memory_access_log`. The mismatch turned the public GDPR
// entry-point into a guaranteed runtime error that wiped nothing.
// ============================================================================

#[test]
fn right_to_erasure_succeeds_and_removes_access_log_entries() {
    let storage = create_test_storage();

    let node = storage
        .ingest(IngestInput {
            content: "a memory the data subject has asked us to forget".into(),
            node_type: "fact".into(),
            ..Default::default()
        })
        .unwrap();

    // Two access events so we can assert that the deletion path runs to
    // completion and counts every artifact, not just the node itself.
    storage.log_access(&node.id, "search_hit").unwrap();
    storage.log_access(&node.id, "promote").unwrap();

    // Pre-fix: this returned `Err(SqliteFailure(no such table: access_log))`
    // and the knowledge node was never deleted. Post-fix: the function must
    // succeed and report at least 3 artifacts erased (node + 2 access rows).
    let erased = storage.right_to_erasure(&node.id).expect(
        "right_to_erasure must not error — DELETE was hitting a misspelled table that does not exist",
    );
    assert!(
        erased >= 3,
        "expected >=3 artifacts erased (node + 2 access rows), got {erased}"
    );

    assert!(
        storage.get_node(&node.id).unwrap().is_none(),
        "node remained in storage after GDPR erasure"
    );
}

#[test]
fn personalized_w20_is_applied_to_scheduler_at_boot() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("personalized.db");

    // Boot 1: open, save personalized w20.
    {
        let s = Storage::new(Some(db_path.clone())).unwrap();
        s.save_personalized_w20(0.37).unwrap();
    }

    // Boot 2: scheduler must reflect the persisted value, not the FSRS-6
    // default. Without the init-time hookup this assertion fails because
    // `FSRSScheduler::default()` ignores the database.
    let s2 = Storage::new(Some(db_path)).unwrap();
    let weights = s2.scheduler_weights_snapshot();
    assert!(
        (weights[20] - 0.37).abs() < 1e-9,
        "scheduler still on default w20 = {} — personalization never reached the scheduler",
        weights[20]
    );
}

/// Helpers for the find_path_between tests below — keep the chain wiring
/// inline so the assertions read like a small graph problem statement.
fn ingest_id(storage: &Storage, content: &str) -> String {
    storage
        .ingest(IngestInput {
            content: content.to_string(),
            ..Default::default()
        })
        .unwrap()
        .id
}

fn connect(storage: &Storage, source: &str, target: &str) {
    let now = Utc::now();
    storage
        .save_connection(&ConnectionRecord {
            source_id: source.to_string(),
            target_id: target.to_string(),
            strength: 0.7,
            link_type: "related".to_string(),
            created_at: now,
            last_activated: now,
            activation_count: 0,
        })
        .unwrap();
}

#[test]
fn find_path_between_returns_shortest_chain_when_one_exists() {
    // A → B → C → D is the only chain. The "subgraph from A" view we used
    // to ship would happily include B and C but never connect them to D,
    // because `to_id` was discarded. This test pins the contract: when a
    // path exists, the function returns the actual sequence of IDs.
    let storage = create_test_storage();
    let a = ingest_id(&storage, "A — origin");
    let b = ingest_id(&storage, "B — relay 1");
    let c = ingest_id(&storage, "C — relay 2");
    let d = ingest_id(&storage, "D — destination");

    connect(&storage, &a, &b);
    connect(&storage, &b, &c);
    connect(&storage, &c, &d);

    let path = storage
        .find_path_between(&a, &d, 5)
        .expect("find_path_between should not error on a connected graph");

    assert_eq!(
        path.as_deref(),
        Some([a.clone(), b.clone(), c.clone(), d.clone()].as_slice()),
        "expected the full A→B→C→D chain, got {path:?}"
    );
}

#[test]
fn find_path_between_returns_none_when_no_chain_exists() {
    // A and C are in disjoint components — the function must not invent
    // a path. Returning the subgraph around A here (the old behavior)
    // misleads the UI into showing "results" that don't actually
    // connect the user's two memories.
    let storage = create_test_storage();
    let a = ingest_id(&storage, "A — island 1");
    let b = ingest_id(&storage, "B — island 1");
    let c = ingest_id(&storage, "C — island 2");
    let d = ingest_id(&storage, "D — island 2");
    connect(&storage, &a, &b);
    connect(&storage, &c, &d);

    assert_eq!(storage.find_path_between(&a, &d, 5).unwrap(), None);
}

#[test]
fn find_path_between_treats_edges_as_undirected_for_discovery() {
    // Stored edges are directed (source → target), but conceptually the
    // "are these memories connected?" question is symmetric — if I add
    // a "B follows A" link, the user expects exploring from A to B and
    // from B to A to both work. Mirrors `get_connections_for_memory`
    // which returns both directions.
    let storage = create_test_storage();
    let a = ingest_id(&storage, "A");
    let b = ingest_id(&storage, "B");
    connect(&storage, &a, &b);

    assert_eq!(
        storage.find_path_between(&b, &a, 3).unwrap().as_deref(),
        Some([b.clone(), a.clone()].as_slice()),
    );
}

#[test]
fn find_path_between_respects_max_depth() {
    // Same A→B→C→D as the happy path, but cap depth at 2 hops — the
    // function must say "no path" rather than walking the full chain.
    let storage = create_test_storage();
    let a = ingest_id(&storage, "A");
    let b = ingest_id(&storage, "B");
    let c = ingest_id(&storage, "C");
    let d = ingest_id(&storage, "D");
    connect(&storage, &a, &b);
    connect(&storage, &b, &c);
    connect(&storage, &c, &d);

    assert_eq!(storage.find_path_between(&a, &d, 2).unwrap(), None);
    assert!(storage.find_path_between(&a, &d, 3).unwrap().is_some());
}

#[test]
fn snapshot_round_trip_restores_memories_and_fts() {
    let dir = tempdir().unwrap();
    let source = Storage::new(Some(dir.path().join("source.db"))).unwrap();

    let first = source
        .ingest(IngestInput {
            content: "The staging cluster runs the blue deployment".to_string(),
            node_type: "fact".to_string(),
            tags: vec!["infra".to_string()],
            ..Default::default()
        })
        .unwrap();
    source
        .ingest(IngestInput {
            content: "Rollback of the blue deployment is a single command".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    let snapshot = dir.path().join("vestige-snapshot.db");
    source.backup_to(&snapshot).unwrap();
    assert!(snapshot.exists(), "backup must produce a file");
    drop(source);

    // A second store, as a user recovering onto a fresh machine would have.
    let target = Storage::new(Some(dir.path().join("target.db"))).unwrap();
    let report = target.restore_from_snapshot(&snapshot).unwrap();

    assert_eq!(report.nodes_imported, 2, "both memories must arrive");
    assert_eq!(report.nodes_in_snapshot, 2);
    assert_eq!(target.get_stats().unwrap().total_nodes, 2);
    assert_eq!(
        target.get_node(&first.id).unwrap().map(|n| n.content),
        Some("The staging cluster runs the blue deployment".to_string()),
        "content must survive the round trip verbatim"
    );

    // The FTS index is external-content, and INSERT OR REPLACE does not fire the delete
    // trigger — the import rebuilds it explicitly, so keyword search has to work here.
    let hits = target.keyword_search("blue deployment", 10, 0.0).unwrap();
    assert!(
        !hits.is_empty(),
        "restored memories must be searchable, not just present"
    );
}

#[test]
fn snapshot_restore_merges_instead_of_replacing() {
    let dir = tempdir().unwrap();
    let source = Storage::new(Some(dir.path().join("source.db"))).unwrap();
    let imported = source
        .ingest(IngestInput {
            content: "Memory that exists only in the snapshot".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();
    let snapshot = dir.path().join("snapshot.db");
    source.backup_to(&snapshot).unwrap();
    drop(source);

    let target = Storage::new(Some(dir.path().join("target.db"))).unwrap();
    target
        .ingest(IngestInput {
            content: "Memory that already lived in the target store".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    let report = target.restore_from_snapshot(&snapshot).unwrap();
    assert_eq!(report.nodes_imported, 1);
    assert_eq!(
        target.get_stats().unwrap().total_nodes,
        2,
        "import is a merge: the local memory must survive"
    );
    assert!(target.get_node(&imported.id).unwrap().is_some());
}

#[test]
fn snapshot_restore_rejects_a_newer_schema() {
    let dir = tempdir().unwrap();
    let source = Storage::new(Some(dir.path().join("source.db"))).unwrap();
    let snapshot = dir.path().join("snapshot.db");
    source.backup_to(&snapshot).unwrap();
    drop(source);

    // Pretend the snapshot was written by a future binary.
    {
        let conn = rusqlite::Connection::open(&snapshot).unwrap();
        conn.execute_batch("UPDATE schema_version SET version = 99;")
            .unwrap();
    }

    let target = Storage::new(Some(dir.path().join("target.db"))).unwrap();
    let err = target.restore_from_snapshot(&snapshot).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("newer than this build"),
        "a future snapshot must be refused with an explanation, got: {message}"
    );
}

/// Regression for the double-`BEGIN` bug: `begin_write_transaction` issued
/// `BEGIN IMMEDIATE` and then `unchecked_transaction()`, which sends a *second* `BEGIN`.
/// Every write through the helper therefore failed with "cannot start a transaction
/// within a transaction", and because the first `BEGIN` had already succeeded the
/// connection stayed inside an open transaction — later writes were invisible to readers
/// and were lost when the process exited. Consolidation (step 1: `apply_decay`) never ran.
#[test]
fn write_transaction_issues_exactly_one_begin_and_commits() {
    use super::helpers::begin_write_transaction;

    let dir = tempdir().unwrap();
    let storage = Storage::new(Some(dir.path().join("wal.db"))).unwrap();
    let node = storage
        .ingest(IngestInput {
            content: "before the transaction".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    {
        let mut writer = storage.writer.lock().unwrap();
        // A clean connection: one BEGIN IMMEDIATE, wrapped by rusqlite itself.
        let tx = begin_write_transaction(&mut writer)
            .expect("BEGIN IMMEDIATE must succeed once — a second BEGIN is the bug");
        tx.execute(
            "UPDATE knowledge_nodes SET content = ?1 WHERE id = ?2",
            rusqlite::params!["after the transaction", node.id],
        )
        .unwrap();
        tx.commit().unwrap();
    }

    // Visible to a reader immediately: with the bug the write sat in an uncommitted
    // transaction and this read returned the old content.
    assert_eq!(
        storage.get_node(&node.id).unwrap().map(|n| n.content),
        Some("after the transaction".to_string()),
        "a committed write must be visible to other connections"
    );
}

/// End-to-end version of the same regression: consolidation used to fail on its very
/// first step, so FSRS decay, dedup, dream and retention snapshots silently never ran.
#[test]
fn consolidation_runs_to_completion() {
    let dir = tempdir().unwrap();
    let storage = Storage::new(Some(dir.path().join("consolidate.db"))).unwrap();
    storage
        .ingest(IngestInput {
            content: "A memory that should survive a consolidation cycle".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    let result = storage
        .run_consolidation()
        .expect("consolidation must complete — its first step is a write transaction");

    assert_eq!(
        storage.get_stats().unwrap().total_nodes,
        1,
        "consolidation must never delete memories (auto-GC removed in 608d866)"
    );
    assert!(
        result.duration_ms >= 0,
        "the pipeline must report a completed run, not bail out at step 1"
    );
}

// ============================================================================
// 2026-09-19 core audit — medium/low findings
// ============================================================================

/// Regression: `keyword_search` sorted by `retention_strength` instead of the
/// FTS5 `rank`. The sanitizer emits a term query (`"alpha" OR "beta" OR ...`),
/// so a memory matching a single term but with retention 1.0 could outrank —
/// and, past the LIMIT, push out — a memory matching every term. The other two
/// keyword paths (`Storage::search`, `keyword_search_with_scores`) already
/// order by `rank`; this pins the third.
#[test]
fn keyword_search_orders_by_bm25_rank_not_retention() {
    let storage = create_test_storage();

    let matches_all = storage
        .ingest(IngestInput {
            content: "alpha beta gamma".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();
    let matches_one = storage
        .ingest(IngestInput {
            content: "alpha omega psi".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    // Make the weaker match the "hotter" memory — the exact shape that used to
    // put it first.
    {
        let writer = storage.writer.lock().unwrap();
        writer
            .execute(
                "UPDATE knowledge_nodes SET retention_strength = 1.0 WHERE id = ?1",
                rusqlite::params![matches_one.id],
            )
            .unwrap();
        writer
            .execute(
                "UPDATE knowledge_nodes SET retention_strength = 0.1 WHERE id = ?1",
                rusqlite::params![matches_all.id],
            )
            .unwrap();
    }

    let hits = storage
        .keyword_search("alpha beta gamma", 10, 0.0)
        .expect("keyword search must succeed");
    let ids: Vec<&str> = hits.iter().map(|n| n.id.as_str()).collect();

    assert!(
        ids.contains(&matches_all.id.as_str()) && ids.contains(&matches_one.id.as_str()),
        "both matches must be returned: {ids:?}"
    );
    assert_eq!(
        ids[0], matches_all.id,
        "bm25 rank must lead the ordering — retention is only a floor, got {ids:?}"
    );
}

/// Regression: the tag filter was `tags LIKE '%"<tag>%'`, which matched tag
/// *prefixes* (`code` matched `codebase`) and let `%`/`_` inside the tag act as
/// wildcards. `get_all_nodes_filtered`/`count_nodes_filtered` already use an
/// exact `json_each` match.
#[test]
fn get_nodes_by_type_and_tag_matches_exact_tags() {
    let storage = create_test_storage();

    let code = storage
        .ingest(IngestInput {
            content: "pattern tagged code".to_string(),
            node_type: "pattern".to_string(),
            tags: vec!["code".to_string()],
            ..Default::default()
        })
        .unwrap();
    let codebase = storage
        .ingest(IngestInput {
            content: "pattern tagged codebase".to_string(),
            node_type: "pattern".to_string(),
            tags: vec!["codebase".to_string()],
            ..Default::default()
        })
        .unwrap();

    let hits = storage
        .get_nodes_by_type_and_tag("pattern", Some("code"), 10)
        .unwrap();
    let ids: Vec<&str> = hits.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![code.id.as_str()],
        "prefix LIKE pulled in unrelated tags: {ids:?}"
    );
    assert!(!ids.contains(&codebase.id.as_str()));

    // `%` in a tag must be a literal, not a wildcard.
    let literal = storage
        .ingest(IngestInput {
            content: "pattern tagged 100%".to_string(),
            node_type: "pattern".to_string(),
            tags: vec!["100%".to_string()],
            ..Default::default()
        })
        .unwrap();
    storage
        .ingest(IngestInput {
            content: "pattern tagged 100x".to_string(),
            node_type: "pattern".to_string(),
            tags: vec!["100x".to_string()],
            ..Default::default()
        })
        .unwrap();

    let hits = storage
        .get_nodes_by_type_and_tag("pattern", Some("100%"), 10)
        .unwrap();
    let ids: Vec<&str> = hits.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![literal.id.as_str()],
        "wildcard in a tag leaked into the LIKE pattern: {ids:?}"
    );
}

/// V16 regression: `knowledge_au` fires on every UPDATE, so a retention-only
/// write (each search hit, every `apply_decay` batch) deleted and re-inserted
/// the FTS5 document. `total_changes()` counts rows written through triggers,
/// so one changed row means the FTS rewrite is gone — and the control block
/// recreates the old trigger shape to show it really was counted.
#[test]
fn metadata_only_update_does_not_rewrite_fts_document() {
    let storage = create_test_storage();
    let node = storage
        .ingest(IngestInput {
            content: "the quick brown fox".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    let retention_bump_changes = |storage: &Storage| -> u64 {
        let writer = storage.writer.lock().unwrap();
        let before = writer.total_changes();
        writer
            .execute(
                "UPDATE knowledge_nodes SET retention_strength = 0.42 WHERE id = ?1",
                rusqlite::params![node.id],
            )
            .unwrap();
        writer.total_changes() - before
    };

    assert_eq!(
        retention_bump_changes(&storage),
        1,
        "V16 trigger must not rewrite the FTS document for a metadata-only UPDATE"
    );

    // Content edits must still reach the index.
    storage
        .update_node_content(&node.id, "a completely different wording")
        .unwrap();
    assert!(
        storage
            .keyword_search("wording", 10, 0.0)
            .unwrap()
            .iter()
            .any(|n| n.id == node.id),
        "content changes must still refresh the FTS index"
    );

    // Control: the pre-V16 trigger (no column filter) does rewrite the document.
    {
        let writer = storage.writer.lock().unwrap();
        writer
            .execute_batch(
                "DROP TRIGGER knowledge_au;
                 CREATE TRIGGER knowledge_au AFTER UPDATE ON knowledge_nodes BEGIN
                     INSERT INTO knowledge_fts(knowledge_fts, rowid, id, content, tags)
                     VALUES ('delete', OLD.rowid, OLD.id, OLD.content, OLD.tags);
                     INSERT INTO knowledge_fts(rowid, id, content, tags)
                     VALUES (NEW.rowid, NEW.id, NEW.content, NEW.tags);
                 END;",
            )
            .unwrap();
    }
    assert!(
        retention_bump_changes(&storage) > 1,
        "control failed: the unfiltered trigger should still rewrite the FTS document"
    );
}

/// Regression: the HNSW sidecar meta only recorded `row_count`, so a memory
/// edit (same number of embeddings, different vector) passed validation and the
/// next boot loaded an index built from the OLD wording. The meta now carries
/// an embedding-state fingerprint.
#[test]
fn vector_index_sidecar_invalidated_by_embedding_rewrite() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let node_id = {
        let storage = Storage::new(Some(db_path.clone())).unwrap();
        let node = storage
            .ingest(IngestInput {
                content: "original wording about migration safety".to_string(),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap();
        storage.persist_vector_index().unwrap();
        node.id
    };

    // Boot 2: rewrite content (and therefore the vector) through the public edit
    // path — the row count stays the same.
    {
        let storage = Storage::new(Some(db_path.clone())).unwrap();
        assert_eq!(
            storage.vector_index_source(),
            VectorIndexSource::Loaded,
            "precondition: the sidecar must be considered valid before the edit"
        );
        storage
            .update_node_content(&node_id, "rewritten wording about vector staleness")
            .unwrap();
    }

    // Boot 3: same row count, different embeddings → the sidecar is stale.
    let storage3 = Storage::new(Some(db_path)).unwrap();
    assert_eq!(
        storage3.vector_index_source(),
        VectorIndexSource::Rebuilt,
        "an embedding rewrite must invalidate the sidecar even when row_count matches"
    );
}

/// GDPR erasure used to leave `insights.source_memories` (a JSON array with no
/// foreign key) pointing at the erased id — a reversible reference to personal
/// data.
#[test]
fn right_to_erasure_removes_insights_referencing_the_memory() {
    let storage = create_test_storage();
    let erased = ingest_id(&storage, "memory the subject asked us to forget");
    let kept = ingest_id(&storage, "unrelated memory");

    let save_insight = |id: &str, sources: Vec<String>| {
        storage
            .save_insight(&InsightRecord {
                id: id.to_string(),
                insight: "an emergent observation".to_string(),
                source_memories: sources,
                confidence: 0.9,
                novelty_score: 0.5,
                insight_type: "hidden_connection".to_string(),
                generated_at: Utc::now(),
                ..Default::default()
            })
            .unwrap();
    };
    save_insight("ins-touches-erased", vec![erased.clone(), kept.clone()]);
    save_insight("ins-unrelated", vec![kept.clone()]);

    storage.right_to_erasure(&erased).unwrap();

    let remaining: Vec<String> = storage
        .get_insights(50)
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert!(
        !remaining.contains(&"ins-touches-erased".to_string()),
        "an insight referencing the erased memory must be removed, got {remaining:?}"
    );
    assert!(
        remaining.contains(&"ins-unrelated".to_string()),
        "unrelated insights must survive, got {remaining:?}"
    );
}

/// The erasure sequence must be atomic: a failure half-way cannot leave the
/// node stripped of its embeddings/logs but still present. Dropping a table the
/// path must touch forces the failure.
#[test]
fn right_to_erasure_rolls_back_when_a_step_fails() {
    let storage = create_test_storage();
    let node = storage
        .ingest(IngestInput {
            content: "memory whose erasure will fail half-way".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    storage
        .writer
        .lock()
        .unwrap()
        .execute_batch("DROP TABLE insights;")
        .unwrap();

    assert!(
        storage.right_to_erasure(&node.id).is_err(),
        "the failing step must surface as an error"
    );
    assert!(
        storage.get_node(&node.id).unwrap().is_some(),
        "a failed erasure must not remove the node"
    );
    let embedding_rows: i64 = storage
        .reader
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM node_embeddings WHERE node_id = ?1",
            rusqlite::params![node.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        embedding_rows, 1,
        "the embedding must survive a rolled-back erasure"
    );
}

// ============================================================================
// GDPR bulk erasure — exact tag membership (gdpr-tag-exact)
//
// `erase_by_tag` built a LIKE pattern `%"<tag>%` with no closing quote and no
// wildcard escaping: erasing `code` also erased every `codebase`, and a tag
// containing `%`/`_` matched unrelated tags. Over-deletion on this path is
// irreversible, so membership is now an exact `json_each` match.
// ============================================================================

#[test]
fn erase_by_tag_removes_only_exact_tag_matches() {
    let storage = create_test_storage();

    let code = ingest_id(&storage, "memory tagged code");
    let codebase = ingest_id(&storage, "memory tagged codebase");
    storage
        .update_node_tags(&code, &["code".to_string()])
        .unwrap();
    storage
        .update_node_tags(&codebase, &["codebase".to_string()])
        .unwrap();

    let (erased, _) = storage.erase_by_tag("code").unwrap();
    assert_eq!(erased, 1, "only the exact `code` tag may be erased");
    assert!(
        storage.get_node(&code).unwrap().is_none(),
        "the memory tagged `code` must be erased"
    );
    assert!(
        storage.get_node(&codebase).unwrap().is_some(),
        "`codebase` is not `code` — erasing one must not erase the other"
    );
}

#[test]
fn erase_by_tag_treats_like_wildcards_literally() {
    let storage = create_test_storage();

    let percent = ingest_id(&storage, "memory tagged 100%");
    let prefix = ingest_id(&storage, "memory tagged 100x");
    storage
        .update_node_tags(&percent, &["100%".to_string()])
        .unwrap();
    storage
        .update_node_tags(&prefix, &["100x".to_string()])
        .unwrap();

    let (erased, _) = storage.erase_by_tag("100%").unwrap();
    assert_eq!(erased, 1, "`%` must match only the literal percent tag");
    assert!(
        storage.get_node(&percent).unwrap().is_none(),
        "the literal `100%` tag must still be erasable"
    );
    assert!(
        storage.get_node(&prefix).unwrap().is_some(),
        "`%` leaked into the match as a wildcard and erased an unrelated tag"
    );

    // `_` is the single-character wildcard in LIKE. No stored tag can hold one
    // (`normalize_tags` folds it to `-`), so the query must not treat it as a
    // wildcard either.
    let underscore_victim = ingest_id(&storage, "memory tagged xyz");
    storage
        .update_node_tags(&underscore_victim, &["xyz".to_string()])
        .unwrap();
    let (erased, _) = storage.erase_by_tag("x_z").unwrap();
    assert_eq!(erased, 0, "`_` must not match an arbitrary character");
    assert!(
        storage.get_node(&underscore_victim).unwrap().is_some(),
        "`_` leaked into the match as a wildcard and erased an unrelated tag"
    );

    // A caller passing a non-canonical surface form still hits the stored tag.
    let shouty = ingest_id(&storage, "memory tagged code");
    storage
        .update_node_tags(&shouty, &["code".to_string()])
        .unwrap();
    let (erased, _) = storage.erase_by_tag(" CODE ").unwrap();
    assert_eq!(
        erased, 1,
        "the query must be canonicalized like stored tags"
    );
    assert!(storage.get_node(&shouty).unwrap().is_none());
}

// ============================================================================
// Vector index lifecycle — content edit and delete (index-consistency)
//
// `update_node_content` evicted the old vector *before* re-embedding, so an
// unavailable embedder dropped the memory out of semantic search for good
// while reporting success; `delete_node` swallowed `index.remove` and left the
// deleted UUID in the on-disk sidecar.
// ============================================================================

/// A failed re-embed must not cost the memory its place in the index, and must
/// not be reported as success. Empty text is rejected by the embedder before
/// any model access, so the failure branch is reachable without an ONNX model.
#[test]
fn content_update_keeps_previous_vector_when_embedding_fails() {
    use crate::embeddings::{EMBEDDING_DIMENSIONS, Embedding};

    let storage = create_test_storage();
    let node = storage
        .ingest(IngestInput {
            content: "original wording".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    // Seed a known vector through the production persistence path so this test
    // does not depend on the embedding model being downloadable.
    let previous = Embedding::new(vec![0.5f32; EMBEDDING_DIMENSIONS]);
    storage
        .store_embedding_for_node(&node.id, &previous)
        .unwrap();
    let stored_before: Vec<u8> = storage
        .reader
        .lock()
        .unwrap()
        .query_row(
            "SELECT embedding FROM node_embeddings WHERE node_id = ?1",
            rusqlite::params![node.id],
            |row| row.get(0),
        )
        .unwrap();

    let err = storage
        .update_node_content(&node.id, "")
        .expect_err("a re-embed that never happened must not be reported as success");
    assert!(
        err.to_string().contains("embed"),
        "the error must name the embedding failure, got: {err}"
    );

    assert!(
        storage.vector_index.lock().unwrap().contains(&node.id),
        "the previous vector must stay in the index when re-embedding fails"
    );
    assert_eq!(
        storage.get_node(&node.id).unwrap().unwrap().content,
        "original wording",
        "content whose vector could not be computed must not be written"
    );
    let stored_after: Vec<u8> = storage
        .reader
        .lock()
        .unwrap()
        .query_row(
            "SELECT embedding FROM node_embeddings WHERE node_id = ?1",
            rusqlite::params![node.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        stored_after, stored_before,
        "the persisted vector must still match the stored content"
    );
}

/// A delete must leave the on-disk sidecar consistent with the database. When
/// it did not, the sidecar kept the deleted UUID and the next process could
/// load it whenever the row count happened to line up again.
#[test]
fn deleting_a_memory_rewrites_the_index_sidecar() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("delete.db");

    let (deleted_id, kept_id) = {
        let storage = Storage::new(Some(db_path.clone())).unwrap();
        let deleted = storage
            .ingest(IngestInput {
                content: "memory that gets deleted".to_string(),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap();
        let kept = storage
            .ingest(IngestInput {
                content: "memory that survives".to_string(),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap();
        storage.persist_vector_index().unwrap();
        assert!(storage.delete_node(&deleted.id).unwrap());
        (deleted.id, kept.id)
    };

    let reloaded = Storage::new(Some(db_path)).unwrap();
    assert_eq!(
        reloaded.vector_index_source(),
        VectorIndexSource::Loaded,
        "the delete must leave a sidecar that is valid, not one that every later boot rejects"
    );
    let index = reloaded.vector_index.lock().unwrap();
    assert!(
        !index.contains(&deleted_id),
        "the deleted UUID came back from the persisted sidecar"
    );
    assert!(
        index.contains(&kept_id),
        "the surviving memory must still be indexed"
    );
}

// ============================================================================
// Physical erasure — secure_delete, WAL checkpoint, FTS merge (ephemeral-data)
//
// The erasure path claimed the data was gone while the raw file still held it:
// `secure_delete` was off, the FTS5 tombstone left the terms in
// `knowledge_fts_data`, and nothing checkpointed the WAL.
// ============================================================================

#[test]
fn writer_connection_runs_with_secure_delete() {
    let storage = create_test_storage();
    let enabled: i64 = storage
        .writer
        .lock()
        .unwrap()
        .query_row("PRAGMA secure_delete", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        enabled, 1,
        "without secure_delete, freed cells keep the deleted text readable in the file"
    );
}

/// The marker is lowercase and stem-free on purpose: `knowledge_fts` tokenizes
/// with porter, so a marker that stems differently would never appear verbatim
/// in the index and the precondition below would be vacuous.
const ERASURE_MARKER: &str = "zqxjvmrk9c2f";

fn bytes_contain(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// A missing file counts as empty: a truncated WAL is deleted on some
/// platforms instead of being left at zero length.
fn read_file_bytes(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_default()
}

#[test]
fn erasure_scrubs_the_content_from_database_and_wal_bytes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("erasure.db");
    let wal_path = dir.path().join("erasure.db-wal");

    let storage = Storage::new(Some(db_path.clone())).unwrap();
    let node = storage
        .ingest(IngestInput {
            content: format!("{ERASURE_MARKER} {}", "erasable padding ".repeat(200)),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    // Flush first so the precondition describes the on-disk state, not the WAL.
    storage.wal_checkpoint().unwrap();
    assert!(
        bytes_contain(&read_file_bytes(&db_path), ERASURE_MARKER),
        "precondition: the content must be readable on disk before the erasure"
    );

    storage.right_to_erasure(&node.id).unwrap();

    assert!(
        !bytes_contain(&read_file_bytes(&db_path), ERASURE_MARKER),
        "erased content is still readable in the raw database file"
    );
    assert!(
        !bytes_contain(&read_file_bytes(&wal_path), ERASURE_MARKER),
        "erased content is still readable in the write-ahead log"
    );
}

// ============================================================================
// At-rest encryption — fail closed (encryption-fail-closed)
//
// A missing `VESTIGE_ENCRYPTION_KEY` used to be ignored, so a process built or
// configured for encryption happily created a plaintext database.
// ============================================================================

/// Environment stub for the encryption-policy tests: the getter is a parameter
/// exactly so these branches can be exercised without mutating process-wide
/// env vars (which would race with every other test in the binary).
fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'static {
    let map: std::collections::HashMap<String, String> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    move |name: &str| map.get(name).cloned()
}

#[test]
fn encryption_config_requires_a_nonempty_key_when_encryption_is_requested() {
    assert_eq!(
        Storage::encryption_config(&env_from(&[(ENCRYPTION_KEY_ENV, "s3cret")])),
        EncryptionConfig::Key("s3cret".to_string()),
        "a passphrase on its own is a request for encryption"
    );
    assert_eq!(
        Storage::encryption_config(&env_from(&[(REQUIRE_ENCRYPTION_ENV, "yes")])),
        EncryptionConfig::RequestedWithoutKey,
        "the explicit switch without a passphrase must be refused upstream"
    );
    assert_eq!(
        Storage::encryption_config(&env_from(&[
            (REQUIRE_ENCRYPTION_ENV, "true"),
            (ENCRYPTION_KEY_ENV, "k"),
        ])),
        EncryptionConfig::Key("k".to_string())
    );
    for blank in ["", "   ", "\t"] {
        assert_eq!(
            Storage::encryption_config(&env_from(&[(ENCRYPTION_KEY_ENV, blank)])),
            EncryptionConfig::Plaintext,
            "a blank passphrase is not a passphrase (got {blank:?})"
        );
    }
    assert_eq!(
        Storage::encryption_config(&env_from(&[])),
        EncryptionConfig::Plaintext,
        "no key and no switch must stay plaintext-with-warning, not an error"
    );
}

#[test]
fn encryption_config_only_honours_truthy_switches() {
    for value in ["1", "true", "TRUE", "Yes", " on "] {
        assert_eq!(
            Storage::encryption_config(&env_from(&[(REQUIRE_ENCRYPTION_ENV, value)])),
            EncryptionConfig::RequestedWithoutKey,
            "{REQUIRE_ENCRYPTION_ENV}={value:?} must count as a request"
        );
    }
    for value in ["0", "false", "no", "off", "maybe", "2"] {
        assert_eq!(
            Storage::encryption_config(&env_from(&[(REQUIRE_ENCRYPTION_ENV, value)])),
            EncryptionConfig::Plaintext,
            "{REQUIRE_ENCRYPTION_ENV}={value:?} must not count as a request"
        );
    }
}

#[test]
fn explicit_encryption_request_without_key_is_refused() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let err = Storage::configure_connection(&conn, &EncryptionConfig::RequestedWithoutKey, true)
        .expect_err("an explicit encryption request must never fall back to plaintext");
    let message = err.to_string();
    assert!(
        message.contains(ENCRYPTION_KEY_ENV),
        "the refusal must name the missing variable: {message}"
    );
}

/// Without SQLCipher compiled in there is no way to apply the passphrase, so a
/// present key must stop the process rather than be dropped.
#[cfg(not(feature = "encryption"))]
#[test]
fn key_without_sqlcipher_feature_is_refused() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let err = Storage::configure_connection(&conn, &EncryptionConfig::Key("s3cret".into()), true)
        .expect_err("a passphrase without SQLCipher must not produce a plaintext database");
    let message = err.to_string();
    assert!(
        message.contains("encryption"),
        "the refusal must name the missing cargo feature: {message}"
    );
    assert!(
        !message.contains("s3cret"),
        "the passphrase must never be echoed back: {message}"
    );
}

/// An encrypted database opened without its key is indistinguishable from
/// garbage (`SQLITE_NOTADB`); the open must fail closed with the actionable
/// reason instead of a bare sqlite error from the PRAGMA batch.
#[test]
fn opening_a_database_that_needs_a_key_is_refused() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("encrypted.db");
    // Stand-in for a SQLCipher file: bytes that are not a SQLite header.
    std::fs::write(&db_path, vec![0xA5u8; 4096]).unwrap();

    match Storage::new(Some(db_path)) {
        Err(StorageError::Init(message)) => assert!(
            message.contains(ENCRYPTION_KEY_ENV),
            "the refusal must tell the operator how to supply the key: {message}"
        ),
        Err(other) => panic!("expected a fail-closed Init error, got {other:?}"),
        Ok(_) => panic!("opening a file that needs a key must not succeed"),
    }
}

// ============================================================================
// Embedding-space drift — sidecar validation and the startup signal
//
// An upgrade of the embedding function leaves every row untouched, so a
// sidecar written in the previous space still passed the row-count and
// embedding-state checks and was loaded as if it were current: queries from
// the new space were compared against documents from the old one.
// ============================================================================

#[test]
fn vector_index_sidecar_from_another_embedding_space_is_rebuilt() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("space.db");

    let node_id = {
        let storage = Storage::new(Some(db_path.clone())).unwrap();
        let node = storage
            .ingest(IngestInput {
                content: "memory embedded in the current space".to_string(),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap();
        storage.persist_vector_index().unwrap();
        node.id
    };

    let meta_path = {
        let storage = Storage::new(Some(db_path.clone())).unwrap();
        storage.vector_index_meta_path()
    };
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    let written_space = meta
        .get("embedding_space")
        .and_then(|value| value.as_str())
        .expect("the sidecar meta must record the embedding space it was written in")
        .to_string();
    assert!(
        !written_space.is_empty(),
        "an empty space id would make every sidecar look stale"
    );

    // Same rows, same row count, same embedding-state stamp — only the space
    // differs, exactly as after an upgrade of the embedding function.
    meta["embedding_space"] = serde_json::json!("nomic-embed-text-v1.5|v1|d384|pre-v2");
    std::fs::write(&meta_path, meta.to_string()).unwrap();

    let reloaded = Storage::new(Some(db_path)).unwrap();
    assert_eq!(
        reloaded.vector_index_source(),
        VectorIndexSource::Rebuilt,
        "a sidecar written in another embedding space must be rebuilt from SQLite, not loaded"
    );
    assert!(
        reloaded.vector_index.lock().unwrap().contains(&node_id),
        "the rebuild must still index the store's vectors"
    );
}

/// Rejecting the sidecar fixes the index, but the stored vectors stay in the
/// old space until they are recomputed. Startup must say so — with the count
/// and the remedy — and must not re-embed anything by itself.
#[test]
fn vectors_from_an_older_embedding_space_produce_an_actionable_warning() {
    use crate::embeddings::{EMBEDDING_DIMENSIONS, Embedding};

    let storage = create_test_storage();
    let node = storage
        .ingest(IngestInput {
            content: "memory with a recorded embedding".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();
    // Written through the production path, so the tag is whatever this build
    // records for a current vector (independent of the ONNX model being there).
    storage
        .store_embedding_for_node(
            &node.id,
            &Embedding::new(vec![0.25f32; EMBEDDING_DIMENSIONS]),
        )
        .unwrap();

    assert_eq!(
        storage.stale_embedding_space_warning().unwrap(),
        None,
        "a store whose vectors all carry the current tag must stay quiet"
    );

    // What a pre-upgrade store looks like: same row, same vector, an older
    // provenance tag.
    storage
        .writer
        .lock()
        .unwrap()
        .execute(
            "UPDATE knowledge_nodes SET embedding_model = 'nomic-embed-text-v1.5' WHERE id = ?1",
            rusqlite::params![node.id],
        )
        .unwrap();

    let message = storage
        .stale_embedding_space_warning()
        .unwrap()
        .expect("a vector tagged in an older space must be reported");
    assert!(
        message.contains('1'),
        "the number of affected memories must be named: {message}"
    );
    assert!(
        message.contains("regenerate_embeddings"),
        "the remedy must be named: {message}"
    );
    assert!(
        message.contains("force: true"),
        "the remedy must include the flag that makes it effective: {message}"
    );
    assert!(
        message.contains(crate::embeddings::embedding_model_tag()),
        "the current space must be named so the user can tell what is old: {message}"
    );
}
