use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Mutex;

use vestige_core::Storage;

use crate::cognitive::CognitiveEngine;

use super::compound::detect_compound_content;
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

#[tokio::test]
async fn test_smart_ingest_empty_content_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "content": "" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("empty"));
}

#[tokio::test]
async fn test_smart_ingest_basic_content_succeeds() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({
        "content": "This is a test fact to remember."
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value["success"], true);
    assert!(value["nodeId"].is_string());
    assert!(value["decision"].is_string());
}

#[tokio::test]
async fn test_smart_ingest_force_create() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({
        "content": "Force create test content.",
        "forceCreate": true
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value["success"], true);
    assert_eq!(value["decision"], "create");
    assert!(
        value["reason"].as_str().unwrap().contains("Forced")
            || value["reason"]
                .as_str()
                .unwrap()
                .contains("Embeddings not available")
    );
}

#[test]
fn test_schema_has_required_fields() {
    let schema_value = schema();
    assert_eq!(schema_value["type"], "object");
    assert!(schema_value["properties"]["content"].is_object());
    assert!(schema_value["properties"]["forceCreate"].is_object());
    assert!(schema_value["properties"]["items"].is_object());
    // v1.7: no top-level required — content for single mode, items for batch mode
    assert!(schema_value.get("required").is_none() || schema_value["required"].is_null());
}

#[tokio::test]
async fn test_smart_ingest_missing_args_fails() {
    let (storage, _dir) = test_storage().await;
    let result = execute(&storage, &test_cognitive(), None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Missing arguments"));
}

#[tokio::test]
async fn test_smart_ingest_whitespace_only_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "content": "   \t\n  " });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("empty"));
}

#[tokio::test]
async fn test_smart_ingest_too_large_fails() {
    let (storage, _dir) = test_storage().await;
    let large = "x".repeat(1_000_001);
    let args = serde_json::json!({ "content": large });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("too large"));
}

#[tokio::test]
async fn test_smart_ingest_exactly_1mb_succeeds() {
    let (storage, _dir) = test_storage().await;
    let content = "x".repeat(1_000_000);
    let args = serde_json::json!({ "content": content });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_smart_ingest_with_node_type() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({
        "content": "A concept to remember",
        "node_type": "concept"
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_smart_ingest_with_tags_and_source() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({
        "content": "Tagged and sourced memory",
        "tags": ["test", "smart-ingest"],
        "source": "unit-test"
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], true);
}

#[tokio::test]
async fn test_smart_ingest_response_has_importance_score() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "content": "Important memory content" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    let value = result.unwrap();
    assert!(value["importanceScore"].is_number());
}

#[tokio::test]
async fn test_smart_ingest_missing_content_field_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "tags": ["test"] });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("content"));
}

// ========================================================================
// TESTS PORTED FROM ingest.rs (v1.7.0 merge)
// ========================================================================

#[tokio::test]
async fn test_smart_ingest_with_all_optional_fields() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({
        "content": "Complex memory with all metadata.",
        "node_type": "decision",
        "tags": ["architecture", "design"],
        "source": "team meeting notes"
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], true);
    assert!(value["nodeId"].is_string());
}

#[tokio::test]
async fn test_smart_ingest_default_node_type_is_fact() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "content": "Default type test content." });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let node_id = result.unwrap()["nodeId"].as_str().unwrap().to_string();
    let node = storage.get_node(&node_id).unwrap().unwrap();
    assert_eq!(node.node_type, "fact");
}

// ========================================================================
// Causality graph — relation-derived edges are persisted with the right
// `link_type` so the `explore_connections` / causal-chain tools survive
// restarts. Pre 2026-05-22 these edges only ever lived in the in-memory
// activation network AND every link was forced to `Causal` regardless of
// the source verb. (Both bugs were RED-tested before fixing.)
// ========================================================================

/// Helper: ingest one piece of content and return its node id.
async fn ingest_content(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    content: &str,
) -> String {
    let args = serde_json::json!({ "content": content });
    let result = execute(storage, cognitive, Some(args))
        .await
        .expect("ingest");
    result["nodeId"].as_str().expect("nodeId").to_string()
}

/// Helper: force-create a memory so the prediction-error gate doesn't
/// silently merge it into a semantically similar existing memory. Used
/// by the causality-graph tests where we need two DISTINCT node ids to
/// observe edge persistence between them.
async fn ingest_force_create(
    storage: &Arc<Storage>,
    cognitive: &Arc<Mutex<CognitiveEngine>>,
    content: &str,
) -> String {
    let args = serde_json::json!({ "content": content, "forceCreate": true });
    let result = execute(storage, cognitive, Some(args))
        .await
        .expect("ingest");
    result["nodeId"].as_str().expect("nodeId").to_string()
}

/// Diagnostic: verify the preprocessing pipeline actually classifies
/// "Stress causes Insomnia" as a Causal relation BEFORE we rely on the
/// post-ingest edge writer.
#[cfg(feature = "preprocessing")]
#[test]
fn preprocess_classifies_causes_as_causal_at_pipeline_layer() {
    use vestige_core::neuroscience::spreading_activation::LinkType;
    use vestige_core::preprocessing::{PreprocessingConfig, preprocess};

    let result = preprocess(
        "The Stress causes the Insomnia disorder.",
        &PreprocessingConfig::default(),
    );
    let entity_names: Vec<&str> = result.entities.iter().map(|e| e.text.as_str()).collect();
    let predicates: Vec<(&str, LinkType)> = result
        .relations
        .iter()
        .map(|r| (r.predicate.as_str(), r.link_type))
        .collect();
    assert!(
        result
            .relations
            .iter()
            .any(|r| r.link_type == LinkType::Causal),
        "preprocess() must classify `causes` as Causal; entities={:?} relations={:?}",
        entity_names,
        predicates,
    );
}

#[cfg(feature = "preprocessing")]
#[tokio::test]
async fn keyword_search_finds_ingested_insomnia_node() {
    let (storage, _dir) = test_storage().await;
    let cognitive = test_cognitive();
    let insomnia_id = ingest_content(
        &storage,
        &cognitive,
        "The Insomnia disorder is widespread among knowledge workers.",
    )
    .await;
    let hits = storage
        .keyword_search("Insomnia", 5, 0.0)
        .expect("keyword_search must not error");
    let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
    assert!(
        ids.contains(&insomnia_id.as_str()),
        "keyword_search('Insomnia') must find the ingested node; \
         this is the prerequisite for create_relation_edges to attach \
         an edge to it. got ids={:?}",
        ids,
    );
}

#[cfg(feature = "preprocessing")]
#[tokio::test]
async fn smart_ingest_persists_causal_edges_to_storage() {
    let (storage, _dir) = test_storage().await;
    let cognitive = test_cognitive();

    // The proper-noun regex requires a prefix (`the`, `a`, `is`, punctuation, …)
    // so capitalised words MID-SENTENCE aren't laundered by the
    // start-of-sentence guard. Hence "the Insomnia disorder" rather than
    // "Insomnia" — both entities must be reachable to the SVO extractor
    // or the relation collapses to a noun-phrase fallback that
    // `keyword_search` can't resolve.
    let insomnia_id = ingest_force_create(
        &storage,
        &cognitive,
        "The Insomnia disorder is widespread among knowledge workers.",
    )
    .await;
    // forceCreate to skip the prediction-error gate, which would otherwise
    // merge the second ingest INTO the first (high semantic similarity)
    // and we'd never get a distinct stress_id to attach an edge to.
    let stress_id = ingest_force_create(
        &storage,
        &cognitive,
        "The Stress causes the Insomnia disorder.",
    )
    .await;
    assert_ne!(
        stress_id, insomnia_id,
        "forceCreate must yield distinct node ids — the rest of the test is meaningless otherwise"
    );

    let connections = storage
        .get_connections_for_memory(&stress_id)
        .expect("get_connections_for_memory must not error");
    let causal: Vec<&vestige_core::ConnectionRecord> = connections
        .iter()
        .filter(|c| c.link_type == "causal")
        .collect();
    assert!(
        !causal.is_empty(),
        "expected at least one persisted edge with link_type=causal after \
         `The Stress causes the Insomnia disorder.`, got: {:?}",
        connections.iter().map(|c| &c.link_type).collect::<Vec<_>>()
    );
    assert!(
        causal
            .iter()
            .any(|c| c.target_id == insomnia_id || c.source_id == insomnia_id),
        "causal edge must connect stress → insomnia; got: {:?}",
        causal,
    );
}

#[cfg(feature = "preprocessing")]
#[tokio::test]
async fn smart_ingest_does_not_mislabel_management_as_causal() {
    let (storage, _dir) = test_storage().await;
    let cognitive = test_cognitive();

    // Anchor for keyword resolution — "Auth Team" must already exist
    // for the post-ingest relation walker to find a target. forceCreate
    // on both ingests so the prediction-error gate doesn't merge them.
    let _team_id =
        ingest_force_create(&storage, &cognitive, "The Auth Team owns authentication.").await;
    let alice_id = ingest_force_create(
        &storage,
        &cognitive,
        "Alice manages the Auth Team since January.",
    )
    .await;

    let connections = storage
        .get_connections_for_memory(&alice_id)
        .expect("get_connections_for_memory must not error");
    for conn in &connections {
        assert_ne!(
            conn.link_type, "causal",
            "`manages` must not produce a Causal edge — pre-fix it did, polluting \
             causal traversals with hierarchical links. Got connection: {:?}",
            conn,
        );
    }
    // And the relation IS expected to exist — as semantic.
    assert!(
        connections.iter().any(|c| c.link_type == "semantic"),
        "expected a semantic edge for `Alice manages the Auth Team`; got: {:?}",
        connections.iter().map(|c| &c.link_type).collect::<Vec<_>>()
    );
}

#[test]
fn test_schema_has_optional_fields() {
    let schema_value = schema();
    assert!(schema_value["properties"]["node_type"].is_object());
    assert!(schema_value["properties"]["tags"].is_object());
    assert!(schema_value["properties"]["source"].is_object());
}

#[tokio::test]
async fn test_smart_ingest_with_source() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({
        "content": "MCP protocol version 2024-11-05 is the current standard.",
        "source": "https://modelcontextprotocol.io/spec"
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["success"], true);
}

// ========================================================================
// BATCH MODE TESTS (ported from checkpoint.rs, v1.7.0 merge)
// ========================================================================

#[tokio::test]
async fn test_batch_empty_items_fails() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "items": [] })),
    )
    .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("empty"));
}

#[tokio::test]
async fn test_batch_ingest() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [
                { "content": "First batch item", "tags": ["test"] },
                { "content": "Second batch item", "tags": ["test"] }
            ]
        })),
    )
    .await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["mode"], "batch");
    assert_eq!(value["summary"]["total"], 2);
}

#[tokio::test]
async fn test_batch_skips_empty_content() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [
                { "content": "Valid item" },
                { "content": "" },
                { "content": "Another valid item" }
            ]
        })),
    )
    .await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["summary"]["skipped"], 1);
}

#[tokio::test]
async fn test_batch_missing_args_fails() {
    let (storage, _dir) = test_storage().await;
    let result = execute(&storage, &test_cognitive(), None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Missing arguments"));
}

#[tokio::test]
async fn test_batch_exceeds_20_items_fails() {
    let (storage, _dir) = test_storage().await;
    let items: Vec<serde_json::Value> = (0..21)
        .map(|i| serde_json::json!({ "content": format!("Item {}", i) }))
        .collect();
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "items": items })),
    )
    .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Maximum 20 items"));
}

#[tokio::test]
async fn test_batch_exactly_20_items_succeeds() {
    let (storage, _dir) = test_storage().await;
    let items: Vec<serde_json::Value> = (0..20)
        .map(|i| serde_json::json!({ "content": format!("Item {}", i) }))
        .collect();
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "items": items })),
    )
    .await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["summary"]["total"], 20);
}

#[tokio::test]
async fn test_batch_skips_whitespace_only_content() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [
                { "content": "   \t\n  " },
                { "content": "Valid content" }
            ]
        })),
    )
    .await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["summary"]["skipped"], 1);
    assert_eq!(value["summary"]["created"], 1);
}

#[tokio::test]
async fn test_batch_single_item_succeeds() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [{ "content": "Single item" }]
        })),
    )
    .await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["summary"]["total"], 1);
    assert_eq!(value["success"], true);
}

#[tokio::test]
async fn test_batch_items_with_all_fields() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [{
                "content": "Full fields item",
                "tags": ["test", "batch"],
                "node_type": "decision",
                "source": "test-suite"
            }]
        })),
    )
    .await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["summary"]["created"], 1);
}

#[tokio::test]
async fn test_batch_results_array_matches_items() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [
                { "content": "First" },
                { "content": "" },
                { "content": "Third" }
            ]
        })),
    )
    .await;
    let value = result.unwrap();
    let results = value["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["index"], 0);
    assert_eq!(results[1]["index"], 1);
    assert_eq!(results[1]["status"], "skipped");
    assert_eq!(results[2]["index"], 2);
}

#[tokio::test]
async fn test_batch_success_true_when_only_skipped() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [
                { "content": "" },
                { "content": "   " }
            ]
        })),
    )
    .await;
    let value = result.unwrap();
    assert_eq!(value["success"], true); // skipped ≠ errors
    assert_eq!(value["summary"]["errors"], 0);
    assert_eq!(value["summary"]["skipped"], 2);
}

#[tokio::test]
async fn test_batch_has_importance_scores() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [{ "content": "Important batch memory content" }]
        })),
    )
    .await;
    let value = result.unwrap();
    let results = value["results"].as_array().unwrap();
    assert!(results[0]["importanceScore"].is_number());
}

#[tokio::test]
async fn test_batch_force_create_global() {
    let (storage, _dir) = test_storage().await;
    // Three items with very similar content + global forceCreate
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "forceCreate": true,
            "items": [
                { "content": "Physics question about quantum mechanics and wave functions" },
                { "content": "Physics question about quantum mechanics and wave equations" },
                { "content": "Physics question about quantum mechanics and wave behavior" }
            ]
        })),
    )
    .await;
    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value["mode"], "batch");
    // All 3 should be created separately, not merged
    assert_eq!(value["summary"]["created"], 3);
    assert_eq!(value["summary"]["updated"], 0);
    // Each result should say "Forced creation"
    let results = value["results"].as_array().unwrap();
    for r in results {
        assert_eq!(r["decision"], "create");
        assert!(r["reason"].as_str().unwrap().contains("Forced"));
    }
}

#[tokio::test]
async fn test_batch_force_create_per_item() {
    let (storage, _dir) = test_storage().await;
    // Mix of forced and non-forced items
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [
                { "content": "Forced item one", "forceCreate": true },
                { "content": "Normal item two" },
                { "content": "Forced item three", "forceCreate": true }
            ]
        })),
    )
    .await;
    assert!(result.is_ok());
    let value = result.unwrap();
    let results = value["results"].as_array().unwrap();
    // Forced items should say "Forced creation"
    assert_eq!(results[0]["decision"], "create");
    assert!(results[0]["reason"].as_str().unwrap().contains("Forced"));
    // Non-forced item gets normal processing
    assert_eq!(results[1]["status"], "saved");
    // Third forced item
    assert_eq!(results[2]["decision"], "create");
    assert!(results[2]["reason"].as_str().unwrap().contains("Forced"));
}

#[tokio::test]
async fn test_no_content_no_items_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "tags": ["orphan"] });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("content"));
}

#[test]
fn test_detect_compound_short_content_returns_none() {
    assert!(detect_compound_content("Short text").is_none());
    assert!(detect_compound_content("This is under 300 chars").is_none());
}

#[test]
fn test_detect_compound_multi_paragraph() {
    let content = "First paragraph about topic A: we discovered that the search engine \
                        has a fundamental issue with how it handles compound queries containing semicolons.\n\n\
                        Second paragraph about topic B: the deployment pipeline needs to be reconfigured \
                        because the staging environment is running out of disk space on the worker nodes.\n\n\
                        Third paragraph about topic C: John mentioned in the standup that he prefers \
                        using dark mode and wants us to add theme support to the internal dashboard tool.";
    assert!(
        content.len() >= 300,
        "Test content must be >=300 chars, got {}",
        content.len()
    );
    let result = detect_compound_content(content);
    assert!(result.is_some());
    assert!(result.unwrap().contains("COMPOUND CONTENT DETECTED"));
}

#[test]
fn test_detect_compound_speaker_pattern() {
    let content = "Alice: I think we should deploy on Friday because the staging tests passed and the team is ready.\n\
                        Bob: That works for me, but let's make sure to run the full integration test suite first before we proceed.\n\
                        Alice: Sure, I'll set up the CI pipeline today and configure the deployment scripts for the new environment.\n\
                        Carol: Can we also add a staging verification step before production? Last time we had issues with config.";
    assert!(
        content.len() >= 300,
        "Test content must be >=300 chars, got {}",
        content.len()
    );
    let result = detect_compound_content(content);
    assert!(result.is_some());
    assert!(result.unwrap().contains("conversation transcript"));
}

#[test]
fn test_detect_compound_bullet_list() {
    let content = "Session summary with multiple learnings from today's work session:\n\
                        - Fixed the authentication bug in login flow where tokens were not refreshed properly\n\
                        - Decided to migrate from MySQL to PostgreSQL for better JSON support and NOTIFY features\n\
                        - John prefers dark mode in all editors and wants the dashboard to support theme switching\n\
                        - Deployment deadline moved to next Friday because of the infrastructure migration blocking us\n\
                        - Added rate limiting to the API gateway to prevent abuse from unauthenticated clients";
    assert!(
        content.len() >= 300,
        "Test content must be >=300 chars, got {}",
        content.len()
    );
    let result = detect_compound_content(content);
    assert!(result.is_some());
    assert!(result.unwrap().contains("bulleted list"));
}

#[test]
fn test_detect_compound_single_fact_no_warning() {
    let content = "The hybrid_search function in vestige-core uses triple scoring: BM25 for lexical match, \
                        semantic embeddings for meaning match, and Reciprocal Rank Fusion to combine them. \
                        The default weights are 0.3 for BM25 and 0.7 for semantic. This is configured in \
                        the search module at crates/vestige-core/src/search/hybrid.rs.";
    assert!(detect_compound_content(content).is_none());
}

#[tokio::test]
async fn test_compound_warning_in_response() {
    let (storage, _dir) = test_storage().await;
    let compound = "Alice: We discussed the deployment plan for the new microservice architecture and decided on Friday.\n\
                         Bob: Let's do it Friday, but I want to make sure all the integration tests pass before we push to production.\n\
                         Carol: I agree with that plan and I'll prepare the rollback scripts just in case something goes wrong.\n\
                         Dave: Make sure the staging environment passes all health checks first and monitoring is configured properly.";
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "content": compound })),
    )
    .await;
    let value = result.unwrap();
    assert!(value["compound_content_warning"].is_string());
}

#[tokio::test]
async fn test_no_compound_warning_for_atomic() {
    let (storage, _dir) = test_storage().await;
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "content": "Single atomic fact about Rust memory safety." })),
    )
    .await;
    let value = result.unwrap();
    assert!(
        value.get("compound_content_warning").is_none()
            || value["compound_content_warning"].is_null()
    );
}
