//! Tests for the prediction-error gate pipeline.

use super::candidate::CandidateMemory;
use super::decision::{CreateReason, GateDecision, SupersedeReason, UpdateType};
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

/// Build an embedding whose cosine to `base` is `theta.cos()`.
///
/// Mixing in the orthogonal direction moves the vector off `base` by a known
/// angle, which is how the tests reach a chosen point inside a similarity band
/// instead of guessing at seeds. The achieved similarity is asserted, so a
/// failure here means the helper — not the gate — is wrong.
fn make_embedding_at_cosine(base: &[f32], theta: f32) -> Vec<f32> {
    let orthogonal = make_orthogonal_embedding();
    let mixed: Vec<f32> = base
        .iter()
        .zip(orthogonal.iter())
        .map(|(b, o)| b * theta.cos() + o * theta.sin())
        .collect();
    let achieved = cosine_similarity(base, &mixed);
    assert!(
        (achieved - theta.cos()).abs() < 0.02,
        "helper premise: wanted cos {:.3}, built {achieved:.3}",
        theta.cos()
    );
    mixed
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

    assert!(
        gate.detect_contradiction("Don't use synchronous code", "Use synchronous code for simplicity")
            .positive
    );

    assert!(
        gate.detect_contradiction(
            "Actually, the correct approach is...",
            "The approach is to..."
        )
        .positive
    );

    assert!(
        !gate
            .detect_contradiction(
                "Use async/await for performance",
                "Use async patterns when needed"
            )
            .positive
    );
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

#[test]
fn near_identical_correction_supersedes_instead_of_reinforcing() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    // Identical embedding → similarity 1.0, i.e. above `near_identical_threshold`.
    // A correction is by construction very similar to the memory it corrects, so
    // similarity alone must not decide. Regression: the near-identical short-circuit
    // used to run first and returned `Update { Reinforce }`, leaving the stale
    // memory in place and `correction_threshold` unreachable.
    // The content pair is the one `test_contradiction_detection` pins as a
    // contradiction, so this test fails for the ordering bug and nothing else.
    let mut candidate = make_candidate("mem-1", 1.0);
    candidate.embedding = embedding.clone();
    candidate.content = "Use synchronous code for simplicity".to_string();

    let decision = gate.evaluate("Don't use synchronous code", &embedding, &[candidate]);

    assert!(
        matches!(
            decision,
            GateDecision::Supersede {
                supersede_reason: SupersedeReason::Correction,
                ..
            }
        ),
        "a near-identical contradiction must supersede, got {decision:?}"
    );
}

/// Regression, real pair from a production store (2026-09-20).
///
/// A Polish adverbial — "w rzeczywistości", "in reality" — sits inside the new
/// memory's own causal explanation. The detector reports a single
/// `CorrectionPhrase` at 0.6 confidence, and because both memories belong to
/// the same project the cosine similarity clears `correction_threshold` (0.70).
/// The gate retired the older memory as a `Correction` on that alone, so a
/// decision acquired `valid_until` one second after it was written.
///
/// One mid-confidence signal is not grounds for retirement: `Update`/`Create`
/// leave the old memory intact, `Supersede` claims it stopped being true.
#[test]
fn lone_correction_phrase_does_not_retire_a_memory() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    let mut candidate = make_candidate("decision-tutorial", 1.0);
    candidate.embedding = embedding.clone();
    candidate.content = "Materiał zaczyna się od stanowiska pracy: instalacji kompilatora. \
         Odbiorca nie napisał wcześniej żadnego programu i nie wie, czym jest rejestr."
        .to_string();

    let decision = gate.evaluate(
        "Objaw: na panelu pojawiały się poziome pasy będące kopią tego, co wyżej. Przyczyna: \
         decyzja o pominięciu wysłania pasma porównywała je ze wspólnym cieniem, więc w \
         rzeczywistości sprawdzała, czy pasmo jest takie samo jak poprzednie.",
        &embedding,
        &[candidate],
    );

    assert!(
        !matches!(decision, GateDecision::Supersede { .. }),
        "a lone 0.6-confidence correction phrase retired a memory: {decision:?}"
    );
}

/// Regression, real behaviour observed in a production store (2026-09-20).
///
/// The 0.75–0.92 similarity band — same project vocabulary, different claim —
/// used to return `Update { Merge }`, and the storage layer implemented that by
/// rewriting the existing memory as `"{old}\n\n[Updated YYYY-MM-DD]\n{new}"`.
/// Three memories in that store ended up carrying an unrelated lesson appended
/// under somebody else's heading, and two of them duplicated lessons that also
/// existed as their own memories.
///
/// Similar vocabulary is not the same claim: the new content goes in as its own
/// memory, linked to the neighbour it resembles.
#[test]
fn similar_but_distinct_content_is_stored_separately_and_linked() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    let mut candidate = make_candidate("mem-1", 1.0);
    // 0.83: above `similarity_threshold` (0.75), below
    // `near_identical_threshold` (0.92) — the band that used to merge.
    candidate.embedding = make_embedding_at_cosine(&embedding, 0.6);
    candidate.content = "The tutorial starts at the workbench: compiler install first, then a \
         first program that ends with something visible on the panel."
        .to_string();

    let decision = gate.evaluate(
        "Every teaching stage is a standalone program that compiles cleanly and gives the \
         learner a visible result.",
        &embedding,
        &[candidate],
    );

    match decision {
        GateDecision::Create {
            reason,
            related_memory_ids,
            ..
        } => {
            assert_eq!(
                reason,
                CreateReason::DifferentDomain,
                "the decision must say why no existing memory was rewritten"
            );
            assert_eq!(
                related_memory_ids,
                vec!["mem-1".to_string()],
                "the neighbour it resembles must still be named, so the caller links them"
            );
        }
        other => panic!("expected a separate, linked memory, got {other:?}"),
    }
}

#[test]
fn near_identical_agreement_still_reinforces() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    let mut candidate = make_candidate("mem-1", 1.0);
    candidate.embedding = embedding.clone();
    candidate.content = "The deploy is on Friday".to_string();

    let decision = gate.evaluate("The deploy is on Friday", &embedding, &[candidate]);

    assert!(
        matches!(
            decision,
            GateDecision::Update {
                update_type: UpdateType::Reinforce,
                ..
            }
        ),
        "an agreeing near-identical memory must still reinforce, got {decision:?}"
    );
}

#[test]
fn equal_similarity_candidates_resolve_by_memory_id_not_input_order() {
    let mut gate = PredictionErrorGate::new();
    let embedding = make_embedding(1.0);

    // Two candidates the gate cannot separate by similarity. Before the shared
    // identity tiebreak the winner was simply the first row the storage layer
    // returned, so one unchanged database produced different ingest decisions
    // depending on the order candidates arrived in.
    let mut aaa = make_candidate("aaa", 1.0);
    aaa.embedding = embedding.clone();
    let mut zzz = make_candidate("zzz", 1.0);
    zzz.embedding = embedding.clone();

    let forward = gate.evaluate("Content", &embedding, &[aaa.clone(), zzz.clone()]);
    let backward = gate.evaluate("Content", &embedding, &[zzz, aaa]);

    assert_eq!(forward.target_id(), Some("zzz"));
    assert_eq!(backward.target_id(), Some("zzz"));
}
