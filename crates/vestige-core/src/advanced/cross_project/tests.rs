use std::path::Path;

use chrono::Utc;

use super::{CodePattern, CrossProjectLearner, PatternCategory, ProjectContext, UniversalPattern};

#[test]
fn test_project_context() {
    let context = ProjectContext::from_path(Path::new("/my/project"))
        .with_language("rust")
        .with_framework("tokio");

    assert_eq!(context.name, Some("project".to_string()));
    assert!(context.languages.contains(&"rust".to_string()));
    assert!(context.frameworks.contains(&"tokio".to_string()));
}

#[test]
fn test_record_pattern_outcome() {
    let learner = CrossProjectLearner::new();

    learner.add_pattern(UniversalPattern {
        id: "test-pattern".to_string(),
        pattern: CodePattern {
            name: "Test".to_string(),
            category: PatternCategory::Testing,
            description: "Test pattern".to_string(),
            example: None,
            triggers: vec![],
            benefits: vec![],
            considerations: vec![],
        },
        projects_seen_in: vec!["proj1".to_string(), "proj2".to_string()],
        success_rate: 0.5,
        applicability: "Testing".to_string(),
        confidence: 0.5,
        first_seen: Utc::now(),
        last_seen: Utc::now(),
        application_count: 0,
    });

    learner.record_pattern_outcome("test-pattern", "proj3", true);
    learner.record_pattern_outcome("test-pattern", "proj4", true);
    learner.record_pattern_outcome("test-pattern", "proj5", false);

    let patterns = learner.get_all_patterns();
    let pattern = patterns.iter().find(|p| p.id == "test-pattern").unwrap();
    assert!((pattern.success_rate - 0.666).abs() < 0.01);
}

#[test]
fn test_find_universal_patterns() {
    let learner = CrossProjectLearner::new();

    // Pattern in only one project (not universal).
    learner.add_pattern(UniversalPattern {
        id: "local".to_string(),
        pattern: CodePattern {
            name: "Local".to_string(),
            category: PatternCategory::Testing,
            description: "Local only".to_string(),
            example: None,
            triggers: vec![],
            benefits: vec![],
            considerations: vec![],
        },
        projects_seen_in: vec!["proj1".to_string()],
        success_rate: 0.8,
        applicability: "".to_string(),
        confidence: 0.5,
        first_seen: Utc::now(),
        last_seen: Utc::now(),
        application_count: 0,
    });

    // Pattern in multiple projects (universal).
    learner.add_pattern(UniversalPattern {
        id: "universal".to_string(),
        pattern: CodePattern {
            name: "Universal".to_string(),
            category: PatternCategory::ErrorHandling,
            description: "Universal pattern".to_string(),
            example: None,
            triggers: vec![],
            benefits: vec![],
            considerations: vec![],
        },
        projects_seen_in: vec![
            "proj1".to_string(),
            "proj2".to_string(),
            "proj3".to_string(),
        ],
        success_rate: 0.9,
        applicability: "".to_string(),
        confidence: 0.7,
        first_seen: Utc::now(),
        last_seen: Utc::now(),
        application_count: 5,
    });

    let universal = learner.find_universal_patterns();
    assert_eq!(universal.len(), 1);
    assert_eq!(universal[0].id, "universal");
}
