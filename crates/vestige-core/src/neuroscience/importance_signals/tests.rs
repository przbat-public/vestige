//! Module-level integration tests covering the full importance pipeline.

use chrono::Utc;

use super::*;

#[test]
fn test_novelty_signal_basic() {
    let mut novelty = NoveltySignal::new();
    let context = Context::current();

    // First time seeing content should be novel
    let score1 = novelty.compute("The quick brown fox jumps over the lazy dog", &context);
    assert!(score1 > 0.5, "New content should be novel");

    // Learn the pattern
    novelty.update_model("The quick brown fox jumps over the lazy dog");
    novelty.update_model("The quick brown fox jumps over the lazy dog");
    novelty.update_model("The quick brown fox jumps over the lazy dog");

    // Same content should be less novel
    let score2 = novelty.compute("The quick brown fox jumps over the lazy dog", &context);
    assert!(score2 < score1, "Repeated content should be less novel");
}

#[test]
fn test_arousal_signal_emotional_content() {
    let arousal = ArousalSignal::new();

    // Neutral content
    let neutral_score = arousal.compute("The meeting is scheduled for tomorrow.");

    // Highly emotional content
    let emotional_score =
        arousal.compute("CRITICAL ERROR!!! Production database is DOWN! Data loss imminent!");

    assert!(
        emotional_score > neutral_score,
        "Emotional content should have higher arousal"
    );
    assert!(
        emotional_score > 0.6,
        "Highly emotional content should score high"
    );
}

#[test]
fn test_arousal_signal_markers() {
    let arousal = ArousalSignal::new();
    let markers = arousal.detect_emotional_markers("URGENT: Critical failure!!!");

    assert!(!markers.is_empty(), "Should detect emotional markers");

    let has_keyword = markers
        .iter()
        .any(|m| m.marker_type == MarkerType::IntensityKeyword);
    assert!(has_keyword, "Should detect intensity keyword");
}

#[test]
fn test_reward_signal_tracking() {
    let reward = RewardSignal::new();

    // Record positive outcomes
    reward.record_outcome("mem-1", OutcomeType::Helpful);
    reward.record_outcome("mem-1", OutcomeType::VeryHelpful);
    reward.record_outcome("mem-1", OutcomeType::Helpful);

    let score = reward.compute("mem-1");
    assert!(
        score > 0.5,
        "Memory with positive outcomes should score high"
    );

    // Record negative outcomes for different memory
    reward.record_outcome("mem-2", OutcomeType::NotHelpful);
    reward.record_outcome("mem-2", OutcomeType::NotHelpful);

    let neg_score = reward.compute("mem-2");
    assert!(
        neg_score < 0.5,
        "Memory with negative outcomes should score low"
    );
}

#[test]
fn test_attention_signal_learning_mode() {
    let attention = AttentionSignal::new();

    // Create a learning-like session
    let learning_session = Session {
        session_id: "s1".to_string(),
        start_time: Utc::now(),
        duration_minutes: 45.0,
        query_count: 20,
        edit_count: 2,
        unique_memories_accessed: 15,
        viewed_docs: true,
        query_topics: vec!["rust".to_string(), "async".to_string(), "tokio".to_string()],
    };

    assert!(
        attention.detect_learning_mode(&learning_session),
        "Should detect learning mode"
    );
}

#[test]
fn test_composite_importance() {
    let signals = ImportanceSignals::new();
    let context = Context::current()
        .with_project("test-project")
        .with_learning_session(true);

    // Test with emotional, novel content
    let score = signals.compute_importance(
        "BREAKTHROUGH: Solved the critical performance issue that was blocking release!!!",
        &context,
    );

    assert!(score.composite > 0.5, "Important content should score high");
    assert!(
        score.arousal > 0.5,
        "Emotional content should have high arousal"
    );

    // Check encoding boost
    assert!(
        score.encoding_boost >= 1.0,
        "High importance should boost encoding"
    );
}

#[test]
fn test_importance_score_explanation() {
    let signals = ImportanceSignals::new();
    let context = Context::current();

    let score = signals.compute_importance("Critical error in production system!", &context);

    let explanation = score.explain();
    assert!(!explanation.is_empty(), "Should provide explanation");

    let summary = score.summary();
    assert!(
        summary.contains("Importance"),
        "Summary should contain score"
    );
}

#[test]
fn test_composite_weights() {
    let weights = CompositeWeights::new(1.0, 2.0, 1.0, 1.0);

    assert!(weights.is_valid(), "Weights should sum to 1.0");
    assert!(
        weights.arousal > weights.novelty,
        "Arousal should have higher weight"
    );
}

#[test]
fn test_consolidation_priority() {
    assert!(ConsolidationPriority::Critical > ConsolidationPriority::High);
    assert!(ConsolidationPriority::High > ConsolidationPriority::Normal);
    assert!(ConsolidationPriority::Normal > ConsolidationPriority::Low);

    assert!(ConsolidationPriority::Critical.decay_modifier() < 1.0);
    assert!(ConsolidationPriority::Low.decay_modifier() > 1.0);
}

#[test]
fn test_sentiment_analyzer() {
    let analyzer = SentimentAnalyzer::new();

    let positive = analyzer.analyze("This is amazing and wonderful!");
    assert!(positive.polarity > 0.0, "Should detect positive sentiment");

    let negative = analyzer.analyze("This is terrible and broken.");
    assert!(negative.polarity < 0.0, "Should detect negative sentiment");

    let negated = analyzer.analyze("This is not bad at all.");
    // Negation should flip sentiment
    assert!(negated.polarity >= 0.0, "Negation should flip sentiment");
}

#[test]
fn test_context_builder() {
    let context = Context::current()
        .with_session("session-123")
        .with_project("vestige")
        .with_query("importance signals")
        .with_learning_session(true)
        .with_emotional_context("focused")
        .with_tags(vec!["rust".to_string(), "memory".to_string()]);

    assert_eq!(context.session_id, Some("session-123".to_string()));
    assert_eq!(context.project, Some("vestige".to_string()));
    assert_eq!(context.recent_queries.len(), 1);
    assert!(context.learning_session_active);
}

#[test]
fn test_access_pattern() {
    let mut pattern = AccessPattern::new();

    pattern.add_access("mem-1", 0.0);
    pattern.add_access("mem-2", 5.0);
    pattern.add_access("mem-1", 3.0);
    pattern.add_query("search query");

    assert_eq!(pattern.unique_memories(), 2);
    assert_eq!(pattern.avg_inter_access_time(), 4.0);
}
