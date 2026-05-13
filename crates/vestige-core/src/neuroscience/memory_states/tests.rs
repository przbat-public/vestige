use super::competition::cosine_similarity;
use super::*;
use chrono::{Duration, Utc};

fn approx_eq(a: f64, b: f64, epsilon: f64) -> bool {
    (a - b).abs() < epsilon
}

// ==================== MemoryState Tests ====================

#[test]
fn test_memory_state_accessibility() {
    assert!(approx_eq(
        MemoryState::Active.accessibility_multiplier(),
        1.0,
        0.001
    ));
    assert!(approx_eq(
        MemoryState::Dormant.accessibility_multiplier(),
        0.7,
        0.001
    ));
    assert!(approx_eq(
        MemoryState::Silent.accessibility_multiplier(),
        0.3,
        0.001
    ));
    assert!(approx_eq(
        MemoryState::Unavailable.accessibility_multiplier(),
        0.05,
        0.001
    ));
}

#[test]
fn test_memory_state_retrievability() {
    assert!(MemoryState::Active.is_retrievable());
    assert!(MemoryState::Dormant.is_retrievable());
    assert!(!MemoryState::Silent.is_retrievable());
    assert!(!MemoryState::Unavailable.is_retrievable());

    assert!(MemoryState::Silent.requires_strong_cue());
    assert!(MemoryState::Unavailable.is_blocked());
}

#[test]
fn test_memory_state_roundtrip() {
    for state in [
        MemoryState::Active,
        MemoryState::Dormant,
        MemoryState::Silent,
        MemoryState::Unavailable,
    ] {
        assert_eq!(MemoryState::parse_name(state.as_str()), state);
    }
}

// ==================== MemoryLifecycle Tests ====================

#[test]
fn test_lifecycle_creation() {
    let lifecycle = MemoryLifecycle::new();
    assert_eq!(lifecycle.state, MemoryState::Active);
    assert_eq!(lifecycle.access_count, 1);
    assert!(lifecycle.state_history.is_empty());
}

#[test]
fn test_lifecycle_access_reactivates() {
    let mut lifecycle = MemoryLifecycle::with_state(MemoryState::Dormant);
    assert_eq!(lifecycle.state, MemoryState::Dormant);

    let changed = lifecycle.record_access();
    assert!(changed);
    assert_eq!(lifecycle.state, MemoryState::Active);
    assert_eq!(lifecycle.access_count, 2);
}

#[test]
fn test_lifecycle_suppression() {
    let mut lifecycle = MemoryLifecycle::new();

    lifecycle.suppress_from_competition("winner123".to_string(), 0.85, Duration::hours(2));

    assert_eq!(lifecycle.state, MemoryState::Unavailable);
    assert!(!lifecycle.is_suppression_expired());
    assert!(lifecycle.suppressed_by.contains(&"winner123".to_string()));

    // Access should not reactivate while suppressed
    let changed = lifecycle.record_access();
    assert!(!changed);
    assert_eq!(lifecycle.state, MemoryState::Unavailable);
}

#[test]
fn test_lifecycle_decay_detection() {
    let mut lifecycle = MemoryLifecycle::new();
    let config = StateDecayConfig {
        active_decay_hours: 0, // Immediate decay
        dormant_decay_days: 0, // Immediate decay
        ..Default::default()
    };

    // Should decay immediately
    assert!(lifecycle.should_decay_to_dormant(&config));

    lifecycle.transition_to(MemoryState::Dormant, StateTransitionReason::TimeDecay);
    assert!(lifecycle.should_decay_to_silent(&config));
}

#[test]
fn test_lifecycle_cue_reactivation() {
    let mut lifecycle = MemoryLifecycle::with_state(MemoryState::Silent);

    // Weak cue should fail
    let reactivated = lifecycle.try_reactivate_with_cue(0.5, 0.8);
    assert!(!reactivated);
    assert_eq!(lifecycle.state, MemoryState::Silent);

    // Strong cue should succeed
    let reactivated = lifecycle.try_reactivate_with_cue(0.9, 0.8);
    assert!(reactivated);
    assert_eq!(lifecycle.state, MemoryState::Dormant);
}

#[test]
fn test_lifecycle_state_history_limit() {
    let mut lifecycle = MemoryLifecycle::new();

    // Add many transitions
    for i in 0..100 {
        lifecycle.transition_to(
            if i % 2 == 0 {
                MemoryState::Dormant
            } else {
                MemoryState::Active
            },
            StateTransitionReason::Access,
        );
    }

    // History should be capped
    assert!(lifecycle.state_history.len() <= MAX_STATE_HISTORY_SIZE);
}

// ==================== Competition Tests ====================

#[test]
fn test_competition_manager() {
    let mut manager = CompetitionManager::new();

    let candidates = vec![
        CompetitionCandidate {
            memory_id: "mem1".to_string(),
            relevance_score: 0.95,
            similarity_to_query: 0.9,
            embedding: None,
        },
        CompetitionCandidate {
            memory_id: "mem2".to_string(),
            relevance_score: 0.80,
            similarity_to_query: 0.85,
            embedding: None,
        },
        CompetitionCandidate {
            memory_id: "mem3".to_string(),
            relevance_score: 0.70,
            similarity_to_query: 0.88,
            embedding: None,
        },
    ];

    let result = manager.run_competition(&candidates, 0.6);
    assert!(result.is_some());

    let result = result.unwrap();
    assert_eq!(result.winner_id, "mem1");
    assert!(result.suppressed_ids.contains(&"mem2".to_string()));
    assert!(result.suppressed_ids.contains(&"mem3".to_string()));
}

#[test]
fn test_competition_no_similar_candidates() {
    let mut manager = CompetitionManager::new();

    let candidates = vec![
        CompetitionCandidate {
            memory_id: "mem1".to_string(),
            relevance_score: 0.95,
            similarity_to_query: 0.9,
            embedding: None,
        },
        CompetitionCandidate {
            memory_id: "mem2".to_string(),
            relevance_score: 0.80,
            similarity_to_query: 0.2, // Very different
            embedding: None,
        },
    ];

    // High threshold means no competition
    let result = manager.run_competition(&candidates, 0.9);
    assert!(result.is_none());
}

#[test]
fn test_competition_win_count() {
    let mut manager = CompetitionManager::new();

    // Run two competitions with same winner
    for _ in 0..2 {
        let candidates = vec![
            CompetitionCandidate {
                memory_id: "winner".to_string(),
                relevance_score: 0.95,
                similarity_to_query: 0.9,
                embedding: None,
            },
            CompetitionCandidate {
                memory_id: "loser".to_string(),
                relevance_score: 0.80,
                similarity_to_query: 0.85,
                embedding: None,
            },
        ];
        manager.run_competition(&candidates, 0.5);
    }

    assert_eq!(manager.win_count("winner"), 2);
    assert_eq!(manager.suppression_count("loser"), 2);
}

// ==================== Pairwise Cosine Similarity Tests ====================

#[test]
fn test_cosine_similarity_identical_vectors() {
    let v = vec![1.0, 0.0, 0.0];
    let sim = cosine_similarity(&v, &v);
    assert!((sim - 1.0).abs() < 1e-9);
}

#[test]
fn test_cosine_similarity_orthogonal_vectors() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.0, 1.0, 0.0];
    let sim = cosine_similarity(&a, &b);
    assert!(sim.abs() < 1e-9);
}

#[test]
fn test_cosine_similarity_opposite_vectors() {
    let a = vec![1.0, 0.0];
    let b = vec![-1.0, 0.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim - (-1.0)).abs() < 1e-9);
}

#[test]
fn test_cosine_similarity_empty_vectors() {
    assert_eq!(cosine_similarity(&[], &[]), 0.0);
}

#[test]
fn test_cosine_similarity_mismatched_dimensions() {
    let a = vec![1.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    assert_eq!(cosine_similarity(&a, &b), 0.0);
}

#[test]
fn test_cosine_similarity_zero_vector() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    assert_eq!(cosine_similarity(&a, &b), 0.0);
}

#[test]
fn test_competition_with_embeddings_suppresses_similar() {
    let mut manager = CompetitionManager::new();

    // Two memories with very similar embeddings — should compete
    let candidates = vec![
        CompetitionCandidate {
            memory_id: "winner".to_string(),
            relevance_score: 0.95,
            similarity_to_query: 0.5, // query-similarity is low
            embedding: Some(vec![1.0, 0.0, 0.0]),
        },
        CompetitionCandidate {
            memory_id: "loser".to_string(),
            relevance_score: 0.80,
            similarity_to_query: 0.5, // query-similarity is low
            // Nearly identical embedding → cosine ≈ 0.995
            embedding: Some(vec![0.99, 0.1, 0.0]),
        },
    ];

    // With the old approximation: (0.5 + 0.5) / 2 = 0.5 < 0.7 → no competition
    // With pairwise cosine: ≈ 0.995 > 0.7 → competition triggered
    let result = manager.run_competition(&candidates, 0.7);
    assert!(
        result.is_some(),
        "Pairwise cosine should trigger competition"
    );
    assert!(
        result
            .unwrap()
            .suppressed_ids
            .contains(&"loser".to_string())
    );
}

#[test]
fn test_competition_with_embeddings_spares_dissimilar() {
    let mut manager = CompetitionManager::new();

    // Two memories both relevant to query but semantically distant
    let candidates = vec![
        CompetitionCandidate {
            memory_id: "jwt_tokens".to_string(),
            relevance_score: 0.95,
            similarity_to_query: 0.9,
            embedding: Some(vec![1.0, 0.0, 0.0]),
        },
        CompetitionCandidate {
            memory_id: "oauth_flows".to_string(),
            relevance_score: 0.80,
            similarity_to_query: 0.85,
            // Orthogonal embedding → cosine = 0.0
            embedding: Some(vec![0.0, 1.0, 0.0]),
        },
    ];

    // With the old approximation: (0.9 + 0.85) / 2 = 0.875 > 0.7 → would suppress
    // With pairwise cosine: 0.0 < 0.7 → no competition (correct!)
    let result = manager.run_competition(&candidates, 0.7);
    assert!(result.is_none(), "Orthogonal embeddings should not compete");
}

#[test]
fn test_competition_mixed_embeddings_falls_back() {
    let mut manager = CompetitionManager::new();

    // Winner has embedding, loser doesn't — fallback to approximation
    let candidates = vec![
        CompetitionCandidate {
            memory_id: "with_emb".to_string(),
            relevance_score: 0.95,
            similarity_to_query: 0.9,
            embedding: Some(vec![1.0, 0.0, 0.0]),
        },
        CompetitionCandidate {
            memory_id: "without_emb".to_string(),
            relevance_score: 0.80,
            similarity_to_query: 0.85,
            embedding: None,
        },
    ];

    // Fallback: (0.9 + 0.85) / 2 = 0.875 > 0.7
    let result = manager.run_competition(&candidates, 0.7);
    assert!(result.is_some(), "Should fall back to approximation");
}

// ==================== State Update Service Tests ====================

#[test]
fn test_state_update_service() {
    let service = StateUpdateService::with_config(StateDecayConfig {
        active_decay_hours: 0,
        dormant_decay_days: 0,
        auto_resolve_suppression: true,
        ..Default::default()
    });

    let mut lifecycle = MemoryLifecycle::new();
    let transitions = service.update_lifecycle(&mut lifecycle);

    // Should have decayed: Active -> Dormant -> Silent
    assert_eq!(lifecycle.state, MemoryState::Silent);
    assert_eq!(transitions.len(), 2);
}

#[test]
fn test_state_update_resolves_suppression() {
    let service = StateUpdateService::with_config(StateDecayConfig {
        auto_resolve_suppression: true,
        ..Default::default()
    });

    let mut lifecycle = MemoryLifecycle::new();
    lifecycle.transition_to(
        MemoryState::Unavailable,
        StateTransitionReason::CompetitionLoss {
            winner_id: "test".to_string(),
            similarity: 0.8,
        },
    );
    // Set suppression to already expired
    lifecycle.suppression_until = Some(Utc::now() - Duration::hours(1));

    let transitions = service.update_lifecycle(&mut lifecycle);

    assert_eq!(lifecycle.state, MemoryState::Dormant);
    assert_eq!(transitions.len(), 1);
    assert!(matches!(
        transitions[0].reason,
        StateTransitionReason::SuppressionExpired
    ));
}

#[test]
fn test_batch_update() {
    let service = StateUpdateService::with_config(StateDecayConfig {
        active_decay_hours: 0,
        dormant_decay_days: 1000, // Won't decay
        ..Default::default()
    });

    let mut lifecycles = vec![
        MemoryLifecycle::new(),
        MemoryLifecycle::new(),
        MemoryLifecycle::with_state(MemoryState::Dormant),
    ];

    let result = service.batch_update(&mut lifecycles);

    assert_eq!(result.active_to_dormant, 2);
    assert_eq!(result.dormant_to_silent, 0);
    assert_eq!(result.total_transitions, 2);
}

// ==================== Accessibility Calculator Tests ====================

#[test]
fn test_accessibility_calculator() {
    let calc = AccessibilityCalculator::default();
    let lifecycle = MemoryLifecycle::new();

    // Active memory just accessed should have high accessibility
    let score = calc.calculate(&lifecycle, 0.8);
    assert!(score > 0.8);
    assert!(score <= 1.0);
}

#[test]
fn test_accessibility_state_multipliers() {
    let calc = AccessibilityCalculator {
        recency_weight: 0.0,
        frequency_weight: 0.0,
        ..Default::default()
    };

    let mut lifecycle = MemoryLifecycle::new();
    let base_score = 1.0;

    // Active: full score
    let active_score = calc.calculate(&lifecycle, base_score);
    assert!(approx_eq(active_score, 1.0, 0.01));

    // Dormant: 0.7x
    lifecycle.state = MemoryState::Dormant;
    let dormant_score = calc.calculate(&lifecycle, base_score);
    assert!(approx_eq(dormant_score, 0.7, 0.01));

    // Silent: 0.3x
    lifecycle.state = MemoryState::Silent;
    let silent_score = calc.calculate(&lifecycle, base_score);
    assert!(approx_eq(silent_score, 0.3, 0.01));

    // Unavailable: 0.05x
    lifecycle.state = MemoryState::Unavailable;
    let unavailable_score = calc.calculate(&lifecycle, base_score);
    assert!(approx_eq(unavailable_score, 0.05, 0.01));
}

// ==================== State Time Accumulator Tests ====================

#[test]
fn test_state_time_accumulator() {
    let mut acc = StateTimeAccumulator::default();

    acc.add(MemoryState::Active, 3600);
    acc.add(MemoryState::Dormant, 7200);

    assert_eq!(acc.active_seconds, 3600);
    assert_eq!(acc.dormant_seconds, 7200);
    assert_eq!(acc.total_seconds(), 10800);

    let pct = acc.percentages();
    assert!(approx_eq(pct.active, 33.33, 0.1));
    assert!(approx_eq(pct.dormant, 66.67, 0.1));
}

// ==================== Memory State Info Tests ====================

#[test]
fn test_memory_state_info() {
    let lifecycle = MemoryLifecycle::new();
    let info = MemoryStateInfo::from_lifecycle(&lifecycle);

    assert_eq!(info.state, MemoryState::Active);
    assert_eq!(info.accessibility, 1.0);
    assert_eq!(info.access_count, 1);
    assert!(
        info.time_since_access.contains("just now") || info.time_since_access.contains("minute")
    );
}

#[test]
fn test_memory_state_info_suppressed() {
    let mut lifecycle = MemoryLifecycle::new();
    lifecycle.suppress_by_user(Duration::hours(2), Some("test reason".to_string()));

    let info = MemoryStateInfo::from_lifecycle(&lifecycle);

    assert_eq!(info.state, MemoryState::Unavailable);
    assert!(info.accessible_after.is_some());
    assert!(!info.recommendations.is_empty());
}

// ==================== Serialization Tests ====================

#[test]
fn test_memory_state_serialization() {
    let state = MemoryState::Dormant;
    let json = serde_json::to_string(&state).unwrap();
    assert_eq!(json, "\"dormant\"");

    let parsed: MemoryState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, state);
}

#[test]
fn test_lifecycle_serialization() {
    let lifecycle = MemoryLifecycle::new();
    let json = serde_json::to_string(&lifecycle).unwrap();
    let parsed: MemoryLifecycle = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.state, lifecycle.state);
    assert_eq!(parsed.access_count, lifecycle.access_count);
}
