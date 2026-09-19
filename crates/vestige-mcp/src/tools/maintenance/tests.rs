use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Mutex;

use vestige_core::Storage;

use crate::cognitive::CognitiveEngine;

use super::gc::{execute_gc, gc_schema};
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
async fn test_system_status_reports_reranker_readiness() {
    // The reranker is loaded lazily in main.rs after startup. Dashboards
    // need a way to tell "search results are using the cross-encoder"
    // vs "still warming up". A default CognitiveEngine has no model
    // loaded, so this must report `false`.
    let (storage, _dir) = test_storage().await;
    let result = execute_system_status(&storage, &test_cognitive(), None).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert!(
        value["rerankerReady"].is_boolean(),
        "rerankerReady must be a boolean flag (operators rely on it for monitoring)"
    );
    assert_eq!(
        value["rerankerReady"], false,
        "default CognitiveEngine has no cross-encoder loaded"
    );
}

#[tokio::test]
async fn test_system_status_reranker_status_string() {
    // A boolean is fine for machines, but the dashboard wants a human
    // readable status too. Three states: ready | warming | disabled.
    // Disabled = embeddings feature off; warming = feature on but model
    // not loaded yet (the common case during the first minute of uptime).
    let (storage, _dir) = test_storage().await;
    let result = execute_system_status(&storage, &test_cognitive(), None).await;
    let value = result.unwrap();
    let status = value["rerankerStatus"]
        .as_str()
        .expect("rerankerStatus must be a string label");
    assert!(
        matches!(status, "ready" | "warming" | "disabled"),
        "rerankerStatus must be ready|warming|disabled, got {status}"
    );
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

// ------------------------------------------------------------------
// Destructive-operation confirmation gate (#5 MCP elicitation prep).
// `gc` is the highest-blast-radius maintenance op: when an over-eager
// agent calls it with `dry_run: false` and we silently delete every
// memory below the retention threshold, there is no recovery short of
// `restore`. These tests pin the gate.
// ------------------------------------------------------------------

#[test]
fn test_gc_schema_exposes_confirmed_flag() {
    let schema = gc_schema();
    let props = schema["properties"]
        .as_object()
        .expect("gc schema must have properties");
    assert!(
        props.contains_key("confirmed"),
        "gc schema must advertise the `confirmed` confirmation flag so clients can prompt for it before retry"
    );
    assert_eq!(
        props["confirmed"]["type"], "boolean",
        "`confirmed` must be a boolean"
    );
    assert_eq!(
        props["confirmed"]["default"], false,
        "`confirmed` must default to false; otherwise the gate is no-op"
    );
}

#[tokio::test]
async fn test_gc_dry_run_does_not_require_confirmation() {
    let (storage, _dir) = test_storage().await;
    // dry_run=true is documented as safe and used by automation triggers.
    // Forcing confirmation here would break every existing client.
    let args = serde_json::json!({ "dry_run": true });
    let result = execute_gc(&storage, Some(args)).await;
    assert!(
        result.is_ok(),
        "dry_run=true must NEVER require confirmation; got: {:?}",
        result.err()
    );
    let value = result.unwrap();
    assert_eq!(value["dryRun"], true);
}

#[tokio::test]
async fn test_gc_destructive_call_without_confirmation_is_blocked() {
    let (storage, _dir) = test_storage().await;
    // Seed a low-retention memory so a non-confirmed delete would have
    // observable effect. The gate must fire BEFORE the delete is queued.
    storage
        .ingest(vestige_core::IngestInput {
            content: "weak memory ripe for gc".to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    let args = serde_json::json!({ "dry_run": false });
    let result = execute_gc(&storage, Some(args)).await;
    let err = result.expect_err("destructive gc without confirmation must error out");
    assert!(
        err.contains("`gc`"),
        "error must name the operation: `{}`",
        err
    );
    assert!(
        err.contains("confirmed: true"),
        "error must show the required flag: `{}`",
        err
    );
    assert!(
        err.contains("requestedSchema"),
        "error must include the structured elicitation schema for clients to consume: `{}`",
        err
    );

    // CRITICAL: the gate must NOT mutate state. If we already started
    // deleting and only THEN errored, the user just lost data.
    let stats = storage.get_stats().unwrap();
    assert_eq!(
        stats.total_nodes, 1,
        "blocked gc must not delete anything: stats={:?}",
        stats
    );
}

#[tokio::test]
async fn test_gc_destructive_call_with_confirmation_proceeds() {
    let (storage, _dir) = test_storage().await;

    let args = serde_json::json!({
        "dry_run": false,
        "confirmed": true,
        // Set min_retention very high so even fresh memories count as candidates;
        // we want the destructive path to actually execute.
        "min_retention": 1.0,
    });
    let result = execute_gc(&storage, Some(args)).await;
    assert!(
        result.is_ok(),
        "confirmed destructive gc must proceed: {:?}",
        result.err()
    );
    let value = result.unwrap();
    assert_eq!(
        value["dryRun"], false,
        "must record that this WAS destructive"
    );
}

#[tokio::test]
async fn test_gc_rejects_string_confirmed_to_avoid_hallucinated_flags() {
    let (storage, _dir) = test_storage().await;
    // An over-eager agent might hallucinate `"confirmed": "true"` (string).
    // The boolean-only contract MUST block this.
    let args = serde_json::json!({
        "dry_run": false,
        "confirmed": "true",
    });
    let result = execute_gc(&storage, Some(args)).await;
    let err = result.expect_err("string `confirmed` must NOT be accepted as a boolean");
    assert!(
        err.contains("`gc`"),
        "expected gc-mentioning error: `{}`",
        err
    );
}

// ------------------------------------------------------------------
// split_memories carried the same destructive power as `gc` but none of
// its gate: `dry_run: false` deleted every compound memory it reported,
// with no acknowledgement, on a tool whose whole workflow is "read the
// report, decide, re-ingest atomically". These tests pin the gate.
// ------------------------------------------------------------------

#[test]
fn test_split_memories_schema_exposes_confirmed_flag() {
    let schema = super::split_memories::split_memories_schema();
    let props = schema["properties"]
        .as_object()
        .expect("split_memories schema must have properties");
    assert!(
        props.contains_key("confirmed"),
        "split_memories must advertise the `confirmed` flag so a client can prompt before the destructive call"
    );
    assert_eq!(props["confirmed"]["type"], "boolean");
    assert_eq!(props["confirmed"]["default"], false);
}

#[tokio::test]
async fn test_split_memories_dry_run_does_not_require_confirmation() {
    let (storage, _dir) = test_storage().await;
    let result =
        execute_split_memories(&storage, Some(serde_json::json!({ "dry_run": true }))).await;
    assert!(
        result.is_ok(),
        "dry_run=true is the safe default and must never require confirmation; got: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_split_memories_destructive_call_without_confirmation_is_blocked() {
    let (storage, _dir) = test_storage().await;
    // A compound memory that the detector would happily delete on dry_run:false.
    let compound = storage
        .ingest(vestige_core::IngestInput {
            content: "Session notes: the release moved to Friday because the migration slipped, \
                      the retry budget was raised to three, and the dashboard bundle was rebuilt."
                .to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    let before = storage.get_stats().unwrap().total_nodes;

    let result = execute_split_memories(
        &storage,
        Some(serde_json::json!({ "dry_run": false, "min_length": 100 })),
    )
    .await;

    let err = result.expect_err("dry_run:false without confirmed must be refused");
    assert!(
        err.contains("confirmed"),
        "the refusal must name the missing confirmation, got: {err}"
    );

    assert_eq!(
        storage.get_stats().unwrap().total_nodes,
        before,
        "nothing may be deleted when the gate refuses the call"
    );
    assert!(
        storage.get_node(&compound.id).unwrap().is_some(),
        "the compound memory must still exist"
    );
}

#[tokio::test]
async fn test_split_memories_confirmed_call_is_allowed() {
    let (storage, _dir) = test_storage().await;
    let result = execute_split_memories(
        &storage,
        Some(serde_json::json!({ "dry_run": false, "confirmed": true, "min_length": 100 })),
    )
    .await;
    assert!(
        result.is_ok(),
        "a confirmed destructive call must go through; got: {:?}",
        result.err()
    );
}
