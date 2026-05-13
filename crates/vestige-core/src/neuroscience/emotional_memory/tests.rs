//! Tests for the emotional-memory pipeline.

use super::evaluation::EmotionCategory;
use super::module::EmotionalMemory;

#[test]
fn test_new_module() {
    let em = EmotionalMemory::new();
    assert_eq!(em.evaluations_count, 0);
    assert_eq!(em.flashbulbs_detected, 0);
    assert!(!em.lexicon.is_empty());
}

#[test]
fn test_neutral_content() {
    let mut em = EmotionalMemory::new();
    let eval = em.evaluate_content("The function takes two parameters");
    assert!(eval.valence.abs() < 0.3);
    assert_eq!(eval.category, EmotionCategory::Neutral);
    assert!(!eval.is_flashbulb);
}

#[test]
fn test_positive_content() {
    let mut em = EmotionalMemory::new();
    let eval = em.evaluate_content("Amazing breakthrough! The fix is working perfectly");
    assert!(
        eval.valence > 0.3,
        "Expected positive valence, got {}",
        eval.valence
    );
    assert!(
        eval.arousal > 0.4,
        "Expected high arousal, got {}",
        eval.arousal
    );
}

#[test]
fn test_negative_content() {
    let mut em = EmotionalMemory::new();
    let eval = em.evaluate_content("Critical bug: production server crash with data corruption");
    assert!(
        eval.valence < -0.3,
        "Expected negative valence, got {}",
        eval.valence
    );
    assert!(
        eval.arousal > 0.5,
        "Expected high arousal, got {}",
        eval.arousal
    );
}

#[test]
fn test_flashbulb_detection_with_importance() {
    let mut em = EmotionalMemory::new();
    let eval = em.evaluate_with_importance(
        "Production server is down!",
        0.8, // High novelty
        0.9, // High arousal
    );
    assert!(
        eval.is_flashbulb,
        "Should detect flashbulb with high novelty + arousal"
    );
}

#[test]
fn test_no_flashbulb_for_normal_content() {
    let mut em = EmotionalMemory::new();
    let eval = em.evaluate_with_importance(
        "Updated the readme file",
        0.2, // Low novelty
        0.1, // Low arousal
    );
    assert!(!eval.is_flashbulb);
}

#[test]
fn test_negation_handling() {
    let mut em = EmotionalMemory::new();
    let positive = em.evaluate_content("This is amazing");
    let negated = em.evaluate_content("This is not amazing");
    assert!(
        negated.valence < positive.valence,
        "Negation should reduce valence"
    );
}

#[test]
fn test_stability_multiplier() {
    let em = EmotionalMemory::new();
    assert_eq!(em.stability_multiplier(0.0), 1.0);
    assert!(em.stability_multiplier(0.5) > 1.0);
    assert!(em.stability_multiplier(1.0) > em.stability_multiplier(0.5));
    // Max multiplier at arousal=1.0 should be 1.3
    assert!((em.stability_multiplier(1.0) - 1.3).abs() < 0.001);
}

#[test]
fn test_mood_congruence_boost() {
    let mut em = EmotionalMemory::new();
    // Set mood to positive
    for _ in 0..5 {
        em.evaluate_content("Great amazing perfect success");
    }
    let (mood_v, _) = em.current_mood();
    assert!(
        mood_v > 0.3,
        "Mood should be positive after positive content"
    );

    // Positive memory should get boost
    let boost = em.mood_congruence_boost(0.7);
    assert!(
        boost > 0.0,
        "Positive memory should get mood-congruent boost"
    );

    // Negative memory should get less/no boost
    let neg_boost = em.mood_congruence_boost(-0.7);
    assert!(
        neg_boost < boost,
        "Negative memory should get less boost in positive mood"
    );
}

#[test]
fn test_capture_targets() {
    let mut em = EmotionalMemory::new();

    // Record some memories
    em.record_encoding("mem-1", 0.3, 0.4);
    em.record_encoding("mem-2", -0.2, 0.3);

    // Low arousal trigger shouldn't capture anything
    let targets = em.get_capture_targets(0.3);
    assert!(targets.is_empty(), "Low arousal shouldn't trigger capture");

    // High arousal trigger should capture recent memories
    let targets = em.get_capture_targets(0.9);
    assert!(!targets.is_empty(), "High arousal should trigger capture");
    assert!(targets.iter().any(|(id, _)| id == "mem-1"));
    assert!(targets.iter().any(|(id, _)| id == "mem-2"));
}

#[test]
fn test_mood_tracking() {
    let mut em = EmotionalMemory::new();
    let (v0, _) = em.current_mood();
    assert!((v0 - 0.0).abs() < 0.001);

    // Evaluate several negative items
    for _ in 0..5 {
        em.evaluate_content("error failure crash bug panic");
    }
    let (v1, a1) = em.current_mood();
    assert!(v1 < 0.0, "Mood should be negative after negative content");
    assert!(
        a1 > 0.3,
        "Arousal should be elevated after negative content"
    );
}

#[test]
fn test_urgency_markers() {
    let mut em = EmotionalMemory::new();
    let eval = em.evaluate_content("CRITICAL: production down, need hotfix ASAP");
    assert!(eval.arousal > 0.5, "Urgency markers should boost arousal");
}

#[test]
fn test_stats() {
    let mut em = EmotionalMemory::new();
    em.evaluate_content("Test content");
    let stats = em.stats();
    assert_eq!(stats.evaluations_count, 1);
    assert!(stats.lexicon_size > 50);
}

#[test]
fn test_emotion_categories() {
    let mut em = EmotionalMemory::new();

    let joy = em.evaluate_content("Amazing success! Everything is working perfectly!");
    assert_eq!(joy.category, EmotionCategory::Joy);

    let frustration = em.evaluate_content("This stupid bug keeps crashing the server");
    assert_eq!(frustration.category, EmotionCategory::Frustration);
}

#[test]
fn test_empty_content() {
    let mut em = EmotionalMemory::new();
    let eval = em.evaluate_content("");
    assert_eq!(eval.valence, 0.0);
    assert_eq!(eval.category, EmotionCategory::Neutral);
    assert!(!eval.is_flashbulb);
}

#[test]
fn test_display_emotion_category() {
    assert_eq!(EmotionCategory::Joy.to_string(), "joy");
    assert_eq!(EmotionCategory::Urgency.to_string(), "urgency");
    assert_eq!(EmotionCategory::Neutral.to_string(), "neutral");
}
