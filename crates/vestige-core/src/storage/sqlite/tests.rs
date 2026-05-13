//! Storage layer integration tests.

use chrono::{Duration, Utc};
use tempfile::tempdir;

use crate::fsrs::Rating;
use crate::memory::IngestInput;

use super::Storage;
use super::records::DreamHistoryRecord;

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
