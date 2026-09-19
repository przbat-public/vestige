//! Storage layer integration tests.

use chrono::{Duration, Utc};
use tempfile::tempdir;

use crate::fsrs::Rating;
use crate::memory::IngestInput;

use super::Storage;
use super::init::VectorIndexSource;
use super::records::{ConnectionRecord, DreamHistoryRecord};

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
