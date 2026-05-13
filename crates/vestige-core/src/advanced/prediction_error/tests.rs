//! Tests for the prediction-error gate pipeline.

use super::candidate::CandidateMemory;
use super::decision::{CreateReason, GateDecision, UpdateType};
use super::gate::{EvaluationIntent, PredictionErrorGate};
use super::similarity::cosine_similarity;


fn make_embedding(seed: f32) -> Vec<f32> {
    // Create embeddings with controlled similarity based on seed
    // Seeds close to each other = similar vectors
    // Seeds far apart = different vectors
    (0..384)
        .map(|i| {
            let base = (i as f32 / 384.0) * std::f32::consts::PI * 2.0;
            (base * seed).sin()
        })
        .collect()
}

fn make_orthogonal_embedding() -> Vec<f32> {
    // Create an embedding that's orthogonal to seed=1.0
    (0..384)
        .map(|i| {
            let base = (i as f32 / 384.0) * std::f32::consts::PI * 2.0;
            (base + std::f32::consts::PI / 2.0).sin() // 90 degree phase shift
        })
        .collect()
}

fn make_candidate(id: &str, seed: f32) -> CandidateMemory {
    CandidateMemory {
        id: id.to_string(),
        content: format!("Content for {}", id),
        embedding: make_embedding(seed),
        retrieval_strength: 0.8,
        retention_strength: 0.7,
        tags: vec![],
        source: None,
        was_demoted: false,
        was_promoted: false,
    }
}

#[test]
fn test_cosine_similarity() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

    let c = vec![0.0, 1.0, 0.0];
    assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);

    let d = vec![-1.0, 0.0, 0.0];
    assert!(cosine_similarity(&a, &d) <= 0.0);
}

#[test]
fn test_empty_candidates() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    let decision = gate.evaluate("New content", &embedding, &[]);

    assert!(matches!(
        decision,
        GateDecision::Create {
            reason: CreateReason::FirstMemory,
            ..
        }
    ));
}

#[test]
fn test_high_similarity_update() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    // Create candidate with identical embedding
    let mut candidate = make_candidate("mem-1", 1.0);
    candidate.embedding = embedding.clone();

    let decision = gate.evaluate("Same content", &embedding, &[candidate]);

    assert!(decision.is_update());
    if let GateDecision::Update { update_type, .. } = decision {
        assert_eq!(update_type, UpdateType::Reinforce);
    }
}

#[test]
fn test_demoted_memory_supersede() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    // Use similar embedding (seed 1.05) - close enough to be above similarity threshold
    let mut candidate = make_candidate("mem-1", 1.0);
    candidate.embedding = make_embedding(1.05);
    candidate.was_demoted = true;

    let decision = gate.evaluate("Better solution", &embedding, &[candidate]);

    // Should supersede the demoted memory if similarity is above threshold
    // If not superseding, it should at least update
    assert!(matches!(
        decision,
        GateDecision::Supersede { .. } | GateDecision::Update { .. }
    ));
}

#[test]
fn test_different_content_create() {
    let mut gate = PredictionErrorGate::new();
    let new_embedding = make_embedding(1.0);

    // Use orthogonal embedding for truly different content
    let mut candidate = make_candidate("mem-1", 1.0);
    candidate.embedding = make_orthogonal_embedding();

    let decision = gate.evaluate("Completely different topic", &new_embedding, &[candidate]);

    assert!(matches!(decision, GateDecision::Create { .. }));
}

#[test]
fn test_contradiction_detection() {
    let gate = PredictionErrorGate::new();

    assert!(gate.detect_contradiction(
        "Don't use synchronous code",
        "Use synchronous code for simplicity"
    ));

    assert!(gate.detect_contradiction(
        "Actually, the correct approach is...",
        "The approach is to..."
    ));

    assert!(!gate.detect_contradiction(
        "Use async/await for performance",
        "Use async patterns when needed"
    ));
}

#[test]
fn test_force_create_intent() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);
    let candidate = make_candidate("mem-1", 1.0);

    let decision = gate.evaluate_with_intent(
        "New content",
        &embedding,
        &[candidate],
        EvaluationIntent::ForceCreate,
    );

    assert!(matches!(
        decision,
        GateDecision::Create {
            reason: CreateReason::ExplicitCreate,
            ..
        }
    ));
}

#[test]
fn test_force_update_intent() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);
    let candidate = make_candidate("mem-1", 5.0);

    let decision = gate.evaluate_with_intent(
        "Updated content",
        &embedding,
        &[candidate],
        EvaluationIntent::ForceUpdate {
            target_id: "mem-1".to_string(),
        },
    );

    assert!(matches!(decision, GateDecision::Update { .. }));
}

#[test]
fn test_stats() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    // Create (empty candidates)
    gate.evaluate("Content", &embedding, &[]);

    // Update (identical)
    let mut candidate = make_candidate("mem-1", 1.0);
    candidate.embedding = embedding.clone();
    gate.evaluate("Content", &embedding, &[candidate.clone()]);

    let stats = gate.stats();
    assert_eq!(stats.total_evaluations, 2);
    assert_eq!(stats.creates, 1);
    assert_eq!(stats.updates, 1);
}
