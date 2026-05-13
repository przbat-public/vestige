use super::*;
use chrono::{Duration, Timelike, Utc};

#[test]
fn test_time_of_day() {
    // Test morning
    assert_eq!(
        TimeOfDay::from_datetime(Utc::now().with_hour(8).unwrap()),
        TimeOfDay::Morning
    );

    // Test afternoon
    assert_eq!(
        TimeOfDay::from_datetime(Utc::now().with_hour(14).unwrap()),
        TimeOfDay::Afternoon
    );

    // Test evening
    assert_eq!(
        TimeOfDay::from_datetime(Utc::now().with_hour(18).unwrap()),
        TimeOfDay::Evening
    );

    // Test night
    assert_eq!(
        TimeOfDay::from_datetime(Utc::now().with_hour(23).unwrap()),
        TimeOfDay::Night
    );

    // Test adjacency
    assert!(TimeOfDay::Morning.is_adjacent(&TimeOfDay::Afternoon));
    assert!(!TimeOfDay::Morning.is_adjacent(&TimeOfDay::Evening));
}

#[test]
fn test_recency_bucket() {
    let now = Utc::now();

    // Test very recent
    assert_eq!(
        RecencyBucket::from_datetime(now - Duration::minutes(30)),
        RecencyBucket::VeryRecent
    );

    // Test today
    assert_eq!(
        RecencyBucket::from_datetime(now - Duration::hours(5)),
        RecencyBucket::Today
    );

    // Test this week
    assert_eq!(
        RecencyBucket::from_datetime(now - Duration::days(3)),
        RecencyBucket::ThisWeek
    );

    // Test adjacency
    assert!(RecencyBucket::VeryRecent.is_adjacent(&RecencyBucket::Today));
    assert!(!RecencyBucket::VeryRecent.is_adjacent(&RecencyBucket::ThisMonth));
}

#[test]
fn test_topical_context() {
    let mut topical = TopicalContext::new();
    topical.add_topic("authentication");
    topical.add_topic("security");
    topical.extract_keywords_from("implementing OAuth2 authentication flow");

    assert!(
        topical
            .active_topics
            .contains(&"authentication".to_string())
    );
    assert!(topical.keywords.contains(&"oauth2".to_string()));

    let terms = topical.all_terms();
    assert!(terms.contains("authentication"));
}

#[test]
fn test_encoding_context() {
    let mut ctx = EncodingContext::new();
    ctx.add_topic("api-design");
    ctx.set_project("vestige");

    assert!(
        ctx.topical
            .active_topics
            .contains(&"api-design".to_string())
    );
    assert_eq!(ctx.session.project, Some("vestige".to_string()));
}

#[test]
fn test_context_matcher_same_context() {
    let matcher = ContextMatcher::new();

    // Create contexts with actual content to match
    let mut ctx1 = EncodingContext::new();
    ctx1.add_topic("authentication");
    ctx1.session.project = Some("test-project".to_string());

    let ctx2 = ctx1.clone();

    let similarity = matcher.match_contexts(&ctx1, &ctx2);
    assert!(
        similarity > 0.8,
        "Same context should have high similarity, got {}",
        similarity
    );
}

#[test]
fn test_context_matcher_different_topics() {
    let matcher = ContextMatcher::new();

    let mut ctx1 = EncodingContext::new();
    ctx1.add_topic("authentication");
    ctx1.add_topic("security");

    let mut ctx2 = EncodingContext::new();
    ctx2.add_topic("database");
    ctx2.add_topic("performance");

    let similarity = matcher.match_contexts(&ctx1, &ctx2);
    assert!(
        similarity < 0.5,
        "Different topics should have low similarity"
    );
}

#[test]
fn test_context_reinstatement() {
    let mut ctx = EncodingContext::new();
    ctx.topical.active_topics = vec!["authentication".to_string()];
    ctx.session.project = Some("vestige".to_string());

    let reinstatement = ContextReinstatement::from_context("mem-123", &ctx);

    assert!(reinstatement.has_hints());
    assert!(reinstatement.topical_hint.is_some());
    assert!(reinstatement.session_hint.is_some());

    let hint = reinstatement.combined_hint().unwrap();
    assert!(hint.contains("authentication"));
    assert!(hint.contains("vestige"));
}

#[test]
fn test_emotional_context() {
    let positive = EmotionalContext::from_sentiment(0.7, 0.8);
    assert!(positive.is_positive());
    assert!(positive.is_high_arousal());

    let negative = EmotionalContext::from_sentiment(-0.5, 0.3);
    assert!(negative.is_negative());
    assert!(!negative.is_high_arousal());
}

#[test]
fn test_context_weights_normalization() {
    let mut weights = ContextWeights {
        temporal: 1.0,
        topical: 2.0,
        session: 1.0,
        emotional: 0.0,
    };
    weights.normalize();

    let sum = weights.temporal + weights.topical + weights.session + weights.emotional;
    assert!((sum - 1.0).abs() < 0.001, "Weights should sum to 1.0");
}
