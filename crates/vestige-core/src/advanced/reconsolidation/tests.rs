//! Tests for the reconsolidation manager.

use chrono::Duration;

use super::constants::{DEFAULT_LABILE_WINDOW_SECS, MAX_MODIFICATIONS_PER_WINDOW};
use super::context::{AccessContext, AccessTrigger};
use super::labile::{MemorySnapshot, Modification, RelationshipType};
use super::manager::ReconsolidationManager;

use super::*;

fn make_snapshot() -> MemorySnapshot {
    MemorySnapshot::capture(
        "Test content".to_string(),
        vec!["test".to_string()],
        0.8,
        5.0,
        0.9,
        vec![],
    )
}

#[test]
fn test_manager_new() {
    let manager = ReconsolidationManager::new();
    assert!(manager.is_enabled());
    assert_eq!(
        manager.get_labile_window(),
        Duration::seconds(DEFAULT_LABILE_WINDOW_SECS)
    );
}

#[test]
fn test_mark_labile() {
    let mut manager = ReconsolidationManager::new();
    let snapshot = make_snapshot();

    manager.mark_labile("mem-1", snapshot);

    assert!(manager.is_labile("mem-1"));
    assert!(!manager.is_labile("mem-2")); // Not marked
}

#[test]
fn test_apply_modification() {
    let mut manager = ReconsolidationManager::new();
    let snapshot = make_snapshot();

    manager.mark_labile("mem-1", snapshot);

    let success = manager.apply_modification(
        "mem-1",
        Modification::AddTag {
            tag: "new-tag".to_string(),
        },
    );

    assert!(success);
    assert_eq!(manager.get_stats().total_modifications, 1);
}

#[test]
fn test_apply_modification_not_labile() {
    let mut manager = ReconsolidationManager::new();

    // Try to modify a memory that's not labile
    let success = manager.apply_modification(
        "mem-1",
        Modification::AddTag {
            tag: "new-tag".to_string(),
        },
    );

    assert!(!success);
}

#[test]
fn test_reconsolidate() {
    let mut manager = ReconsolidationManager::new();
    let snapshot = make_snapshot();

    manager.mark_labile("mem-1", snapshot);
    manager.apply_modification(
        "mem-1",
        Modification::AddTag {
            tag: "new-tag".to_string(),
        },
    );

    let result = manager.reconsolidate("mem-1");

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(result.was_modified);
    assert_eq!(result.change_summary.tags_added, 1);
}

#[test]
fn test_remaining_labile_time() {
    let mut manager = ReconsolidationManager::new();
    let snapshot = make_snapshot();

    manager.mark_labile("mem-1", snapshot);

    let remaining = manager.remaining_labile_time("mem-1");
    assert!(remaining.is_some());
    assert!(remaining.unwrap() > Duration::zero());
}

#[test]
fn test_modification_types() {
    let modifications = vec![
        Modification::AddContext {
            context: "test".to_string(),
        },
        Modification::StrengthenConnection {
            target_memory_id: "other".to_string(),
            boost: 0.5,
        },
        Modification::AddTag {
            tag: "tag".to_string(),
        },
        Modification::RemoveTag {
            tag: "old".to_string(),
        },
        Modification::UpdateEmotion {
            sentiment_score: Some(0.5),
            sentiment_magnitude: None,
        },
        Modification::LinkMemory {
            related_memory_id: "rel".to_string(),
            relationship: RelationshipType::Supports,
        },
        Modification::UpdateContent {
            new_content: None,
            is_correction: true,
        },
        Modification::AddSource {
            source: "web".to_string(),
        },
        Modification::BoostRetrieval { boost: 0.1 },
    ];

    for modification in modifications {
        assert!(!modification.description().is_empty());
    }
}

#[test]
fn test_relationship_types() {
    let relationships = [
        RelationshipType::Supports,
        RelationshipType::Contradicts,
        RelationshipType::Elaborates,
        RelationshipType::Generalizes,
        RelationshipType::Exemplifies,
        RelationshipType::TemporallyRelated,
        RelationshipType::Causes,
        RelationshipType::SimilarTo,
    ];

    // Just ensure all variants exist
    assert_eq!(relationships.len(), 8);
}

#[test]
fn test_change_summary() {
    let mut summary = ChangeSummary::default();
    assert!(!summary.has_changes());

    summary.tags_added = 1;
    assert!(summary.has_changes());
}

#[test]
fn test_labile_state() {
    let snapshot = make_snapshot();
    let mut state = LabileState::new("mem-1".to_string(), snapshot);

    assert!(state.is_within_window(Duration::seconds(300)));
    assert!(!state.reconsolidated);

    // Add modifications
    for i in 0..MAX_MODIFICATIONS_PER_WINDOW {
        assert!(state.add_modification(Modification::AddTag {
            tag: format!("tag-{}", i),
        }));
    }

    // Should fail now (limit reached)
    assert!(!state.add_modification(Modification::AddTag {
        tag: "overflow".to_string(),
    }));
}

#[test]
fn test_retrieval_history() {
    let mut manager = ReconsolidationManager::new();
    let snapshot = make_snapshot();

    // Mark and reconsolidate multiple times
    for _ in 0..3 {
        manager.mark_labile("mem-1", snapshot.clone());
        manager.reconsolidate("mem-1");
    }

    assert_eq!(manager.get_retrieval_count("mem-1"), 3);
    assert_eq!(manager.get_retrieval_history("mem-1").len(), 3);
}

#[test]
fn test_stats() {
    let mut manager = ReconsolidationManager::new();
    let snapshot = make_snapshot();

    manager.mark_labile("mem-1", snapshot.clone());
    manager.apply_modification(
        "mem-1",
        Modification::AddTag {
            tag: "t".to_string(),
        },
    );
    manager.reconsolidate("mem-1");

    let stats = manager.get_stats();
    assert_eq!(stats.total_marked_labile, 1);
    assert_eq!(stats.total_reconsolidated, 1);
    assert_eq!(stats.total_modified, 1);
    assert_eq!(stats.total_modifications, 1);
}

#[test]
fn test_disabled_manager() {
    let mut manager = ReconsolidationManager::new();
    manager.set_enabled(false);

    let snapshot = make_snapshot();
    manager.mark_labile("mem-1", snapshot);

    // Should not be labile when disabled
    assert!(!manager.is_labile("mem-1"));
}

#[test]
fn test_access_context() {
    let mut manager = ReconsolidationManager::new();
    let snapshot = make_snapshot();
    let context = AccessContext {
        trigger: AccessTrigger::Search,
        query: Some("test query".to_string()),
        co_retrieved: vec!["mem-2".to_string(), "mem-3".to_string()],
        session_id: Some("session-1".to_string()),
    };

    manager.mark_labile_with_context("mem-1", snapshot, context);

    let state = manager.get_labile_state("mem-1");
    assert!(state.is_some());
    assert!(state.unwrap().access_context.is_some());
}

#[test]
fn test_get_labile_memory_ids() {
    let mut manager = ReconsolidationManager::new();

    manager.mark_labile("mem-1", make_snapshot());
    manager.mark_labile("mem-2", make_snapshot());
    manager.mark_labile("mem-3", make_snapshot());

    let ids = manager.get_labile_memory_ids();
    assert_eq!(ids.len(), 3);
}
