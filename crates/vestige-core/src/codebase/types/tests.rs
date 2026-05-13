//! Tests for `codebase::types`.

use std::path::PathBuf;

use super::*;

#[test]
fn test_architectural_decision_builder() {
    let decision = ArchitecturalDecision::new(
        "adr-001".to_string(),
        "Use Event Sourcing".to_string(),
        "Need complete audit trail".to_string(),
    )
    .with_files(vec![PathBuf::from("src/events.rs")])
    .with_tags(vec!["architecture".to_string()]);

    assert_eq!(decision.id, "adr-001");
    assert!(!decision.files_affected.is_empty());
    assert!(!decision.tags.is_empty());
}

#[test]
fn test_codebase_node_id() {
    let decision = ArchitecturalDecision::new(
        "test-id".to_string(),
        "Test".to_string(),
        "Test".to_string(),
    );
    let node = CodebaseNode::ArchitecturalDecision(decision);
    assert_eq!(node.id(), "test-id");
    assert_eq!(node.node_type(), "architectural_decision");
}

#[test]
fn test_file_relationship_from_git() {
    let rel = FileRelationship::from_git_cochange(
        "rel-001".to_string(),
        vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")],
        0.8,
        15,
    );

    assert_eq!(rel.relationship_type, RelationType::FrequentCochange);
    assert_eq!(rel.source, RelationshipSource::GitCochange);
    assert_eq!(rel.strength, 0.8);
    assert_eq!(rel.observation_count, 15);
}

#[test]
fn test_searchable_text() {
    let pattern = CodePattern::new(
        "pat-001".to_string(),
        "Repository Pattern".to_string(),
        "Abstract data access".to_string(),
        "When you need to decouple domain logic from data access".to_string(),
    );
    let node = CodebaseNode::CodePattern(pattern);
    let text = node.to_searchable_text();

    assert!(text.contains("Repository Pattern"));
    assert!(text.contains("Abstract data access"));
}
