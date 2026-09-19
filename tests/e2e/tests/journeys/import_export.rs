//! # Import/Export Journey Tests
//!
//! Tests the data portability features that allow users to backup, migrate,
//! and share their memory data. This ensures users have control over their
//! data and can move between systems.
//!
//! ## User Journey
//!
//! 1. User builds up memories over time
//! 2. User exports memories for backup or migration
//! 3. User imports memories on new system or from backup
//! 4. User shares relevant memories with teammates
//! 5. User merges memories from multiple sources
//!
//! ## What ships, and what is tested here
//!
//! There are exactly two portability paths in the product:
//!
//! * **SQLite snapshot** — `Storage::backup_to` (`VACUUM INTO`) writes a full
//!   copy of the store and `Storage::restore_from_snapshot` merges it back into
//!   a live store (ids, review state and vectors included).
//! * **JSON export** — the `export` MCP tool writes `Vec<KnowledgeNode>` as
//!   JSON (`crates/vestige-mcp/src/tools/maintenance/export.rs:160-171`) and
//!   the `restore` MCP tool reads it back as objects carrying
//!   `content` / `nodeType` / `tags` / `source`
//!   (`crates/vestige-mcp/src/tools/restore.rs:49-57`).
//!
//! The two contract tests below pin the JSON *shape* the two MCP halves agree
//! on (they can break independently, in different crates). The
//! [`storage_journeys`] module at the bottom drives both paths end to end on
//! real stores — export from one data directory, import into a fresh one, and
//! assert the imported memory is *searchable there*.

use serde::{Deserialize, Serialize};
use vestige_core::KnowledgeNode;

/// The four fields the shipped `restore` tool reads out of a JSON export.
///
/// Mirrors `MemoryBackup` (`crates/vestige-mcp/src/tools/restore.rs:49-57`),
/// which is private to the MCP crate. The point of the mirror is that it is
/// deliberately *narrower* than a `KnowledgeNode`: extra keys must be ignored
/// (no `deny_unknown_fields`) or the shipped export could not be restored at
/// all. `MemoryBackup::node_type` also accepts a snake_case `node_type` alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestorableMemory {
    content: String,
    #[serde(alias = "node_type")]
    node_type: Option<String>,
    tags: Option<Vec<String>>,
    source: Option<String>,
}

/// Build a real `KnowledgeNode` — the type the shipped `export` tool
/// serialises.
fn node(content: &str, node_type: &str, tags: &[&str]) -> KnowledgeNode {
    KnowledgeNode {
        content: content.to_string(),
        node_type: node_type.to_string(),
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        source: Some("export-contract".to_string()),
        ..Default::default()
    }
}

// ============================================================================
// CONTRACT 1: the exported JSON carries the fields the restore tool reads
// ============================================================================

/// The shipped `export` tool writes full `KnowledgeNode` values; the shipped
/// `restore` tool reads only four of their fields. This pins that contract:
/// rename `content`, `node_type`, `tags` or `source`'s wire name and a real
/// backup silently stops restoring.
#[test]
fn test_export_json_shape_carries_the_fields_restore_reads() {
    let exported = vec![
        node(
            "Rust ownership ensures memory safety without a garbage collector.",
            "concept",
            &["rust", "memory"],
        ),
        node(
            "Decision: SQLite WAL mode is used for concurrent reads.",
            "decision",
            &["storage"],
        ),
    ];

    // Exactly what `execute_export` writes: `serde_json::to_writer_pretty`.
    let json = serde_json::to_string_pretty(&exported).expect("serialisation must succeed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("the export must be JSON");
    let array = parsed
        .as_array()
        .expect("the export format is an array of memories");

    assert_eq!(array.len(), 2);
    for (entry, original) in array.iter().zip(&exported) {
        for key in ["content", "nodeType", "tags", "source"] {
            assert!(
                entry.get(key).is_some(),
                "the exported object must carry `{key}` for restore to read; got {entry}"
            );
        }
        // The narrow reader the restore tool uses must accept the full object.
        let restorable: RestorableMemory =
            serde_json::from_str(&entry.to_string()).expect("restore must parse an export entry");
        assert_eq!(restorable.content, original.content);
        assert_eq!(
            restorable.node_type.as_deref(),
            Some(original.node_type.as_str())
        );
        assert_eq!(restorable.tags.as_deref(), Some(original.tags.as_slice()));
        assert_eq!(restorable.source.as_deref(), original.source.as_deref());
    }
}

// ============================================================================
// CONTRACT 2: the export format round-trips as the node type itself
// ============================================================================

/// A memory exported and re-parsed as a `KnowledgeNode` must keep content,
/// type and tags. This is the read side of the shipped format: if the wire
/// names drift, this fails here rather than in a user's restore.
#[test]
fn test_export_json_round_trips_through_the_node_type() {
    let original = node(
        "FSRS-6 schedules the next review from retrievability.",
        "fact",
        &["fsrs", "scheduling"],
    );

    let json = serde_json::to_string(&original).expect("serialisation must succeed");
    let restored: KnowledgeNode =
        serde_json::from_str(&json).expect("deserialisation must succeed");

    assert_eq!(restored.content, original.content);
    assert_eq!(restored.node_type, original.node_type);
    assert_eq!(restored.tags, original.tags);
    assert_eq!(restored.source, original.source);
}

// ============================================================================
// REAL STORAGE JOURNEYS (two data dirs, real HTTP-free round trips)
// ============================================================================

mod storage_journeys {
    use std::path::Path;

    use tempfile::TempDir;
    use vestige_core::memory::IngestInput;
    use vestige_core::{KnowledgeNode, Rating};
    use vestige_e2e_tests::harness::{TestDatabaseManager, enable_mock_embeddings};

    /// Build an `IngestInput` (the struct carries defaults for the rest).
    fn memory(content: &str, node_type: &str, tags: &[&str]) -> IngestInput {
        IngestInput {
            content: content.to_string(),
            node_type: node_type.to_string(),
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            source: Some("import-export-journey".to_string()),
            ..Default::default()
        }
    }

    fn reopen(db_path: &Path) -> TestDatabaseManager {
        TestDatabaseManager::new_at_path(db_path.to_path_buf())
    }

    fn found(hits: &[KnowledgeNode], id: &str) -> bool {
        hits.iter().any(|n| n.id == id)
    }

    /// `backup_to` → fresh data dir → `restore_from_snapshot`: content, tags,
    /// FSRS state and vectors must all arrive, and the imported memory must be
    /// retrievable *in the new store* — the round trip that used to be
    /// impossible (the snapshot was a dead end until `restore_from_snapshot`
    /// existed).
    #[test]
    fn test_journey_snapshot_backup_restores_into_a_fresh_store() {
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let source_path = dir.path().join("source").join("vestige.db");
        let target_path = dir.path().join("target").join("vestige.db");
        std::fs::create_dir_all(source_path.parent().expect("parent"))
            .expect("create source data dir");
        std::fs::create_dir_all(target_path.parent().expect("parent"))
            .expect("create target data dir");
        let snapshot = dir.path().join("source").join("backup.db");

        // ---- Source store: two memories, one of them reviewed ---------------
        let source = TestDatabaseManager::new_at_path(source_path);
        let decision = source
            .storage
            .ingest(memory(
                "Consolidation only reports low retention candidates; it never deletes.",
                "decision",
                &["consolidation", "safety"],
            ))
            .expect("ingest must succeed");
        let recipe = source
            .storage
            .ingest(memory(
                "Sourdough starter hydration ratios for bread.",
                "note",
                &["baking"],
            ))
            .expect("ingest must succeed");
        let reviewed = source
            .storage
            .mark_reviewed(&decision.id, Rating::Good)
            .expect("mark_reviewed must succeed");

        source
            .storage
            .backup_to(&snapshot)
            .expect("VACUUM INTO backup must succeed");
        assert!(
            snapshot.is_file(),
            "the backup must leave a snapshot file at {snapshot:?}"
        );

        // ---- Target store: a fresh data dir that never saw this data --------
        let target = TestDatabaseManager::new_at_path(target_path.clone());
        assert!(target.is_empty(), "the target store must start empty");
        let report = target
            .storage
            .restore_from_snapshot(&snapshot)
            .expect("restore_from_snapshot must succeed");
        assert_eq!(
            report.nodes_imported, 2,
            "every memory in the snapshot must be merged: {report:?}"
        );
        assert_eq!(report.nodes_in_snapshot, 2);
        assert_eq!(
            report.embeddings_imported, 2,
            "the vectors must travel with the snapshot (same embedding profile): {report:?}"
        );
        assert!(
            report.live_schema_version >= report.snapshot_schema_version,
            "a snapshot from this build may not claim a newer schema: {report:?}"
        );

        // ---- State, not just content ----------------------------------------
        let restored = target
            .storage
            .get_node(&decision.id)
            .expect("get_node must not error")
            .expect("the imported memory must exist in the target store");
        assert_eq!(restored.content, decision.content);
        assert_eq!(restored.node_type, "decision");
        assert_eq!(restored.tags, vec!["consolidation", "safety"]);
        assert_eq!(restored.source, decision.source);
        assert_eq!(
            restored.reps, reviewed.reps,
            "the review count must travel with the snapshot"
        );
        assert_eq!(
            restored.storage_strength, reviewed.storage_strength,
            "the FSRS storage strength must travel with the snapshot"
        );
        assert!(
            restored.has_embedding.unwrap_or(false),
            "an imported memory whose vector travelled must report an embedding"
        );

        // ---- Searchable *there* ---------------------------------------------
        let keyword = target
            .storage
            .keyword_search("consolidation", 5, 0.0)
            .expect("keyword_search must succeed");
        assert!(
            found(&keyword, &decision.id),
            "the FTS5 index must be rebuilt for imported rows, got {:?}",
            keyword.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        let (kw, sem) = vestige_core::default_hybrid_weights();
        let hybrid = target
            .storage
            .hybrid_search("bread baking hydration", 5, kw, sem)
            .expect("hybrid_search must succeed");
        assert!(
            hybrid.iter().any(|r| r.node.id == recipe.id),
            "an imported vector must be usable by semantic search, got {:?}",
            hybrid
                .iter()
                .map(|r| (&r.node.id, r.combined_score))
                .collect::<Vec<_>>()
        );

        // ---- Durable, and the source is untouched ---------------------------
        drop(target);
        let restarted = reopen(&target_path);
        assert_eq!(restarted.node_count(), 2);
        let after_restart = restarted
            .storage
            .keyword_search("sourdough", 5, 0.0)
            .expect("keyword_search must succeed");
        assert!(
            found(&after_restart, &recipe.id),
            "imported rows must survive a restart of the target store"
        );

        assert_eq!(
            source.node_count(),
            2,
            "a backup is a read; the source store must be unchanged"
        );
        let source_hits = source
            .storage
            .keyword_search("sourdough", 5, 0.0)
            .expect("keyword_search must succeed");
        assert!(found(&source_hits, &recipe.id));
    }

    /// Importing into a store that already has memories must merge (by id), not
    /// replace, and re-importing the same snapshot must be idempotent.
    #[test]
    fn test_journey_restore_merges_without_duplicating_or_losing_local_memories() {
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let source_path = dir.path().join("source.db");
        let target_path = dir.path().join("target.db");
        let snapshot = dir.path().join("backup.db");

        let source = TestDatabaseManager::new_at_path(source_path);
        let imported = source
            .storage
            .ingest(memory(
                "Spreading activation walks the knowledge graph from a seed memory.",
                "concept",
                &["graph"],
            ))
            .expect("ingest must succeed");
        source
            .storage
            .backup_to(&snapshot)
            .expect("backup_to must succeed");
        drop(source);

        // The target already holds a memory of its own.
        let target = TestDatabaseManager::new_at_path(target_path.clone());
        let local = target
            .storage
            .ingest(memory(
                "Local memory: the dashboard listens on port 3927 by default.",
                "fact",
                &["dashboard"],
            ))
            .expect("ingest must succeed");
        assert_eq!(target.node_count(), 1);

        let report = target
            .storage
            .restore_from_snapshot(&snapshot)
            .expect("restore_from_snapshot must succeed");
        assert_eq!(report.nodes_imported, 1);
        assert_eq!(
            target.node_count(),
            2,
            "restore must merge, not replace: the local memory must survive"
        );
        let local_after = target
            .storage
            .get_node(&local.id)
            .expect("get_node must not error")
            .expect("the local memory must still exist");
        assert_eq!(local_after.content, local.content);

        // Re-importing the same snapshot must not duplicate rows: the merge is
        // keyed by id (INSERT OR REPLACE).
        let again = target
            .storage
            .restore_from_snapshot(&snapshot)
            .expect("repeat restore_from_snapshot must succeed");
        assert_eq!(again.nodes_imported, 1);
        assert_eq!(
            target.node_count(),
            2,
            "a second import of the same snapshot must be idempotent"
        );

        drop(target);
        let restarted = reopen(&target_path);
        assert_eq!(restarted.node_count(), 2);
        let hits = restarted
            .storage
            .keyword_search("spreading activation", 5, 0.0)
            .expect("keyword_search must succeed");
        assert!(
            found(&hits, &imported.id),
            "the merged memory must be searchable after a restart"
        );
    }

    /// The JSON path: exactly the bytes the `export` tool writes, re-read by the
    /// shape the `restore` tool accepts, ingested into a fresh data dir and then
    /// searched there.
    ///
    /// Unlike the snapshot path this one re-ingests, so ids and FSRS state are
    /// *not* preserved (the reader carries four fields). That is asserted, not
    /// assumed: a user migrating with JSON must know they lose review history.
    #[test]
    fn test_journey_json_export_reimports_into_a_fresh_store() {
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let source_path = dir.path().join("source.db");
        let target_path = dir.path().join("target.db");
        let export_path = dir.path().join("memories.json");

        // ---- Export: what `execute_export` writes ---------------------------
        let source = TestDatabaseManager::new_at_path(source_path);
        let exported_node = source
            .storage
            .ingest(memory(
                "Vestige keeps FSRS-6 review state per memory in SQLite.",
                "fact",
                &["fsrs", "storage"],
            ))
            .expect("ingest must succeed");
        let nodes = source
            .storage
            .get_all_nodes(100, 0)
            .expect("get_all_nodes must succeed");
        assert_eq!(nodes.len(), 1, "test setup: exactly one memory to export");
        let json = serde_json::to_string_pretty(&nodes).expect("export serialisation");
        std::fs::write(&export_path, json).expect("the export file must be written");
        drop(source);

        // ---- Import into a fresh data dir -----------------------------------
        let raw = std::fs::read_to_string(&export_path).expect("read the export");
        let restorable: Vec<super::RestorableMemory> =
            serde_json::from_str(&raw).expect("the export must parse in the restore shape");
        assert_eq!(restorable.len(), 1);

        let target = TestDatabaseManager::new_at_path(target_path.clone());
        assert!(target.is_empty(), "the target store must start empty");
        for entry in &restorable {
            target
                .storage
                .ingest(IngestInput {
                    content: entry.content.clone(),
                    node_type: entry
                        .node_type
                        .clone()
                        .unwrap_or_else(|| "fact".to_string()),
                    tags: entry.tags.clone().unwrap_or_default(),
                    source: entry.source.clone(),
                    ..Default::default()
                })
                .expect("re-ingest must succeed");
        }
        assert_eq!(target.node_count(), 1);

        // The JSON path re-ingests, so the id changes; only the four carried
        // fields survive. Pin that rather than leave it implicit.
        assert!(
            target
                .storage
                .get_node(&exported_node.id)
                .expect("get_node must not error")
                .is_none(),
            "the JSON restore path re-ingests: the original id is not preserved"
        );

        let hits = target
            .storage
            .keyword_search("FSRS-6 review state", 5, 0.0)
            .expect("keyword_search must succeed");
        assert_eq!(
            hits.len(),
            1,
            "the re-imported memory must be searchable in the fresh store"
        );
        assert_eq!(hits[0].tags, vec!["fsrs", "storage"]);
        assert_eq!(hits[0].source.as_deref(), Some("import-export-journey"));

        drop(target);
        let restarted = reopen(&target_path);
        let after_restart = restarted
            .storage
            .keyword_search("FSRS-6 review state", 5, 0.0)
            .expect("keyword_search must succeed");
        assert_eq!(
            after_restart.len(),
            1,
            "the re-imported memory must survive a restart of the target store"
        );
    }
}
