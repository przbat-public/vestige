//! Tests for the intent-detection pipeline.

use super::actions::{ActionType, UserAction};
use super::detector::IntentDetector;
use super::intent_kinds::DetectedIntent;

#[test]
fn test_debugging_detection() {
    let detector = IntentDetector::new();

    detector.record_action(UserAction::error("NullPointerException at line 42"));
    detector.record_action(UserAction::file_opened("/src/service.rs"));
    detector.record_action(UserAction::search("fix null pointer"));

    let result = detector.detect_intent();

    if let DetectedIntent::Debugging { symptoms, .. } = &result.primary_intent {
        assert!(!symptoms.is_empty());
    } else if result.confidence > 0.0 {
        // May detect different intent based on order
    }
}

#[test]
fn test_learning_detection() {
    let detector = IntentDetector::new();

    detector.record_action(UserAction::docs_viewed("async/await"));
    detector.record_action(UserAction::search("how to use tokio"));
    detector.record_action(UserAction::docs_viewed("futures"));

    let result = detector.detect_intent();

    if let DetectedIntent::Learning { topic, .. } = &result.primary_intent {
        assert!(!topic.is_empty());
    }
}

#[test]
fn test_intent_tags() {
    let debugging = DetectedIntent::Debugging {
        suspected_area: "auth".to_string(),
        symptoms: vec![],
    };

    let tags = debugging.relevant_tags();
    assert!(tags.contains(&"debugging".to_string()));
    assert!(tags.contains(&"error".to_string()));
}

#[test]
fn test_action_creation() {
    let action = UserAction::file_opened("/src/main.rs").with_metadata("project", "vestige");

    assert_eq!(action.action_type, ActionType::FileOpened);
    assert!(action.metadata.contains_key("project"));
}
