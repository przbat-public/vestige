use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Mutex;

use vestige_core::Storage;

use crate::cognitive::CognitiveEngine;

use super::split_memories::{detect_compound_content_reason, execute_split_memories};
use super::system_status::{execute_system_status, system_status_schema};

fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
    Arc::new(Mutex::new(CognitiveEngine::new()))
}

async fn test_storage() -> (Arc<Storage>, TempDir) {
    let dir = TempDir::new().unwrap();
    let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
    (Arc::new(storage), dir)
}

#[test]
fn test_system_status_schema() {
    let schema = system_status_schema();
    assert_eq!(schema["type"], "object");
}

#[tokio::test]
async fn test_system_status_empty_db() {
    let (storage, _dir) = test_storage().await;
    let result = execute_system_status(&storage, &test_cognitive(), None).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["tool"], "system_status");
    assert_eq!(value["status"], "empty");
    assert_eq!(value["totalMemories"], 0);
    assert!(value["warnings"].is_array());
    assert!(value["recommendations"].is_array());
}

#[tokio::test]
async fn test_system_status_with_memories() {
    let (storage, _dir) = test_storage().await;
    {
        storage
            .ingest(vestige_core::IngestInput {
                content: "Test memory for status".to_string(),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec![],
                valid_from: None,
                valid_until: None,
                provenance: None,
                ..Default::default()
            })
            .unwrap();
    }
    let result = execute_system_status(&storage, &test_cognitive(), None).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["totalMemories"], 1);
    assert!(value["stateDistribution"].is_object());
    assert!(value["embeddingCoverage"].is_string());
}

#[tokio::test]
async fn test_system_status_has_cognitive_health() {
    let (storage, _dir) = test_storage().await;
    let result = execute_system_status(&storage, &test_cognitive(), None).await;
    let value = result.unwrap();
    assert!(value["cognitiveHealth"].is_object());
    assert_eq!(value["cognitiveHealth"]["modulesActive"], 28);
}

#[tokio::test]
async fn test_system_status_has_automation_triggers() {
    let (storage, _dir) = test_storage().await;
    let result = execute_system_status(&storage, &test_cognitive(), None).await;
    assert!(result.is_ok());
    let value = result.unwrap();

    let triggers = &value["automationTriggers"];
    assert!(triggers.is_object(), "automationTriggers should be present");
    assert!(triggers["lastDreamTimestamp"].is_null(), "No dreams yet");
    assert_eq!(triggers["savesSinceLastDream"], 0, "Empty DB = 0 saves");
    assert!(
        triggers["lastConsolidationTimestamp"].is_null(),
        "No consolidation yet"
    );
    // lastBackupTimestamp depends on filesystem state, just check it exists
    assert!(triggers.get("lastBackupTimestamp").is_some());
}

#[tokio::test]
async fn test_system_status_automation_triggers_with_memories() {
    let (storage, _dir) = test_storage().await;
    {
        for i in 0..3 {
            storage
                .ingest(vestige_core::IngestInput {
                    content: format!("Automation trigger test memory {}", i),
                    node_type: "fact".to_string(),
                    source: None,
                    sentiment_score: 0.0,
                    sentiment_magnitude: 0.0,
                    tags: vec![],
                    valid_from: None,
                    valid_until: None,
                    provenance: None,
                    ..Default::default()
                })
                .unwrap();
        }
    }
    let result = execute_system_status(&storage, &test_cognitive(), None).await;
    let value = result.unwrap();

    let triggers = &value["automationTriggers"];
    // No dream ever → savesSinceLastDream == totalMemories
    assert_eq!(triggers["savesSinceLastDream"], 3);
    assert!(triggers["lastDreamTimestamp"].is_null());
}

#[tokio::test]
async fn test_split_memories_empty_db() {
    let (storage, _dir) = test_storage().await;
    let result = execute_split_memories(&storage, None).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["total_scanned"], 0);
    assert_eq!(value["compound_found"], 0);
    assert_eq!(value["dry_run"], true);
}

#[tokio::test]
async fn test_split_memories_finds_compound() {
    let (storage, _dir) = test_storage().await;
    let compound = "Alice: We discussed the deployment plan for the new microservice and decided on Friday release.\n\
                         Bob: Let's do it Friday but make sure all the integration and unit tests pass first before deploy.\n\
                         Carol: I agree with that plan and I will prepare the rollback scripts for safety in case of failures.\n\
                         Dave: Make sure the staging environment passes all health checks and monitoring is configured properly.";
    storage
        .ingest(vestige_core::IngestInput {
            content: compound.to_string(),
            node_type: "event".to_string(),
            source: None,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: vec![],
            valid_from: None,
            valid_until: None,
            provenance: None,
            ..Default::default()
        })
        .unwrap();
    storage
        .ingest(vestige_core::IngestInput {
            content: "Single atomic fact about Rust.".to_string(),
            node_type: "fact".to_string(),
            source: None,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: vec![],
            valid_from: None,
            valid_until: None,
            provenance: None,
            ..Default::default()
        })
        .unwrap();

    let result = execute_split_memories(&storage, None).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["total_scanned"], 2);
    assert_eq!(value["compound_found"], 1);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["deleted"], 0);
    let memories = value["memories"].as_array().unwrap();
    assert_eq!(memories.len(), 1);
    assert!(
        memories[0]["reason"]
            .as_str()
            .unwrap()
            .contains("conversation transcript")
    );
}

#[test]
fn test_detect_compound_reason_short_returns_none() {
    assert!(detect_compound_content_reason("Short memory").is_none());
}

#[test]
fn test_detect_compound_reason_multi_bullet() {
    let content = "Session summary with many items from today's productive work session:\n\
                        - Fixed the authentication bug in login flow where the tokens expired too quickly\n\
                        - Decided to migrate from MySQL to PostgreSQL for better performance and JSON support\n\
                        - John prefers dark mode in all editors and wants dashboard themes to be configurable\n\
                        - Deployment deadline moved to next Friday because of the infrastructure migration delay\n\
                        - Added rate limiting to the API gateway to prevent abuse from unauthenticated external clients";
    let result = detect_compound_content_reason(content);
    assert!(result.is_some());
    assert!(result.unwrap().contains("multi-item list"));
}
