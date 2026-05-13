use super::*;
use chrono::Utc;

#[test]
fn test_user_model_creation() {
    let model = UserModel::new();
    assert!(model.interests.is_empty());
    assert!(model.recent_queries.is_empty());
}

#[test]
fn test_interest_update() {
    let mut model = UserModel::new();
    model.update_interest("rust", 0.8);

    assert!(model.interests.contains_key("rust"));
    assert!(model.interests.get("rust").unwrap() > &0.0);
}

#[test]
fn test_query_recording() {
    let mut model = UserModel::new();
    model.record_query("how to use async", &["rust", "async"]);

    assert_eq!(model.recent_queries.len(), 1);
    assert!(model.interests.contains_key("rust"));
    assert!(model.interests.contains_key("async"));
}

#[test]
fn test_temporal_patterns() {
    let mut patterns = TemporalPatterns::new();
    let now = Utc::now();

    patterns.record_activity(now, "coding", 0.9);

    let topics = patterns.topics_for_time(now);
    assert!(!topics.is_empty());
}

#[test]
fn test_session_context() {
    let mut context = SessionContext::new();
    context.add_query("test query".to_string());
    context.add_active_file("/src/main.rs".to_string());

    assert_eq!(context.recent_queries.len(), 1);
    assert_eq!(context.active_files.len(), 1);
    assert!(context.is_active());
}

#[test]
fn test_predictive_memory_creation() {
    let predictor = PredictiveMemory::new();
    assert_eq!(predictor.config().min_confidence, DEFAULT_MIN_CONFIDENCE);
}

#[test]
fn test_record_query() {
    let predictor = PredictiveMemory::new();
    let result = predictor.record_query("authentication", &["security", "jwt"]);
    assert!(result.is_ok());
}

#[test]
fn test_record_interest() {
    let predictor = PredictiveMemory::new();
    let result = predictor.record_interest("machine learning", 0.9);
    assert!(result.is_ok());

    let interests = predictor.get_top_interests(5).unwrap();
    assert!(!interests.is_empty());
}

#[test]
fn test_prediction_reason_description() {
    let reason = PredictionReason::InterestBased {
        topic: "rust".to_string(),
        weight: 0.8,
    };

    let desc = reason.description();
    assert!(desc.contains("rust"));
    assert!(desc.contains("80%"));
}

#[test]
fn test_predict_needed_memories() {
    let predictor = PredictiveMemory::new();

    // Record some activity
    predictor.record_interest("rust", 0.9).unwrap();
    predictor
        .record_query("async programming", &["rust", "async"])
        .unwrap();

    let context = SessionContext::new();
    let predictions = predictor.predict_needed_memories(&context);

    assert!(predictions.is_ok());
}

#[test]
fn test_novelty_signal() {
    let predictor = PredictiveMemory::new();

    // Record interest in Rust multiple times to build up the weight
    // (INTEREST_LEARNING_RATE is 0.1, so we need multiple calls to reach > 0.5)
    for _ in 0..20 {
        predictor.record_interest("rust", 1.0).unwrap();
    }

    // Novel topic should have high novelty
    let novelty = predictor
        .signal_novelty("mem-1", &["python".to_string()])
        .unwrap();
    assert!(novelty > 0.5, "Python should be novel (got {})", novelty);

    // Familiar topic should have lower novelty
    let novelty = predictor
        .signal_novelty("mem-2", &["rust".to_string()])
        .unwrap();
    assert!(novelty < 0.5, "Rust should be familiar (got {})", novelty);
}

#[test]
fn test_prediction_accuracy() {
    let predictor = PredictiveMemory::new();

    // Initially should be 0.0 (no history)
    let accuracy = predictor.prediction_accuracy().unwrap();
    assert_eq!(accuracy, 0.0);
}
