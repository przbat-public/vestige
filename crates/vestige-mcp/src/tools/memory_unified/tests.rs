use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Mutex;

use vestige_core::{MemoryState, Storage};

use crate::cognitive::CognitiveEngine;

use super::execute::execute;
use super::helpers::{
    ACCESSIBILITY_ACTIVE, ACCESSIBILITY_DORMANT, ACCESSIBILITY_SILENT, compute_accessibility,
    state_from_accessibility,
};
use super::schema::schema;

fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
    Arc::new(Mutex::new(CognitiveEngine::new()))
}

async fn test_storage() -> (Arc<Storage>, TempDir) {
    let dir = TempDir::new().unwrap();
    let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
    (Arc::new(storage), dir)
}

#[test]
fn test_accessibility_thresholds() {
    // Test Active state
    let accessibility = compute_accessibility(0.9, 0.8, 0.7);
    assert!(accessibility >= ACCESSIBILITY_ACTIVE);
    assert!(matches!(
        state_from_accessibility(accessibility),
        MemoryState::Active
    ));

    // Test Dormant state
    let accessibility = compute_accessibility(0.5, 0.5, 0.5);
    assert!((ACCESSIBILITY_DORMANT..ACCESSIBILITY_ACTIVE).contains(&accessibility));
    assert!(matches!(
        state_from_accessibility(accessibility),
        MemoryState::Dormant
    ));

    // Test Silent state
    let accessibility = compute_accessibility(0.2, 0.2, 0.2);
    assert!((ACCESSIBILITY_SILENT..ACCESSIBILITY_DORMANT).contains(&accessibility));
    assert!(matches!(
        state_from_accessibility(accessibility),
        MemoryState::Silent
    ));

    // Test Unavailable state
    let accessibility = compute_accessibility(0.05, 0.05, 0.05);
    assert!(accessibility < ACCESSIBILITY_SILENT);
    assert!(matches!(
        state_from_accessibility(accessibility),
        MemoryState::Unavailable
    ));
}

#[test]
fn test_schema_structure() {
    let schema = schema();
    assert!(schema["properties"]["action"].is_object());
    assert!(schema["properties"]["id"].is_object());
    assert!(schema["properties"]["reason"].is_object());
    assert_eq!(schema["required"], serde_json::json!(["action"]));
    // Verify all 7 actions are in enum
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert_eq!(actions.len(), 7);
    assert!(actions.contains(&serde_json::json!("get_batch")));
    assert!(actions.contains(&serde_json::json!("edit")));
    assert!(actions.contains(&serde_json::json!("promote")));
    assert!(actions.contains(&serde_json::json!("demote")));
}

// === INTEGRATION TESTS ===

async fn ingest_memory(storage: &Arc<Storage>) -> String {
    let node = storage
        .ingest(vestige_core::IngestInput {
            content: "Memory unified test content".to_string(),
            node_type: "fact".to_string(),
            source: Some("test".to_string()),
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: vec!["test-tag".to_string()],
            valid_from: None,
            valid_until: None,
            provenance: None,
            ..Default::default()
        })
        .unwrap();
    node.id
}

#[tokio::test]
async fn test_missing_args_fails() {
    let (storage, _dir) = test_storage().await;
    let result = execute(&storage, &test_cognitive(), None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Missing arguments"));
}

#[tokio::test]
async fn test_invalid_action_fails() {
    let (storage, _dir) = test_storage().await;
    let args =
        serde_json::json!({ "action": "invalid", "id": "00000000-0000-0000-0000-000000000000" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid action"));
}

#[tokio::test]
async fn test_invalid_uuid_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "action": "get", "id": "not-a-uuid" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid memory ID format"));
}

#[tokio::test]
async fn test_get_existing_memory() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "get", "id": id });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["action"], "get");
    assert_eq!(value["found"], true);
    assert_eq!(value["node"]["id"], id);
    assert_eq!(value["node"]["content"], "Memory unified test content");
    assert_eq!(value["node"]["nodeType"], "fact");
    assert!(value["node"]["createdAt"].is_string());
    assert!(value["node"]["tags"].is_array());
}

#[tokio::test]
async fn test_get_nonexistent_memory() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "action": "get", "id": "00000000-0000-0000-0000-000000000000" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["found"], false);
    assert_eq!(value["message"], "Memory not found");
}

#[tokio::test]
async fn test_delete_existing_memory() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "delete", "id": id });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["action"], "delete");
    assert_eq!(value["success"], true);
}

#[tokio::test]
async fn test_delete_nonexistent_memory() {
    let (storage, _dir) = test_storage().await;
    // Ingest+delete a throwaway memory to warm writer after WAL migration
    let warmup_id = storage
        .ingest(vestige_core::IngestInput {
            content: "warmup".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap()
        .id;
    let _ = storage.delete_node(&warmup_id);
    let args =
        serde_json::json!({ "action": "delete", "id": "00000000-0000-0000-0000-000000000000" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], false);
    assert!(value["message"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_delete_then_get_returns_not_found() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let del_args = serde_json::json!({ "action": "delete", "id": id });
    execute(&storage, &test_cognitive(), Some(del_args))
        .await
        .unwrap();
    let get_args = serde_json::json!({ "action": "get", "id": id });
    let result = execute(&storage, &test_cognitive(), Some(get_args)).await;
    let value = result.unwrap();
    assert_eq!(value["found"], false);
}

#[tokio::test]
async fn test_state_existing_memory() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "state", "id": id });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["action"], "state");
    assert_eq!(value["memoryId"], id);
    assert!(value["accessibility"].is_number());
    assert!(value["state"].is_string());
    assert!(value["description"].is_string());
    assert!(value["components"]["retentionStrength"].is_number());
    assert!(value["components"]["retrievalStrength"].is_number());
    assert!(value["components"]["storageStrength"].is_number());
    assert_eq!(value["thresholds"]["active"], 0.7);
    assert_eq!(value["thresholds"]["dormant"], 0.4);
    assert_eq!(value["thresholds"]["silent"], 0.1);
}

#[tokio::test]
async fn test_state_nonexistent_memory_fails() {
    let (storage, _dir) = test_storage().await;
    let args =
        serde_json::json!({ "action": "state", "id": "00000000-0000-0000-0000-000000000000" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_accessibility_boundary_active() {
    let a = compute_accessibility(1.0, 0.7, 0.5);
    assert!(a >= ACCESSIBILITY_ACTIVE);
    assert!(matches!(state_from_accessibility(a), MemoryState::Active));
}

#[test]
fn test_accessibility_boundary_zero() {
    let a = compute_accessibility(0.0, 0.0, 0.0);
    assert_eq!(a, 0.0);
    assert!(matches!(
        state_from_accessibility(a),
        MemoryState::Unavailable
    ));
}

// ========================================================================
// PROMOTE/DEMOTE TESTS (ported from feedback.rs, v1.7.0 merge)
// ========================================================================

#[tokio::test]
async fn test_promote_missing_id_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "action": "promote", "id": "not-a-uuid" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid memory ID format"));
}

#[tokio::test]
async fn test_promote_nonexistent_node_fails() {
    let (storage, _dir) = test_storage().await;
    let args =
        serde_json::json!({ "action": "promote", "id": "00000000-0000-0000-0000-000000000000" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Node not found"));
}

#[tokio::test]
async fn test_promote_succeeds() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "promote", "id": id, "reason": "It was helpful" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], true);
    assert_eq!(value["action"], "promoted");
    assert_eq!(value["nodeId"], id);
    assert_eq!(value["reason"], "It was helpful");
    assert!(value["changes"]["retrievalStrength"].is_object());
}

#[tokio::test]
async fn test_promote_without_reason_succeeds() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "promote", "id": id });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], true);
    assert!(value["reason"].is_null());
}

#[tokio::test]
async fn test_promote_changes_contain_expected_fields() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "promote", "id": id });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    let value = result.unwrap();
    assert!(value["changes"]["retrievalStrength"]["before"].is_number());
    assert!(value["changes"]["retrievalStrength"]["after"].is_number());
    assert_eq!(value["changes"]["retrievalStrength"]["delta"], "+0.20");
    assert!(value["changes"]["retentionStrength"]["before"].is_number());
    assert_eq!(value["changes"]["retentionStrength"]["delta"], "+0.10");
    assert_eq!(value["changes"]["stability"]["multiplier"], "1.5x");
}

#[tokio::test]
async fn test_demote_invalid_uuid_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "action": "demote", "id": "bad-id" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid memory ID format"));
}

#[tokio::test]
async fn test_demote_nonexistent_node_fails() {
    let (storage, _dir) = test_storage().await;
    let args =
        serde_json::json!({ "action": "demote", "id": "00000000-0000-0000-0000-000000000000" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Node not found"));
}

#[tokio::test]
async fn test_demote_succeeds() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "demote", "id": id, "reason": "It was wrong" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], true);
    assert_eq!(value["action"], "demoted");
    assert_eq!(value["nodeId"], id);
    assert_eq!(value["reason"], "It was wrong");
    assert!(value["note"].as_str().unwrap().contains("NOT deleted"));
}

#[tokio::test]
async fn test_demote_changes_contain_expected_fields() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "demote", "id": id });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    let value = result.unwrap();
    assert!(value["changes"]["retrievalStrength"]["before"].is_number());
    assert_eq!(value["changes"]["retrievalStrength"]["delta"], "-0.30");
    assert_eq!(value["changes"]["retentionStrength"]["delta"], "-0.15");
    assert_eq!(value["changes"]["stability"]["multiplier"], "0.5x");
}

// ========================================================================
// EDIT TESTS (v1.9.2)
// ========================================================================

#[tokio::test]
async fn test_edit_succeeds() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({
        "action": "edit",
        "id": id,
        "content": "Updated memory content"
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], true);
    assert_eq!(value["action"], "edit");
    assert_eq!(value["nodeId"], id);
    assert!(
        value["oldContentPreview"]
            .as_str()
            .unwrap()
            .contains("Memory unified test content")
    );
    assert!(
        value["newContentPreview"]
            .as_str()
            .unwrap()
            .contains("Updated memory content")
    );
    assert!(
        value["note"]
            .as_str()
            .unwrap()
            .contains("FSRS state preserved")
    );
}

#[tokio::test]
async fn test_edit_preserves_fsrs_state() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;

    // Get FSRS state before edit
    let before = storage.get_node(&id).unwrap().unwrap();

    // Edit content
    let args = serde_json::json!({
        "action": "edit",
        "id": id,
        "content": "Completely new content after edit"
    });
    execute(&storage, &test_cognitive(), Some(args))
        .await
        .unwrap();

    // Verify FSRS state preserved
    let after = storage.get_node(&id).unwrap().unwrap();
    assert_eq!(after.stability, before.stability);
    assert_eq!(after.difficulty, before.difficulty);
    assert_eq!(after.reps, before.reps);
    assert_eq!(after.lapses, before.lapses);
    assert_eq!(after.retention_strength, before.retention_strength);
    // Content should be updated
    assert_eq!(after.content, "Completely new content after edit");
    assert_ne!(after.content, before.content);
}

#[tokio::test]
async fn test_edit_missing_content_fails() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "edit", "id": id });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("content"));
}

#[tokio::test]
async fn test_edit_empty_content_fails() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    let args = serde_json::json!({ "action": "edit", "id": id, "content": "  " });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("empty"));
}

#[tokio::test]
async fn test_edit_nonexistent_memory_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({
        "action": "edit",
        "id": "00000000-0000-0000-0000-000000000000",
        "content": "New content"
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[tokio::test]
async fn test_edit_with_multibyte_utf8_content() {
    let (storage, _dir) = test_storage().await;
    let id = ingest_memory(&storage).await;
    // Content with emoji and CJK characters (multi-byte UTF-8)
    let long_content = "🧠".repeat(100); // 100 brain emoji = 400 bytes but only 100 chars
    let args = serde_json::json!({
        "action": "edit",
        "id": id,
        "content": long_content
    });
    // This must NOT panic (previous code would panic on byte-level truncation)
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], true);
}
