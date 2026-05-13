use std::sync::Arc;

use chrono::{Duration, Utc};
use tempfile::TempDir;
use tokio::sync::Mutex;
use uuid::Uuid;

use vestige_core::Storage;

use crate::cognitive::CognitiveEngine;

use super::execute::execute;
use super::schema::schema;

fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
    Arc::new(Mutex::new(CognitiveEngine::new()))
}

async fn test_storage() -> (Arc<Storage>, TempDir) {
    let dir = TempDir::new().unwrap();
    let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
    (Arc::new(storage), dir)
}


    /// Helper to create an intention and return its ID
    async fn create_test_intention(storage: &Arc<Storage>, description: &str) -> String {
        let args = serde_json::json!({
            "action": "set",
            "description": description
        });
        let result = execute(storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        result["intentionId"].as_str().unwrap().to_string()
    }

    // ========================================================================
    // ACTION ROUTING TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_missing_action_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({});
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid arguments"));
    }

    #[tokio::test]
    async fn test_unknown_action_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "unknown" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown action"));
    }

    #[tokio::test]
    async fn test_missing_arguments_fails() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing arguments"));
    }

    // ========================================================================
    // SET ACTION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_set_action_basic_succeeds() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "set",
            "description": "Remember to write unit tests"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "set");
        assert!(value["intentionId"].is_string());
        assert!(
            value["message"]
                .as_str()
                .unwrap()
                .contains("Intention created")
        );
    }

    #[tokio::test]
    async fn test_set_action_missing_description_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "set" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing 'description'"));
    }

    #[tokio::test]
    async fn test_set_action_empty_description_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "set",
            "description": ""
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_set_action_with_priority() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "set",
            "description": "Critical bug fix needed",
            "priority": "critical"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["priority"], 4);
    }

    #[tokio::test]
    async fn test_set_action_with_time_trigger() {
        let (storage, _dir) = test_storage().await;
        let future_time = (Utc::now() + Duration::hours(1)).to_rfc3339();
        let args = serde_json::json!({
            "action": "set",
            "description": "Meeting reminder",
            "trigger": {
                "type": "time",
                "at": future_time
            }
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value["triggerAt"].is_string());
    }

    #[tokio::test]
    async fn test_set_action_with_duration_trigger() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "set",
            "description": "Check build status",
            "trigger": {
                "type": "time",
                "inMinutes": 30
            }
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value["triggerAt"].is_string());
    }

    #[tokio::test]
    async fn test_set_action_with_deadline() {
        let (storage, _dir) = test_storage().await;
        let deadline = (Utc::now() + Duration::days(7)).to_rfc3339();
        let args = serde_json::json!({
            "action": "set",
            "description": "Complete feature by end of week",
            "deadline": deadline
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value["deadline"].is_string());
    }

    // ========================================================================
    // CHECK ACTION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_check_action_empty_succeeds() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "check" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["action"], "check");
        assert!(value["triggered"].is_array());
        assert!(value["pending"].is_array());
        assert!(value["checkedAt"].is_string());
    }

    #[tokio::test]
    async fn test_check_action_returns_pending() {
        let (storage, _dir) = test_storage().await;
        create_test_intention(&storage, "Future task").await;

        let args = serde_json::json!({ "action": "check" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let pending = value["pending"].as_array().unwrap();
        assert!(!pending.is_empty());
    }

    #[tokio::test]
    async fn test_check_action_with_context() {
        let (storage, _dir) = test_storage().await;

        // Create context-triggered intention
        let set_args = serde_json::json!({
            "action": "set",
            "description": "Check tests in payments",
            "trigger": {
                "type": "context",
                "codebase": "payments"
            }
        });
        execute(&storage, &test_cognitive(), Some(set_args))
            .await
            .unwrap();

        // Check with matching context
        let check_args = serde_json::json!({
            "action": "check",
            "context": {
                "codebase": "payments-service"
            }
        });
        let result = execute(&storage, &test_cognitive(), Some(check_args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let triggered = value["triggered"].as_array().unwrap();
        assert!(!triggered.is_empty());
    }

    #[tokio::test]
    async fn test_check_action_time_triggered() {
        let (storage, _dir) = test_storage().await;

        // Create time-triggered intention in the past
        let past_time = (Utc::now() - Duration::hours(1)).to_rfc3339();
        let set_args = serde_json::json!({
            "action": "set",
            "description": "Past due task",
            "trigger": {
                "type": "time",
                "at": past_time
            }
        });
        execute(&storage, &test_cognitive(), Some(set_args))
            .await
            .unwrap();

        let check_args = serde_json::json!({ "action": "check" });
        let result = execute(&storage, &test_cognitive(), Some(check_args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let triggered = value["triggered"].as_array().unwrap();
        assert!(!triggered.is_empty());
    }

    // ========================================================================
    // UPDATE ACTION TESTS - COMPLETE
    // ========================================================================

    #[tokio::test]
    async fn test_update_action_complete_succeeds() {
        let (storage, _dir) = test_storage().await;
        let intention_id = create_test_intention(&storage, "Task to complete").await;

        let args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "complete"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["action"], "update");
        assert_eq!(value["status"], "complete");
        assert!(value["message"].as_str().unwrap().contains("complete"));
    }

    #[tokio::test]
    async fn test_update_action_complete_nonexistent_fails() {
        let (storage, _dir) = test_storage().await;
        let fake_id = Uuid::new_v4().to_string();

        let args = serde_json::json!({
            "action": "update",
            "id": fake_id,
            "status": "complete"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_update_action_missing_id_fails() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({
            "action": "update",
            "status": "complete"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing 'id'"));
    }

    #[tokio::test]
    async fn test_update_action_missing_status_fails() {
        let (storage, _dir) = test_storage().await;
        let intention_id = create_test_intention(&storage, "Task").await;

        let args = serde_json::json!({
            "action": "update",
            "id": intention_id
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing 'status'"));
    }

    // ========================================================================
    // UPDATE ACTION TESTS - SNOOZE
    // ========================================================================

    #[tokio::test]
    async fn test_update_action_snooze_succeeds() {
        let (storage, _dir) = test_storage().await;
        let intention_id = create_test_intention(&storage, "Task to snooze").await;

        let args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "snooze",
            "snooze_minutes": 30
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["status"], "snooze");
        assert!(value["snoozedUntil"].is_string());
        assert!(value["message"].as_str().unwrap().contains("snoozed"));
    }

    #[tokio::test]
    async fn test_update_action_snooze_default_minutes() {
        let (storage, _dir) = test_storage().await;
        let intention_id = create_test_intention(&storage, "Task with default snooze").await;

        let args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "snooze"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value["message"].as_str().unwrap().contains("30 minutes"));
    }

    // ========================================================================
    // UPDATE ACTION TESTS - CANCEL
    // ========================================================================

    #[tokio::test]
    async fn test_update_action_cancel_succeeds() {
        let (storage, _dir) = test_storage().await;
        let intention_id = create_test_intention(&storage, "Task to cancel").await;

        let args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "cancel"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(value["status"], "cancel");
        assert!(value["message"].as_str().unwrap().contains("cancelled"));
    }

    #[tokio::test]
    async fn test_update_action_unknown_status_fails() {
        let (storage, _dir) = test_storage().await;
        let intention_id = create_test_intention(&storage, "Task").await;

        let args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "invalid"
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown status"));
    }

    // ========================================================================
    // LIST ACTION TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_list_action_empty_succeeds() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "action": "list" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["action"], "list");
        assert!(value["intentions"].is_array());
        assert_eq!(value["total"], 0);
        assert_eq!(value["status"], "active");
    }

    #[tokio::test]
    async fn test_list_action_returns_created() {
        let (storage, _dir) = test_storage().await;
        create_test_intention(&storage, "First task").await;
        create_test_intention(&storage, "Second task").await;

        let args = serde_json::json!({ "action": "list" });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert_eq!(value["total"], 2);
    }

    #[tokio::test]
    async fn test_list_action_filter_by_status() {
        let (storage, _dir) = test_storage().await;
        let intention_id = create_test_intention(&storage, "Task to complete").await;

        // Complete one
        let complete_args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "complete"
        });
        execute(&storage, &test_cognitive(), Some(complete_args))
            .await
            .unwrap();

        // Create another active one
        create_test_intention(&storage, "Active task").await;

        // List fulfilled
        let list_args = serde_json::json!({
            "action": "list",
            "filter_status": "fulfilled"
        });
        let result = execute(&storage, &test_cognitive(), Some(list_args))
            .await
            .unwrap();
        assert_eq!(result["total"], 1);
        assert_eq!(result["status"], "fulfilled");
    }

    #[tokio::test]
    async fn test_list_action_with_limit() {
        let (storage, _dir) = test_storage().await;
        for i in 0..5 {
            create_test_intention(&storage, &format!("Task {}", i)).await;
        }

        let args = serde_json::json!({
            "action": "list",
            "limit": 3
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let intentions = value["intentions"].as_array().unwrap();
        assert!(intentions.len() <= 3);
    }

    #[tokio::test]
    async fn test_list_action_all_status() {
        let (storage, _dir) = test_storage().await;
        let intention_id = create_test_intention(&storage, "Task to complete").await;
        create_test_intention(&storage, "Active task").await;

        // Complete one
        let complete_args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "complete"
        });
        execute(&storage, &test_cognitive(), Some(complete_args))
            .await
            .unwrap();

        // List all
        let list_args = serde_json::json!({
            "action": "list",
            "filter_status": "all"
        });
        let result = execute(&storage, &test_cognitive(), Some(list_args))
            .await
            .unwrap();
        assert_eq!(result["total"], 2);
    }

    // ========================================================================
    // FULL LIFECYCLE TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_intention_full_lifecycle() {
        let (storage, _dir) = test_storage().await;

        // 1. Create intention
        let intention_id = create_test_intention(&storage, "Full lifecycle test").await;

        // 2. Verify it appears in list
        let list_args = serde_json::json!({ "action": "list" });
        let list_result = execute(&storage, &test_cognitive(), Some(list_args))
            .await
            .unwrap();
        assert_eq!(list_result["total"], 1);

        // 3. Snooze it
        let snooze_args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "snooze",
            "snooze_minutes": 5
        });
        let snooze_result = execute(&storage, &test_cognitive(), Some(snooze_args)).await;
        assert!(snooze_result.is_ok());

        // 4. Complete it
        let complete_args = serde_json::json!({
            "action": "update",
            "id": intention_id,
            "status": "complete"
        });
        let complete_result = execute(&storage, &test_cognitive(), Some(complete_args)).await;
        assert!(complete_result.is_ok());

        // 5. Verify it's no longer active
        let final_list_args = serde_json::json!({ "action": "list" });
        let final_list = execute(&storage, &test_cognitive(), Some(final_list_args))
            .await
            .unwrap();
        assert_eq!(final_list["total"], 0);

        // 6. Verify it's in fulfilled list
        let fulfilled_args = serde_json::json!({
            "action": "list",
            "filter_status": "fulfilled"
        });
        let fulfilled_list = execute(&storage, &test_cognitive(), Some(fulfilled_args))
            .await
            .unwrap();
        assert_eq!(fulfilled_list["total"], 1);
    }

    #[tokio::test]
    async fn test_intention_priority_ordering() {
        let (storage, _dir) = test_storage().await;

        // Create intentions with different priorities
        let args_low = serde_json::json!({
            "action": "set",
            "description": "Low priority task",
            "priority": "low"
        });
        execute(&storage, &test_cognitive(), Some(args_low))
            .await
            .unwrap();

        let args_critical = serde_json::json!({
            "action": "set",
            "description": "Critical task",
            "priority": "critical"
        });
        execute(&storage, &test_cognitive(), Some(args_critical))
            .await
            .unwrap();

        let args_normal = serde_json::json!({
            "action": "set",
            "description": "Normal task",
            "priority": "normal"
        });
        execute(&storage, &test_cognitive(), Some(args_normal))
            .await
            .unwrap();

        // List and verify ordering (critical should be first due to priority DESC ordering)
        let list_args = serde_json::json!({ "action": "list" });
        let list_result = execute(&storage, &test_cognitive(), Some(list_args))
            .await
            .unwrap();
        let intentions = list_result["intentions"].as_array().unwrap();

        assert!(intentions.len() >= 3);
        // Critical (4) should come before normal (2) and low (1)
        let first_priority = intentions[0]["priority"].as_str().unwrap();
        assert_eq!(first_priority, "critical");
    }

    // ========================================================================
    // SCHEMA TESTS
    // ========================================================================

    #[test]
    fn test_schema_has_required_action() {
        let schema_value = schema();
        assert_eq!(schema_value["type"], "object");
        assert!(schema_value["properties"]["action"].is_object());
        assert!(
            schema_value["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("action"))
        );
    }

    #[test]
    fn test_schema_has_action_enum() {
        let schema_value = schema();
        let action_enum = schema_value["properties"]["action"]["enum"]
            .as_array()
            .unwrap();
        assert!(action_enum.contains(&serde_json::json!("set")));
        assert!(action_enum.contains(&serde_json::json!("check")));
        assert!(action_enum.contains(&serde_json::json!("update")));
        assert!(action_enum.contains(&serde_json::json!("list")));
    }

    #[test]
    fn test_schema_has_set_parameters() {
        let schema_value = schema();
        assert!(schema_value["properties"]["description"].is_object());
        assert!(schema_value["properties"]["trigger"].is_object());
        assert!(schema_value["properties"]["priority"].is_object());
        assert!(schema_value["properties"]["deadline"].is_object());
    }

    #[test]
    fn test_schema_has_update_parameters() {
        let schema_value = schema();
        assert!(schema_value["properties"]["id"].is_object());
        assert!(schema_value["properties"]["status"].is_object());
        assert!(schema_value["properties"]["snooze_minutes"].is_object());
    }

    #[test]
    fn test_schema_has_check_parameters() {
        let schema_value = schema();
        assert!(schema_value["properties"]["context"].is_object());
        assert!(schema_value["properties"]["include_snoozed"].is_object());
    }

    #[test]
    fn test_schema_has_list_parameters() {
        let schema_value = schema();
        assert!(schema_value["properties"]["filter_status"].is_object());
        assert!(schema_value["properties"]["limit"].is_object());
    }
