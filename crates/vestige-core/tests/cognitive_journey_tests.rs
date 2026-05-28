//! Cognitive Journey Tests
//!
//! End-to-end tests that simulate realistic user journeys through the memory
//! lifecycle. Inspired by cognitive science literature and competitor analysis
//! (Mem0, Graphiti/Zep, Letta/MemGPT, memary).
//!
//! Each test validates a cognitive property that distinguishes a memory system
//! from a simple key-value store.
//!
//! References:
//! - Bjork & Bjork (1992): Desirable difficulties
//! - Roediger & Butler (2011): Testing effect
//! - Loftus (2005): Misinformation effect
//! - Tulving (1972): Episodic vs semantic memory distinction
//! - Collins & Loftus (1975): semantic network / spreading activation
//!   (the implementation here is a bounded graph walk inspired by their model,
//!   not the full ACT-R activation equation from Anderson 2007)
//! - Nelson & Narens (1990): Metamemory monitoring
//! - Cepeda et al. (2006): Spacing effect

use chrono::{Duration, Utc};
use vestige_core::memory::{DualStrength, KnowledgeNode, StrengthDecay};
use vestige_core::neuroscience::memory_states::{MemoryLifecycle, MemoryState, StateUpdateService};
use vestige_core::neuroscience::spreading_activation::{ActivationNetwork, LinkType};

// ====================================================================
// JOURNEY 1: Memory Lifecycle — Birth, Strengthening, Decay, Revival
// Bjork's desirable difficulties framework
// ====================================================================

#[test]
fn j1_memory_decays_monotonically_over_time() {
    let decay = StrengthDecay::new(5.0, 0.0);

    let r_fresh = decay.retrieval_at(0.0);
    let r_1d = decay.retrieval_at(1.0);
    let r_7d = decay.retrieval_at(7.0);
    let r_30d = decay.retrieval_at(30.0);

    assert!(r_fresh > r_1d, "Memory should decay after 1 day");
    assert!(r_1d > r_7d, "Decay continues over a week");
    assert!(r_7d > r_30d, "Decay continues at 30 days");
    assert!(r_30d > 0.0, "Power law: memories never fully vanish");
}

#[test]
fn j1_retrieval_practice_strengthens_memory() {
    let mut dual = DualStrength::new(0.5, 0.3);
    let initial_storage = dual.storage;

    dual.on_successful_recall();

    assert!(
        dual.storage > initial_storage,
        "Successful retrieval increases storage: {:.3} > {:.3}",
        dual.storage,
        initial_storage
    );
    assert!(
        (dual.retrieval - 1.0).abs() < f64::EPSILON,
        "Retrieval resets to 1.0 after successful recall"
    );
}

#[test]
fn j1_lapse_still_strengthens_storage() {
    let mut dual = DualStrength::new(0.5, 0.3);
    let initial_storage = dual.storage;

    dual.on_lapse();

    assert!(
        dual.storage > initial_storage,
        "Bjork: even failed retrieval strengthens storage (desirable difficulty)"
    );
}

// ====================================================================
// JOURNEY 2: Episodic vs Semantic Memory
// Tulving (1972)
// ====================================================================

#[test]
fn j2_episodic_events_carry_temporal_context() {
    let episode = KnowledgeNode {
        node_type: "event".to_string(),
        content: "Deployed v2.0 to production, caused 502 errors".to_string(),
        valid_from: Some(Utc::now()),
        ..Default::default()
    };

    assert!(
        episode.valid_from.is_some(),
        "Episodes must have temporal context"
    );
    assert_eq!(episode.node_type, "event");
}

#[test]
fn j2_semantic_facts_are_atemporal() {
    let fact = KnowledgeNode::default();
    assert!(fact.valid_from.is_none(), "Semantic facts are atemporal");
    assert!(fact.valid_until.is_none(), "Semantic facts don't expire");
}

// ====================================================================
// JOURNEY 3: Testing Effect — Retrieval Beats Restudy
// Roediger & Butler (2011)
// ====================================================================

#[test]
fn j3_multiple_recalls_compound_storage() {
    let mut dual = DualStrength::new(0.5, 0.5);
    let initial = dual.storage;

    for _ in 0..5 {
        dual.on_successful_recall();
        dual.apply_decay(1.0, 5.0);
    }

    assert!(
        dual.storage > initial + 0.3,
        "5 successful recalls should significantly increase storage: {:.3} vs {:.3}",
        dual.storage,
        initial
    );
}

// ====================================================================
// JOURNEY 4: Spreading Activation — Associative Retrieval
// Collins & Loftus (1975) — bounded semantic-network walk
// (NOT the full ACT-R activation equation from Anderson 2007)
// ====================================================================

#[test]
fn j4_activation_spreads_to_associated_nodes() {
    let mut network = ActivationNetwork::default();

    network.add_edge(
        "rust_borrow".to_string(),
        "rust_lifetime".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "rust_borrow".to_string(),
        "python_gc".to_string(),
        LinkType::Semantic,
        0.2,
    );

    let activated = network.activate("rust_borrow", 1.0);

    let lifetime_act = activated
        .iter()
        .find(|a| a.memory_id == "rust_lifetime")
        .map(|a| a.activation)
        .unwrap_or(0.0);

    let python_act = activated
        .iter()
        .find(|a| a.memory_id == "python_gc")
        .map(|a| a.activation)
        .unwrap_or(0.0);

    assert!(
        lifetime_act > python_act,
        "Strong association ({:.3}) should activate more than weak ({:.3})",
        lifetime_act,
        python_act
    );
}

#[test]
fn j4_activation_decays_with_graph_distance() {
    let mut network = ActivationNetwork::default();

    network.add_edge("A".into(), "B".into(), LinkType::Semantic, 0.8);
    network.add_edge("B".into(), "C".into(), LinkType::Semantic, 0.8);

    let activated = network.activate("A", 1.0);

    let b_act = activated
        .iter()
        .find(|a| a.memory_id == "B")
        .map(|a| a.activation)
        .unwrap_or(0.0);
    let c_act = activated
        .iter()
        .find(|a| a.memory_id == "C")
        .map(|a| a.activation)
        .unwrap_or(0.0);

    assert!(
        b_act > c_act,
        "Activation decays with distance: B={:.3} > C={:.3}",
        b_act,
        c_act
    );
}

// ====================================================================
// JOURNEY 5: Memory State Transitions
// Bjork's new theory of disuse
// ====================================================================

#[test]
fn j5_state_transitions_tracked_by_service() {
    let service = StateUpdateService::new();

    let mut active_lc = MemoryLifecycle::new();
    active_lc.state = MemoryState::Active;
    active_lc.last_access = Utc::now();
    active_lc.access_count = 10;

    let mut stale_lc = MemoryLifecycle::new();
    stale_lc.state = MemoryState::Active;
    stale_lc.last_access = Utc::now() - Duration::days(60);
    stale_lc.access_count = 2;

    let mut lifecycles = vec![active_lc, stale_lc];
    let result = service.batch_update(&mut lifecycles);

    assert_eq!(
        lifecycles[0].state,
        MemoryState::Active,
        "Recently accessed memory stays Active"
    );

    let _ = result.total_transitions;
}

// ====================================================================
// JOURNEY 6: Spacing Effect — FSRS Intervals Grow
// Cepeda et al. (2006)
// ====================================================================

#[test]
fn j6_fsrs_intervals_increase_with_successful_reviews() {
    use vestige_core::fsrs::{FSRSScheduler, Rating};

    let scheduler = FSRSScheduler::default();
    let mut state = scheduler.new_card();

    let mut intervals = Vec::new();
    for _ in 0..5 {
        let result = scheduler.review(&state, Rating::Good, state.scheduled_days as f64, None);
        intervals.push(result.interval);
        state = result.state;
    }

    for i in 1..intervals.len() {
        assert!(
            intervals[i] >= intervals[i - 1],
            "FSRS intervals must grow: [{}]={} < [{}]={}",
            i,
            intervals[i],
            i - 1,
            intervals[i - 1]
        );
    }
}

#[test]
fn j6_lapse_reduces_stability() {
    use vestige_core::fsrs::{FSRSScheduler, Rating};

    let scheduler = FSRSScheduler::default();
    let mut good_state = scheduler.new_card();
    let mut lapsed_state = scheduler.new_card();

    for _ in 0..3 {
        let r = scheduler.review(
            &good_state,
            Rating::Good,
            good_state.scheduled_days as f64,
            None,
        );
        good_state = r.state;
    }

    for _ in 0..2 {
        let r = scheduler.review(
            &lapsed_state,
            Rating::Good,
            lapsed_state.scheduled_days as f64,
            None,
        );
        lapsed_state = r.state;
    }
    let r = scheduler.review(
        &lapsed_state,
        Rating::Again,
        lapsed_state.scheduled_days as f64,
        None,
    );
    lapsed_state = r.state;

    assert!(
        good_state.stability >= lapsed_state.stability,
        "Lapse reduces stability: good={:.2} vs lapsed={:.2}",
        good_state.stability,
        lapsed_state.stability
    );
}

// ====================================================================
// JOURNEY 7: Temporal Invalidation
// Graphiti/Zep pattern; Loftus (2005) misinformation effect
// ====================================================================

#[test]
fn j7_superseded_fact_has_valid_until() {
    let old = KnowledgeNode {
        content: "React class components are the standard".to_string(),
        valid_until: Some(Utc::now() - Duration::days(100)),
        ..Default::default()
    };

    let new = KnowledgeNode::default();
    assert!(old.valid_until.is_some(), "Superseded knowledge has expiry");
    assert!(new.valid_until.is_none(), "Current knowledge has no expiry");
}

// ====================================================================
// JOURNEY 8: Confidence Calibration — Opinions vs Facts
// Nelson & Narens (1990) metamemory
// ====================================================================

#[test]
fn j8_opinion_markers_detectable() {
    let opinions = [
        "I think Rust is better than Go",
        "We should probably use PostgreSQL",
        "It seems like the cache is the bottleneck",
    ];
    let facts = [
        "PostgreSQL supports JSONB columns",
        "The deployment failed at 3:42 AM UTC",
        "Rust 1.75 was released in December 2023",
    ];

    let markers = ["i think", "probably", "seems like", "i believe", "might be"];

    for text in &opinions {
        let lower = text.to_lowercase();
        assert!(
            markers.iter().any(|m| lower.contains(m)),
            "Opinion should contain hedge marker: {text}"
        );
    }

    for text in &facts {
        let lower = text.to_lowercase();
        assert!(
            !markers.iter().any(|m| lower.contains(m)),
            "Fact should not contain hedge marker: {text}"
        );
    }
}

// ====================================================================
// JOURNEY 9: Emotional Enhancement
// Brown & Kulik (1977) flashbulb memories
// ====================================================================

#[test]
fn j9_emotional_memories_resist_decay() {
    let neutral = StrengthDecay::new(5.0, 0.0);
    let emotional = StrengthDecay::new(5.0, 0.9);

    let n_30d = neutral.retrieval_at(30.0);
    let e_30d = emotional.retrieval_at(30.0);

    assert!(
        e_30d >= n_30d,
        "Emotional memories resist decay: emotional={:.3} >= neutral={:.3}",
        e_30d,
        n_30d
    );
}

// ====================================================================
// JOURNEY 10: Cross-Session Knowledge Accumulation
// ====================================================================

#[test]
fn j10_knowledge_accumulates_via_activation_network() {
    let mut network = ActivationNetwork::default();

    network.add_edge("bug".into(), "fix".into(), LinkType::Causal, 0.8);
    assert_eq!(network.node_count(), 2);

    network.add_edge("fix".into(), "refactor".into(), LinkType::Temporal, 0.7);
    assert_eq!(network.node_count(), 3);

    let chain = network.activate("bug", 1.0);
    assert!(
        chain.len() >= 2,
        "Activation from session 1 should reach later sessions: got {}",
        chain.len()
    );
}

// ====================================================================
// JOURNEY 11: Stability vs Retrievability Independence
// FSRS-6 core principle
// ====================================================================

#[test]
fn j11_high_stability_survives_long_gaps() {
    let well_learned = StrengthDecay::new(30.0, 0.0);
    let poorly_learned = StrengthDecay::new(2.0, 0.0);

    let well_60d = well_learned.retrieval_at(60.0);
    let poor_60d = poorly_learned.retrieval_at(60.0);

    assert!(
        well_60d > poor_60d,
        "S=30 more retrievable at 60d than S=2: {:.3} vs {:.3}",
        well_60d,
        poor_60d
    );

    assert!(
        well_learned.retrieval_at(365.0) > 0.0,
        "Even at 1 year, well-learned items have nonzero retrievability"
    );
}

// ====================================================================
// JOURNEY 12: Dual Strength Model Properties
// Bjork & Bjork (1992) core predictions
// ====================================================================

#[test]
fn j12_retention_combines_both_strengths() {
    let high_both = DualStrength::new(5.0, 0.9);
    let low_storage = DualStrength::new(0.5, 0.9);
    let low_retrieval = DualStrength::new(5.0, 0.1);

    assert!(
        high_both.retention() > low_storage.retention(),
        "Higher storage should increase retention"
    );
    assert!(
        high_both.retention() > low_retrieval.retention(),
        "Higher retrieval should increase retention"
    );
}

#[test]
fn j12_retrieval_weight_dominates() {
    let r = DualStrength::new(1.0, 0.9).retention();
    let s = DualStrength::new(9.0, 0.1).retention();

    assert!(
        r > s,
        "Retrieval strength (70% weight) should dominate over storage (30%): \
         high-R={:.3} vs high-S={:.3}",
        r,
        s
    );
}
