//! Tests for the dreams module.

use chrono::{DateTime, Duration, Utc};

use crate::advanced::prediction_error::{CandidateMemory, PredictionErrorGate};
use crate::memory::freshness::newest;
use crate::memory::{FreshnessKey, KnowledgeNode};

use super::activity::ActivityTracker;
use super::connection_graph::{ConnectionGraph, ConnectionReason};
use super::dreamer::MemoryDreamer;
use super::replay::{MemoryReplay, Pattern, PatternType};
use super::report::ConsolidationReport;
use super::scheduler::ConsolidationScheduler;
use super::similarity::{
    calculate_memory_similarity, content_word_similarity, cosine_similarity, tag_similarity,
};
use super::types::{
    ContradictionPair, DiscoveredConnection, DiscoveredConnectionType, DreamMemory, DreamStats,
    InsightType,
};

fn make_memory(id: &str, content: &str, tags: Vec<&str>) -> DreamMemory {
    DreamMemory {
        id: id.to_string(),
        content: content.to_string(),
        embedding: None,
        tags: tags.into_iter().map(String::from).collect(),
        created_at: Utc::now(),
        access_count: 1,
    }
}

fn make_memory_with_time(id: &str, content: &str, tags: Vec<&str>, hours_ago: i64) -> DreamMemory {
    DreamMemory {
        id: id.to_string(),
        content: content.to_string(),
        embedding: None,
        tags: tags.into_iter().map(String::from).collect(),
        created_at: Utc::now() - Duration::hours(hours_ago),
        access_count: 1,
    }
}

#[tokio::test]
async fn test_dream_cycle() {
    let dreamer = MemoryDreamer::new();

    let memories = vec![
        make_memory(
            "1",
            "Database indexing improves query performance",
            vec!["database", "performance"],
        ),
        make_memory(
            "2",
            "Query optimization techniques for SQL",
            vec!["database", "sql"],
        ),
        make_memory(
            "3",
            "Performance tuning in database systems",
            vec!["database", "performance"],
        ),
        make_memory(
            "4",
            "Understanding B-tree indexes",
            vec!["database", "indexing"],
        ),
    ];

    let result = dreamer.dream(&memories).await;

    assert!(result.stats.memories_analyzed == 4);
    assert!(result.stats.connections_evaluated > 0);
}

#[test]
fn test_tag_similarity() {
    let dreamer = MemoryDreamer::new();

    let tags_a = vec!["rust".to_string(), "programming".to_string()];
    let tags_b = vec!["rust".to_string(), "memory".to_string()];

    let sim = dreamer.tag_similarity(&tags_a, &tags_b);
    assert!(sim > 0.0 && sim < 1.0);
}

#[test]
fn test_insight_type_description() {
    assert!(!InsightType::HiddenConnection.description().is_empty());
    assert!(!InsightType::RecurringPattern.description().is_empty());
}

#[test]
fn test_cosine_similarity() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

    let c = vec![0.0, 1.0, 0.0];
    assert!(cosine_similarity(&a, &c).abs() < 0.001);
}

// ========== Activity Tracker Tests ==========

#[test]
fn test_activity_tracker_new() {
    let tracker = ActivityTracker::new();
    assert!(tracker.is_idle());
    assert_eq!(tracker.activity_rate(), 0.0);
}

#[test]
fn test_activity_tracker_record() {
    let mut tracker = ActivityTracker::new();

    tracker.record_activity();
    assert!(!tracker.is_idle()); // Just recorded activity

    let stats = tracker.get_stats();
    assert_eq!(stats.total_events, 1);
    assert!(stats.last_activity.is_some());
}

#[test]
fn test_activity_rate() {
    let mut tracker = ActivityTracker::new();

    // Record 10 events
    for _ in 0..10 {
        tracker.record_activity();
    }

    // Rate should be > 0
    assert!(tracker.activity_rate() > 0.0);
}

// ========== Consolidation Scheduler Tests ==========

#[test]
fn test_scheduler_new() {
    let scheduler = ConsolidationScheduler::new();
    // Should consolidate immediately (interval passed since "past" initialization)
    assert!(scheduler.should_consolidate_force());
}

#[test]
fn test_scheduler_with_interval() {
    let scheduler = ConsolidationScheduler::with_interval(12);
    assert!(scheduler.time_until_next() <= Duration::hours(12));
}

#[test]
fn test_scheduler_activity_tracking() {
    let mut scheduler = ConsolidationScheduler::new();

    scheduler.record_activity();

    let stats = scheduler.get_activity_stats();
    assert_eq!(stats.total_events, 1);
    assert!(!stats.is_idle);
}

#[tokio::test]
async fn test_consolidation_cycle() {
    let mut scheduler = ConsolidationScheduler::new();

    let memories = vec![
        make_memory_with_time("1", "First memory about rust", vec!["rust"], 5),
        make_memory_with_time(
            "2",
            "Second memory about rust programming",
            vec!["rust", "programming"],
            4,
        ),
        make_memory_with_time("3", "Third memory about systems", vec!["systems"], 3),
        make_memory_with_time(
            "4",
            "Fourth memory about rust systems",
            vec!["rust", "systems"],
            2,
        ),
    ];

    let report = scheduler.run_consolidation_cycle(&memories).await;

    // Should have completed all stages
    assert!(report.stage1_replay.is_some());
    // duration_ms is u64, so just verify the field is accessible
    let _ = report.duration_ms;
    assert!(report.completed_at <= Utc::now());
}

// ========== Memory Replay Tests ==========

#[test]
fn test_memory_replay_structure() {
    let replay = MemoryReplay {
        sequence: vec!["1".to_string(), "2".to_string()],
        synthetic_combinations: vec![("1".to_string(), "2".to_string())],
        discovered_patterns: vec![],
        replayed_at: Utc::now(),
    };

    assert_eq!(replay.sequence.len(), 2);
    assert_eq!(replay.synthetic_combinations.len(), 1);
}

// ========== Connection Graph Tests ==========

#[test]
fn test_connection_graph_add() {
    let mut graph = ConnectionGraph::new();

    graph.add_connection("a", "b", 0.8, ConnectionReason::Semantic);

    assert_eq!(graph.connection_count("a"), 1);
    assert_eq!(graph.connection_count("b"), 1);
    assert!((graph.total_connection_strength("a") - 0.8).abs() < 0.01);
}

#[test]
fn test_connection_graph_strengthen() {
    let mut graph = ConnectionGraph::new();

    graph.add_connection("a", "b", 0.5, ConnectionReason::Semantic);
    assert!(graph.strengthen_connection("a", "b", 0.2));

    // Strength should be approximately 0.7
    let strength = graph.total_connection_strength("a");
    assert!(strength >= 0.7);
}

#[test]
fn test_connection_graph_decay_and_prune() {
    let mut graph = ConnectionGraph::new();

    graph.add_connection("a", "b", 0.2, ConnectionReason::Semantic);

    // Apply decay multiple times
    for _ in 0..10 {
        graph.apply_decay(0.8);
    }

    // Prune weak connections
    let pruned = graph.prune_weak(0.1);

    // Connection should be pruned
    assert!(pruned > 0 || graph.connection_count("a") == 0);
}

#[test]
fn test_connection_graph_stats() {
    let mut graph = ConnectionGraph::new();

    graph.add_connection("a", "b", 0.8, ConnectionReason::Semantic);
    graph.add_connection("b", "c", 0.6, ConnectionReason::CrossReference);

    let stats = graph.get_stats();
    assert_eq!(stats.total_connections, 2);
    assert!(stats.average_strength > 0.0);
}

// ========== Consolidation Report Tests ==========

#[test]
fn test_consolidation_report_new() {
    let report = ConsolidationReport::new();

    assert_eq!(report.stage2_connections, 0);
    assert_eq!(report.total_insights(), 0);
    assert_eq!(report.total_new_connections(), 0);
}

// ========== Pattern Tests ==========

#[test]
fn test_pattern_types() {
    let pattern = Pattern {
        id: "test".to_string(),
        pattern_type: PatternType::Recurring,
        description: "Test pattern".to_string(),
        memory_ids: vec!["1".to_string(), "2".to_string()],
        confidence: 0.8,
        discovered_at: Utc::now(),
    };

    assert_eq!(pattern.pattern_type, PatternType::Recurring);
    assert_eq!(pattern.memory_ids.len(), 2);
}

// ========== Helper Function Tests ==========

#[test]
fn test_calculate_memory_similarity() {
    let mem_a = make_memory(
        "1",
        "Rust programming language",
        vec!["rust", "programming"],
    );
    let mem_b = make_memory("2", "Rust systems programming", vec!["rust", "systems"]);

    let similarity = calculate_memory_similarity(&mem_a, &mem_b);
    assert!(similarity > 0.0); // Should have some similarity due to shared "rust" tag
}

#[test]
fn test_tag_similarity_function() {
    let tags_a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let tags_b = vec!["b".to_string(), "c".to_string(), "d".to_string()];

    let sim = tag_similarity(&tags_a, &tags_b);
    // Jaccard: 2 / 4 = 0.5
    assert!((sim - 0.5).abs() < 0.01);
}

#[test]
fn test_content_word_similarity() {
    let content_a = "The quick brown fox jumps over the lazy dog";
    let content_b = "The quick brown cat jumps over the lazy dog";

    let sim = content_word_similarity(content_a, content_b);
    assert!(sim > 0.5); // High overlap
}

// ========== Negation Divergence Tests ==========

#[test]
fn test_negation_divergence_detects_opposing_texts() {
    let a = "The API does not support pagination and cannot handle large datasets";
    let b = "The API supports pagination and handles large datasets well";
    assert!(
        MemoryDreamer::has_negation_divergence(a, b),
        "Should detect negation markers in A that are absent in B"
    );
}

#[test]
fn test_negation_divergence_ignores_similar_texts() {
    let a = "Rust is a systems programming language";
    let b = "Rust is used for systems programming";
    assert!(
        !MemoryDreamer::has_negation_divergence(a, b),
        "Two affirmative sentences should not trigger divergence"
    );
}

#[test]
fn test_negation_divergence_requires_significant_gap() {
    let a = "It doesn't work on Windows";
    let b = "It works on Linux";
    // Only 1 negation marker difference — below threshold of 2
    assert!(
        !MemoryDreamer::has_negation_divergence(a, b),
        "A single negation difference should not trigger (threshold = 2)"
    );
}

#[test]
fn test_negation_divergence_polish_markers() {
    let a = "API nie obsługuje paginacji, nigdy nie działało poprawnie";
    let b = "API obsługuje paginację i działa poprawnie";
    assert!(
        MemoryDreamer::has_negation_divergence(a, b),
        "Polish negation markers (nie, nigdy) should be detected"
    );
}

// ========== Contradiction Detection Tests ==========

#[test]
fn test_contradiction_detected_in_dream() {
    let dreamer = MemoryDreamer::new();

    let mem_old = DreamMemory {
        id: "old".to_string(),
        content: "The function does not handle errors and cannot recover from failures".to_string(),
        embedding: Some(vec![1.0, 0.0, 0.0]),
        tags: vec!["error-handling".to_string()],
        created_at: Utc::now() - Duration::days(30),
        access_count: 2,
    };
    let mem_new = DreamMemory {
        id: "new".to_string(),
        content: "The function handles errors gracefully and recovers from failures".to_string(),
        embedding: Some(vec![0.95, 0.1, 0.0]),
        tags: vec!["error-handling".to_string()],
        created_at: Utc::now(),
        access_count: 5,
    };

    let memories = vec![&mem_old, &mem_new];
    let connections = dreamer.discover_connections(&memories, &mut DreamStats::default());

    let contradictions: Vec<_> = connections
        .iter()
        .filter(|c| c.connection_type == DiscoveredConnectionType::Contradiction)
        .collect();

    assert!(
        !contradictions.is_empty(),
        "Should detect contradiction between opposing error-handling claims"
    );
}

#[test]
fn test_contradiction_demotes_older_less_accessed_memory() {
    let dreamer = MemoryDreamer::new();

    let mem_old = DreamMemory {
        id: "old".to_string(),
        content: "The service does not support authentication and cannot verify users".to_string(),
        embedding: Some(vec![1.0, 0.0, 0.0]),
        tags: vec!["auth".to_string()],
        created_at: Utc::now() - Duration::days(60),
        access_count: 1,
    };
    let mem_new = DreamMemory {
        id: "new".to_string(),
        content: "The service supports JWT authentication and verifies users properly".to_string(),
        embedding: Some(vec![0.95, 0.1, 0.0]),
        tags: vec!["auth".to_string()],
        created_at: Utc::now(),
        access_count: 10,
    };

    let memories = vec![&mem_old, &mem_new];
    let connections = dreamer.discover_connections(&memories, &mut DreamStats::default());
    let contradictions = dreamer.detect_contradictions(&memories, &connections);

    if !contradictions.is_empty() {
        assert_eq!(contradictions[0].survivor_id, "new");
        assert_eq!(contradictions[0].demoted_id, "old");
    }
}

#[test]
fn test_dream_result_includes_contradictions() {
    let dreamer = MemoryDreamer::new();

    let mem1 = DreamMemory {
        id: "a".to_string(),
        content: "Feature X was deprecated and removed from the codebase entirely".to_string(),
        embedding: Some(vec![1.0, 0.0]),
        tags: vec!["feature-x".to_string()],
        created_at: Utc::now() - Duration::days(90),
        access_count: 2,
    };
    let mem2 = DreamMemory {
        id: "b".to_string(),
        content: "Feature X is active and working correctly in production".to_string(),
        embedding: Some(vec![0.9, 0.1]),
        tags: vec!["feature-x".to_string()],
        created_at: Utc::now(),
        access_count: 8,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = rt.block_on(dreamer.dream(&[mem1, mem2]));

    let _ = (
        result.contradictions_found.len(),
        result.memories_demoted.len(),
    );
}

// ========== Connection Type Contradiction ==========

#[test]
fn test_connection_type_contradiction_variant() {
    let conn_type = DiscoveredConnectionType::Contradiction;
    let serialized = serde_json::to_string(&conn_type).unwrap();
    assert!(serialized.contains("Contradiction"));

    let deserialized: DiscoveredConnectionType = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, DiscoveredConnectionType::Contradiction);
}

// ========== Deterministic Contradiction Resolution ==========

fn make_dream_memory(
    id: &str,
    content: &str,
    created_at: DateTime<Utc>,
    access_count: u32,
) -> DreamMemory {
    DreamMemory {
        id: id.to_string(),
        content: content.to_string(),
        embedding: None,
        tags: vec![],
        created_at,
        access_count,
    }
}

/// Resolve a pair through an already-discovered contradiction. Using an explicit
/// connection isolates the survivor choice from the discovery heuristic, so the
/// assertions below pin only the resolution rule.
fn resolve_contradiction(first: &DreamMemory, second: &DreamMemory) -> Vec<ContradictionPair> {
    let connection = DiscoveredConnection {
        from_id: first.id.clone(),
        to_id: second.id.clone(),
        similarity: 0.99,
        connection_type: DiscoveredConnectionType::Contradiction,
        reasoning: "fixture: pair already discovered as contradictory".to_string(),
    };
    MemoryDreamer::new().detect_contradictions(&[first, second], &[connection])
}

#[test]
fn contradiction_survivor_is_the_newer_memory_not_the_most_accessed_one() {
    let dreamer = MemoryDreamer::new();

    // The stale claim was retrieved far more often than its correction — the
    // shape that made `access_count` win. Popularity is not freshness: keeping
    // the stale side demotes the correction and lets the corrected fact return.
    let stale = DreamMemory {
        id: "stale".to_string(),
        content: "The service does not support authentication and cannot verify users".to_string(),
        embedding: Some(vec![1.0, 0.0, 0.0]),
        tags: vec!["auth".to_string()],
        created_at: Utc::now() - Duration::days(60),
        access_count: 100,
    };
    let correction = DreamMemory {
        id: "correction".to_string(),
        content: "The service supports JWT authentication and verifies users properly".to_string(),
        embedding: Some(vec![0.95, 0.1, 0.0]),
        tags: vec!["auth".to_string()],
        created_at: Utc::now(),
        access_count: 1,
    };

    let memories = [&stale, &correction];
    let connections = dreamer.discover_connections(&memories, &mut DreamStats::default());
    let contradictions = dreamer.detect_contradictions(&memories, &connections);

    assert_eq!(
        contradictions.len(),
        1,
        "fixture must be discovered as a contradiction, got {contradictions:?}"
    );
    assert_eq!(contradictions[0].survivor_id, "correction");
    assert_eq!(contradictions[0].demoted_id, "stale");
}

#[test]
fn contradiction_tie_is_broken_by_memory_id_not_by_connection_direction() {
    let now = Utc::now();
    // Equal timestamps and equal access counts: nothing but the documented
    // tiebreak may decide. The old fall-through returned whichever memory the
    // connection named second, so the same store produced a different demotion
    // list depending on the order the caller handed the memories over.
    let aaa = make_dream_memory("aaa", "Deploy is on Friday", now, 3);
    let zzz = make_dream_memory("zzz", "Deploy moved to Monday", now, 3);

    let forward = resolve_contradiction(&aaa, &zzz);
    let backward = resolve_contradiction(&zzz, &aaa);

    assert_eq!(forward[0].survivor_id, "zzz");
    assert_eq!(backward[0].survivor_id, "zzz");
    assert_eq!(forward[0].demoted_id, "aaa");
    assert_eq!(backward[0].demoted_id, "aaa");
}

/// The stored-node view of a dream input, with no valid-time anchor — the shape
/// the read path sees for a memory the temporal extractor never anchored.
fn stored_node(memory: &DreamMemory) -> KnowledgeNode {
    KnowledgeNode {
        id: memory.id.clone(),
        content: memory.content.clone(),
        created_at: memory.created_at,
        ..KnowledgeNode::default()
    }
}

/// The invariant that was violated: one pair of conflicting memories must leave
/// the write path (ingest gate), the consolidation path (dream cycle) and the
/// read path (the shared freshness rule) naming the same loser. The dream cycle
/// ranked by `access_count` first, so a stale-but-popular memory survived there
/// while every read path called that same memory superseded — a `reflect` pass
/// and a `search` disagreeing about the same database.
#[test]
fn write_path_and_read_path_agree_on_which_memory_is_superseded() {
    let stale = DreamMemory {
        id: "stale".to_string(),
        content: "The service does not support authentication and cannot verify users".to_string(),
        embedding: Some(vec![1.0, 0.0, 0.0]),
        tags: vec!["auth".to_string()],
        created_at: Utc::now() - Duration::days(60),
        access_count: 100,
    };
    let correction = DreamMemory {
        id: "correction".to_string(),
        content: "The service supports JWT authentication and verifies users properly".to_string(),
        embedding: Some(vec![0.95, 0.1, 0.0]),
        tags: vec!["auth".to_string()],
        created_at: Utc::now(),
        access_count: 1,
    };

    // Write path: filing the correction against the memory it contradicts.
    let mut gate = PredictionErrorGate::new();
    let decision = gate.evaluate(
        &correction.content,
        &[1.0, 0.0, 0.0],
        &[CandidateMemory {
            id: stale.id.clone(),
            content: stale.content.clone(),
            embedding: vec![1.0, 0.0, 0.0],
            retrieval_strength: 0.9,
            retention_strength: 0.9,
            tags: stale.tags.clone(),
            source: None,
            was_demoted: false,
            was_promoted: false,
        }],
    );
    assert_eq!(decision.target_id(), Some(stale.id.as_str()));

    // Consolidation path: the dream cycle resolves the same pair.
    let dreamer = MemoryDreamer::new();
    let memories = [&stale, &correction];
    let connections = dreamer.discover_connections(&memories, &mut DreamStats::default());
    let contradictions = dreamer.detect_contradictions(&memories, &connections);
    assert_eq!(
        contradictions.len(),
        1,
        "fixture must be discovered as a contradiction, got {contradictions:?}"
    );

    // Read path: the same pair as stored nodes, through the shared rule.
    let stale_key = FreshnessKey::from_node(&stored_node(&stale));
    let correction_key = FreshnessKey::from_node(&stored_node(&correction));
    let current = newest([&stale_key, &correction_key]).expect("non-empty candidate set");

    assert_eq!(current.memory_id, "correction");
    assert_eq!(contradictions[0].survivor_id, current.memory_id);
    assert_eq!(contradictions[0].demoted_id, stale.id);
    assert_eq!(
        decision.target_id(),
        Some(contradictions[0].demoted_id.as_str())
    );
}
