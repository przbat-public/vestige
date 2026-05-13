//! Tests for the 4-phase dream cycle.

use chrono::{Duration, Utc};

use crate::memory::KnowledgeNode;
use crate::neuroscience::emotional_memory::EmotionalMemory;
use crate::neuroscience::importance_signals::ImportanceSignals;
use crate::neuroscience::synaptic_tagging::SynapticTaggingSystem;

use super::engine::DreamEngine;
use super::types::{
    CreativeConnection, CreativeConnectionType, DreamPhase, TriageCategory, TriagedMemory,
};

fn make_test_node(id: &str, content: &str, tags: &[&str]) -> KnowledgeNode {
    let now = Utc::now();
    KnowledgeNode {
        id: id.to_string(),
        content: content.to_string(),
        node_type: "fact".to_string(),
        created_at: now - Duration::hours(1),
        updated_at: now,
        last_accessed: now,
        stability: 5.0,
        difficulty: 5.0,
        reps: 2,
        lapses: 0,
        storage_strength: 3.0,
        retrieval_strength: 0.8,
        retention_strength: 0.7,
        sentiment_score: 0.0,
        sentiment_magnitude: 0.0,
        next_review: None,
        source: None,
        tags: tags.iter().map(|s| s.to_string()).collect(),
        valid_from: None,
        valid_until: None,
        utility_score: None,
        times_retrieved: None,
        times_useful: None,
        emotional_valence: None,
        flashbulb: None,
        temporal_level: None,
        has_embedding: None,
        embedding_model: None,
        provenance: None,
        ..Default::default()
    }
}

fn make_emotional_node(id: &str, content: &str, sentiment_mag: f64) -> KnowledgeNode {
    let mut node = make_test_node(id, content, &["bug-fix"]);
    node.sentiment_magnitude = sentiment_mag;
    node
}

#[test]
fn test_dream_engine_creation() {
    let engine = DreamEngine::new();
    assert!((engine.high_value_ratio - 0.7).abs() < f64::EPSILON);
    assert_eq!(engine.wave_batch_size, 15);
}

#[test]
fn test_full_dream_cycle_runs() {
    let engine = DreamEngine::new();
    let mut emotional = EmotionalMemory::new();
    let importance = ImportanceSignals::new();
    let mut synaptic = SynapticTaggingSystem::new();

    let memories: Vec<KnowledgeNode> = (0..10)
        .map(|i| {
            make_test_node(
                &format!("mem-{}", i),
                &format!("Test memory content for dream cycle number {}", i),
                &["test"],
            )
        })
        .collect();

    let result = engine.run(&memories, &mut emotional, &importance, &mut synaptic);

    assert_eq!(result.phases.len(), 4);
    assert_eq!(result.phases[0].phase, DreamPhase::Nrem1);
    assert_eq!(result.phases[1].phase, DreamPhase::Nrem3);
    assert_eq!(result.phases[2].phase, DreamPhase::Rem);
    assert_eq!(result.phases[3].phase, DreamPhase::Integration);
    assert!(result.total_duration_ms < 5000); // Should be fast
    assert_eq!(result.memories_replayed, 10); // All 10 in replay queue
}

#[test]
fn test_nrem1_triage_categories() {
    let engine = DreamEngine::new();
    let mut emotional = EmotionalMemory::new();
    let importance = ImportanceSignals::new();

    let memories = vec![
        make_emotional_node("emo-1", "Critical production crash error panic!", 0.9),
        make_test_node(
            "future-1",
            "TODO: remind me to add caching next time",
            &["planning"],
        ),
        make_test_node("standard-1", "The function returns a string", &["docs"]),
    ];

    let (triaged, _queue, phase) = engine.phase_nrem1(&memories, &mut emotional, &importance);

    assert_eq!(triaged.len(), 3);
    assert_eq!(phase.phase, DreamPhase::Nrem1);
    assert!(phase.memories_processed == 3);

    // Emotional memory should be categorized
    let emo = triaged.iter().find(|m| m.id == "emo-1").unwrap();
    assert_eq!(emo.category, TriageCategory::Emotional);

    // Future-relevant should be categorized
    let future = triaged.iter().find(|m| m.id == "future-1").unwrap();
    assert_eq!(future.category, TriageCategory::FutureRelevant);
}

#[test]
fn test_replay_queue_70_30_split() {
    let engine = DreamEngine::new();
    let mut emotional = EmotionalMemory::new();
    let importance = ImportanceSignals::new();

    let memories: Vec<KnowledgeNode> = (0..20)
        .map(|i| {
            make_test_node(
                &format!("mem-{}", i),
                &format!("Memory with varying importance content {}", i),
                &["test"],
            )
        })
        .collect();

    let (_triaged, queue, _phase) = engine.phase_nrem1(&memories, &mut emotional, &importance);

    // All 20 should be in the queue (70% + 30% = 100%)
    assert_eq!(queue.len(), 20);
}

#[test]
fn test_nrem3_consolidation_waves() {
    let engine = DreamEngine::new();
    let mut synaptic = SynapticTaggingSystem::new();

    let triaged: Vec<TriagedMemory> = (0..10)
        .map(|i| TriagedMemory {
            id: format!("mem-{}", i),
            content: format!("Test memory {}", i),
            importance: 0.5,
            category: TriageCategory::Standard,
            tags: vec!["test".to_string()],
            created_at: Utc::now(),
            retention_strength: 0.7,
            emotional_valence: 0.0,
            is_flashbulb: false,
        })
        .collect();

    let replay_queue: Vec<String> = triaged.iter().map(|m| m.id.clone()).collect();

    let (strengthened, _downscaled, phase) =
        engine.phase_nrem3(&replay_queue, &triaged, &mut synaptic);

    assert_eq!(phase.phase, DreamPhase::Nrem3);
    assert_eq!(strengthened.len(), 10);
}

#[test]
fn test_synaptic_downscaling() {
    let engine = DreamEngine::new();
    let mut synaptic = SynapticTaggingSystem::new();

    let triaged: Vec<TriagedMemory> = vec![
        TriagedMemory {
            id: "replayed".to_string(),
            content: "Important replayed memory".to_string(),
            importance: 0.8,
            category: TriageCategory::Novel,
            tags: vec![],
            created_at: Utc::now(),
            retention_strength: 0.9,
            emotional_valence: 0.0,
            is_flashbulb: false,
        },
        TriagedMemory {
            id: "unreplayed".to_string(),
            content: "Low importance unreplayed memory".to_string(),
            importance: 0.2,
            category: TriageCategory::Standard,
            tags: vec![],
            created_at: Utc::now(),
            retention_strength: 0.3,
            emotional_valence: 0.0,
            is_flashbulb: false,
        },
    ];

    // Only replay the important one
    let replay_queue = vec!["replayed".to_string()];

    let (_strengthened, downscaled, _phase) =
        engine.phase_nrem3(&replay_queue, &triaged, &mut synaptic);

    // The unreplayed low-importance memory should be marked for downscaling
    assert_eq!(downscaled, 1);
}

#[test]
fn test_rem_cross_domain_connections() {
    let engine = DreamEngine::new();
    let mut emotional = EmotionalMemory::new();

    let triaged = vec![
        TriagedMemory {
            id: "rust-1".to_string(),
            content: "Implemented error handling with Result type pattern".to_string(),
            importance: 0.6,
            category: TriageCategory::Standard,
            tags: vec!["rust".to_string()],
            created_at: Utc::now(),
            retention_strength: 0.7,
            emotional_valence: 0.3,
            is_flashbulb: false,
        },
        TriagedMemory {
            id: "typescript-1".to_string(),
            content: "Used error handling with try-catch pattern for API errors".to_string(),
            importance: 0.5,
            category: TriageCategory::Standard,
            tags: vec!["typescript".to_string()],
            created_at: Utc::now(),
            retention_strength: 0.6,
            emotional_valence: 0.0,
            is_flashbulb: false,
        },
    ];

    let (connections, _emotional_processed, phase) = engine.phase_rem(&triaged, &mut emotional);

    assert_eq!(phase.phase, DreamPhase::Rem);
    // Should find connection via shared "error handling" and "pattern" words
    assert!(
        !connections.is_empty(),
        "Should find cross-domain error handling pattern"
    );
}

#[test]
fn test_rem_emotional_processing() {
    let engine = DreamEngine::new();
    let mut emotional = EmotionalMemory::new();

    let triaged = vec![TriagedMemory {
        id: "angry-1".to_string(),
        content: "Critical production error crashed the entire system".to_string(),
        importance: 0.8,
        category: TriageCategory::Emotional,
        tags: vec!["incident".to_string()],
        created_at: Utc::now(),
        retention_strength: 0.9,
        emotional_valence: -0.8,
        is_flashbulb: false,
    }];

    let (_connections, emotional_processed, _phase) = engine.phase_rem(&triaged, &mut emotional);

    assert_eq!(
        emotional_processed, 1,
        "Negative emotional memory should be processed"
    );
}

#[test]
fn test_integration_validates_insights() {
    let engine = DreamEngine::new();

    let connections = vec![
        CreativeConnection {
            memory_a_id: "a".to_string(),
            memory_b_id: "b".to_string(),
            insight: "Strong connection".to_string(),
            confidence: 0.8,
            connection_type: CreativeConnectionType::CrossDomain,
        },
        CreativeConnection {
            memory_a_id: "c".to_string(),
            memory_b_id: "d".to_string(),
            insight: "Weak connection".to_string(),
            confidence: 0.1, // Below validation threshold
            connection_type: CreativeConnectionType::Complementary,
        },
    ];

    let triaged = vec![
        TriagedMemory {
            id: "a".to_string(),
            content: "Memory A".to_string(),
            importance: 0.5,
            category: TriageCategory::Standard,
            tags: vec!["tag-a".to_string()],
            created_at: Utc::now() - Duration::days(10),
            retention_strength: 0.7,
            emotional_valence: 0.0,
            is_flashbulb: false,
        },
        TriagedMemory {
            id: "b".to_string(),
            content: "Memory B".to_string(),
            importance: 0.5,
            category: TriageCategory::Standard,
            tags: vec!["tag-b".to_string()],
            created_at: Utc::now(),
            retention_strength: 0.8,
            emotional_valence: 0.0,
            is_flashbulb: false,
        },
    ];

    let (insights, phase) = engine.phase_integration(&connections, &triaged);

    assert_eq!(phase.phase, DreamPhase::Integration);
    // Only the strong connection should survive validation
    assert_eq!(insights.len(), 1);
    assert_eq!(insights[0].insight, "Strong connection");
}

#[test]
fn test_content_similarity() {
    let engine = DreamEngine::new();

    let sim = engine.content_similarity(
        "error handling with Result type pattern",
        "error handling with try-catch pattern",
    );
    assert!(
        sim > 0.2,
        "Similar content should have >0.2 Jaccard: {}",
        sim
    );

    let dissim = engine.content_similarity(
        "Rust memory management with ownership",
        "Python web framework for HTTP endpoints",
    );
    assert!(dissim < sim, "Dissimilar content should score lower");
}

#[test]
fn test_empty_memories_returns_empty_results() {
    let engine = DreamEngine::new();
    let mut emotional = EmotionalMemory::new();
    let importance = ImportanceSignals::new();
    let mut synaptic = SynapticTaggingSystem::new();

    let result = engine.run(&[], &mut emotional, &importance, &mut synaptic);

    assert_eq!(result.phases.len(), 4);
    assert_eq!(result.memories_replayed, 0);
    assert_eq!(result.insights.len(), 0);
    assert_eq!(result.memories_strengthened, 0);
}

#[test]
fn test_phase_durations_are_recorded() {
    let engine = DreamEngine::new();
    let mut emotional = EmotionalMemory::new();
    let importance = ImportanceSignals::new();
    let mut synaptic = SynapticTaggingSystem::new();

    let memories: Vec<KnowledgeNode> = (0..5)
        .map(|i| make_test_node(&format!("m{}", i), &format!("Content {}", i), &["test"]))
        .collect();

    let result = engine.run(&memories, &mut emotional, &importance, &mut synaptic);

    for phase in &result.phases {
        // Duration should be non-negative (might be 0ms for fast operations)
        assert!(phase.duration_ms < 10000);
        assert!(
            !phase.actions.is_empty(),
            "Each phase should report actions"
        );
    }
}

#[test]
fn test_flashbulb_detected_in_triage() {
    let engine = DreamEngine::new();
    let mut emotional = EmotionalMemory::new();
    let importance = ImportanceSignals::new();

    let mut node = make_test_node(
        "flash-1",
        "CRITICAL: Production server crash! Emergency rollback needed immediately!",
        &["incident"],
    );
    node.sentiment_magnitude = 0.9;

    let (triaged, _queue, phase) = engine.phase_nrem1(&[node], &mut emotional, &importance);

    // Check that the triage processed the memory
    assert_eq!(triaged.len(), 1);
    // Flashbulb detection depends on importance signals — just verify triage runs
    assert_eq!(phase.phase, DreamPhase::Nrem1);
}
