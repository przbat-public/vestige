use std::sync::Arc;

use rusqlite::params;
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

/// A direct connection to the store file, for assertions about what was
/// written.
///
/// Deliberately not a `Storage` read: "was anything written?" asked through the
/// same layer that would have done the writing can be answered by a cache or a
/// filter, and the claim under test is about the file. The `Storage` writer is
/// still open in WAL mode, which permits a second reader.
fn store_conn(dir: &TempDir) -> rusqlite::Connection {
    rusqlite::Connection::open(dir.path().join("test.db")).unwrap()
}

/// Row count straight from a table, no read path in between.
fn count_rows(conn: &rusqlite::Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap_or_else(|e| panic!("counting {table}: {e}"))
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

/// An empty item is refused, not "skipped": a skip reads as work deferred to
/// later, and this content must never be stored at all.
#[tokio::test]
async fn test_batch_rejects_empty_content() {
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
    assert_eq!(value["summary"]["rejected"], 1);
    assert_eq!(value["summary"]["skipped"], 0);
    assert_eq!(value["results"][1]["decision"], "reject");
    assert_eq!(value["results"][1]["stored"], false);
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
async fn test_batch_rejects_whitespace_only_content() {
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
    assert_eq!(value["summary"]["rejected"], 1);
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
    assert_eq!(results[1]["status"], "rejected");
    assert_eq!(results[2]["index"], 2);
}

/// A batch that refuses every item is not a failed batch: the gate did its job,
/// and `success` tracks transport/ingest errors, not refusals.
#[tokio::test]
async fn test_batch_success_true_when_only_rejected() {
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
    assert_eq!(value["success"], true); // refused ≠ errors
    assert_eq!(value["summary"]["errors"], 0);
    assert_eq!(value["summary"]["rejected"], 2);
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

// ========================================================================
// WRITE GATE (wave 2): refusal writes nothing, a flag is persisted, and a
// tool-driven write says who wrote it.
// ========================================================================

/// The refusal, checked against the store rather than the response.
///
/// A test that only asserted `decision == "reject"` would pass while the memory
/// sat in `knowledge_nodes`, in the FTS index and in the embed queue — which is
/// exactly what the pre-wave-2 code did with this content: it wrote it and
/// labelled it. So every write path is attempted here and every table the write
/// would have touched is counted afterwards.
#[tokio::test]
async fn refused_content_is_not_written_anywhere() {
    let (storage, dir) = test_storage().await;
    let cognitive = test_cognitive();
    // A code block: the repository already stores this, so a copy of it is
    // wrong after the next commit with nothing to signal it.
    let content = "```rust\nfn merge(a: &[f32], b: &[f32]) -> Vec<f32> {\n    a.iter().chain(b).copied().collect()\n}\n```";

    // Both doors into the write path: the prediction-error path and the
    // force_create path that bypasses it.
    for force in [false, true] {
        let result = execute(
            &storage,
            &cognitive,
            Some(serde_json::json!({ "content": content, "forceCreate": force })),
        )
        .await
        .expect("a refusal is an answer, not a transport error");

        assert_eq!(
            result["decision"], "reject",
            "forceCreate={force}: {result}"
        );
        assert_eq!(result["success"], false, "forceCreate={force}: {result}");
        assert_eq!(
            result["stored"], false,
            "the refusal must state that nothing was written: {result}"
        );
        assert!(
            result["nodeId"].is_null(),
            "a refused memory has no id to hand back: {result}"
        );
        assert_eq!(
            result["findings"][0]["kind"], "derivable_from_repo",
            "the refusal must name the rule that fired: {result}"
        );
        assert!(
            result["guidance"]
                .as_str()
                .is_some_and(|g| g.contains("AGENTS.md")),
            "the refusal must say what to do instead: {result}"
        );
    }

    let conn = store_conn(&dir);
    assert_eq!(
        count_rows(&conn, "knowledge_nodes"),
        0,
        "a refused memory must not exist"
    );
    assert_eq!(
        count_rows(&conn, "memory_revisions"),
        0,
        "a memory that was never written has no history"
    );
    assert_eq!(
        count_rows(&conn, "node_embeddings"),
        0,
        "a refused memory must not be embedded"
    );
    assert_eq!(
        count_rows(&conn, "knowledge_fts"),
        0,
        "a refused memory must not be searchable"
    );

    let stats = storage.get_stats().unwrap();
    assert_eq!(stats.total_nodes, 0);
    assert_eq!(stats.nodes_with_embeddings, 0);
    assert!(
        storage.keyword_search("merge", 5, 0.0).unwrap().is_empty(),
        "a refused memory must not be reachable through search"
    );
}

/// The gate's other outcome, and the one the design makes the default: a
/// fixable memory is written and flagged. The flag has to outlive the response,
/// or nobody can find the memory later to fix it.
#[tokio::test]
async fn a_flagged_memory_is_stored_retrievable_and_marked() {
    let (storage, dir) = test_storage().await;
    let content = "BUG FIX: naprawiłem to, co omawialiśmy; Files: src/search.rs:112";
    let result = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "content": content, "forceCreate": true })),
    )
    .await
    .unwrap();

    assert_eq!(
        result["self_contained"]["requiresContext"], true,
        "a fixable memory is flagged, never refused: {result}"
    );
    assert_eq!(result["self_contained"]["rejected"], false);
    let node_id = result["nodeId"]
        .as_str()
        .expect("a flagged memory is still a memory");

    // Stored and readable.
    let node = storage
        .get_node(node_id)
        .unwrap()
        .expect("the flagged memory must be retrievable by id");
    assert_eq!(node.content, content);

    // Marked, in the file rather than in the mapper: 0 is "flagged", NULL
    // would be "never checked", and the difference is the whole point.
    let conn = store_conn(&dir);
    let (marker, findings): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT self_contained, self_contained_findings FROM knowledge_nodes WHERE id = ?1",
            params![node_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(marker, Some(0), "the flag must be persisted");
    let findings: serde_json::Value =
        serde_json::from_str(&findings.expect("the reason must be persisted with it")).unwrap();
    assert!(
        findings.as_array().is_some_and(|f| !f.is_empty()),
        "a marker without its findings is unforgeable only in the bad sense: {findings}"
    );

    // Still searchable: flagging is not quarantine.
    let hits = storage.keyword_search("naprawiłem", 5, 0.0).unwrap();
    assert!(
        hits.iter().any(|hit| hit.id == node_id),
        "a flagged memory must stay in the index"
    );
}

/// Wave 1 wrote `memory_revisions.actor` as NULL on every path because the
/// layer holding the identity is this one. An audit trail that cannot say who
/// wrote a version answers half the question it exists for.
#[tokio::test]
async fn a_tool_driven_write_records_who_wrote_it() {
    let (storage, _dir) = test_storage().await;
    let cognitive = test_cognitive();

    let single = execute(
        &storage,
        &cognitive,
        Some(serde_json::json!({
            "content": "Vestige records the writer on every revision it appends",
            "agent": "cursor",
            "session_id": "session-42",
        })),
    )
    .await
    .unwrap();
    let single_id = single["nodeId"].as_str().unwrap();

    let batch = execute(
        &storage,
        &cognitive,
        Some(serde_json::json!({
            "items": [{ "content": "A batch memory written by the same agent" }],
            "agent": "cursor",
            "session_id": "session-42",
        })),
    )
    .await
    .unwrap();
    let batch_id = batch["results"][0]["nodeId"].as_str().unwrap();

    for (label, node_id) in [("single", single_id), ("batch", batch_id)] {
        let revisions = storage.get_memory_revisions(node_id, 10).unwrap();
        let create = revisions
            .first()
            .unwrap_or_else(|| panic!("{label}: a memory starts with a create revision"));
        let actor = create
            .actor
            .as_deref()
            .unwrap_or_else(|| panic!("{label}: the actor must be recorded, not left NULL"));
        assert!(
            actor.contains("cursor") && actor.contains("session-42"),
            "{label}: the revision must name the agent and the conversation, got {actor}"
        );
    }
}

// ============================================================================
// Code anchors at write time
// ============================================================================

/// A path in the content is anchored, and the finding that exists to demand an
/// anchor stops firing.
///
/// This is the wiring the design asks for: `bare_code_reference` warns about a
/// path with no `code_ref`, so a write that has just created one must not be
/// warned about the same path. The two used to be unconnected — the gate looked
/// at the text and the anchors did not exist at all.
#[tokio::test]
async fn a_path_in_the_content_is_anchored_and_stops_being_a_bare_reference() {
    let (storage, dir) = test_storage().await;
    // A path that is not in the checkout the tests run from, so the verdict is
    // the honest `unchecked` rather than a `fresh` earned against whatever
    // repository happens to be the working directory. The version that *does*
    // observe a revision is covered against a throwaway repository in
    // `vestige_core::code_refs`.
    let content =
        "BUG FIX: the merge dropped a sub-query in crates/vestige-mcp/src/tools/absent_probe.rs";

    let value = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "content": content, "agent": "test" })),
    )
    .await
    .unwrap();

    let node_id = value["nodeId"].as_str().unwrap().to_string();
    let anchors = value["anchors"]
        .as_array()
        .expect("the write must report the anchors it stored");
    assert_eq!(anchors.len(), 1, "{value}");
    assert_eq!(
        anchors[0]["path"],
        "crates/vestige-mcp/src/tools/absent_probe.rs"
    );
    assert!(
        anchors[0]["symbol"].is_null(),
        "a symbol is never invented from prose: {value}"
    );
    assert_eq!(anchors[0]["verdict"], "unchecked");

    // The stored row, read straight from the file.
    let conn = store_conn(&dir);
    let (path, symbol, verdict): (String, Option<String>, String) = conn
        .query_row(
            "SELECT path, symbol, verdict FROM code_refs WHERE node_id = ?1",
            params![node_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(path, "crates/vestige-mcp/src/tools/absent_probe.rs");
    assert_eq!(symbol, None);
    assert_eq!(verdict, "unchecked");

    let kinds: Vec<String> = value["self_contained"]["findings"]
        .as_array()
        .map(|findings| {
            findings
                .iter()
                .filter_map(|f| f["kind"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !kinds.iter().any(|k| k == "bare_code_reference"),
        "the path has an anchor now, so the rule demanding one must not fire: {value}"
    );
}

/// Without the anchor the same text is flagged, which is what makes the test
/// above a statement about the wiring rather than about the rule being dead.
#[tokio::test]
async fn a_path_that_is_not_anchored_is_still_flagged() {
    let (storage, _dir) = test_storage().await;
    // A URL is the control: it looks like a reference but is not a checkout
    // path, so nothing anchors it and the gate has nothing to suppress.
    let value = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "content": "The report lives at https://example.invalid/report.json and Marek read it"
        })),
    )
    .await
    .unwrap();

    assert!(
        value.get("anchors").is_none(),
        "a URL is not a checkout reference: {value}"
    );
}

/// The explicit form is preferred over whatever the prose says, including its
/// revision and its symbol.
#[tokio::test]
async fn an_explicit_anchor_wins_over_the_path_in_the_content() {
    let (storage, dir) = test_storage().await;
    let value = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "content": "The retry loop in src/search.rs was the wrong layer",
            "codeRefs": ["src/search.rs@1cfdf45#run_retry_loop"]
        })),
    )
    .await
    .unwrap();

    let node_id = value["nodeId"].as_str().unwrap().to_string();
    let anchors = value["anchors"].as_array().unwrap();
    assert_eq!(anchors.len(), 1, "one path, one anchor: {value}");
    assert_eq!(anchors[0]["commit"], "1cfdf45");
    assert_eq!(anchors[0]["symbol"], "run_retry_loop");

    let conn = store_conn(&dir);
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM code_refs WHERE node_id = ?1 AND path = 'src/search.rs'",
            params![node_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 1,
        "the derived and explicit forms must not both land"
    );
}

/// Batch items carry their own anchors, and the gate sees them there too.
#[tokio::test]
async fn batch_items_store_their_own_anchors() {
    let (storage, dir) = test_storage().await;
    let value = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({
            "items": [
                { "content": "Marek prefers the migration to run before the deploy in scripts/deploy.sh" }
            ]
        })),
    )
    .await
    .unwrap();

    let node_id = value["results"][0]["nodeId"].as_str().unwrap().to_string();
    let conn = store_conn(&dir);
    let path: String = conn
        .query_row(
            "SELECT path FROM code_refs WHERE node_id = ?1",
            params![node_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(path, "scripts/deploy.sh");
    assert!(
        value["results"][0]["anchors"].is_array(),
        "a batch item reports its anchors like the single mode does: {value}"
    );
}

/// A reported contradiction has to reach the writer, or the write path's whole
/// restraint is invisible: the memory is stored, nothing is retired, and the
/// only place that says "this may deny something you already have" is the
/// response. Before the report was attached, an agent saw `decision: "create"`
/// with a high similarity and no way to tell a fresh fact from a contradiction.
#[tokio::test]
async fn a_reported_contradiction_reaches_the_response() {
    let (storage, _dir) = test_storage().await;
    if !storage.embedding_service_ready() {
        eprintln!("embedding service not ready — the gate cannot compare candidates");
        return;
    }
    let cognitive = test_cognitive();

    let existing = storage
        .ingest(vestige_core::IngestInput {
            content: "We use synchronous code for the payment callback because it is simpler to \
                      reason about."
                .to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();

    // No `forceCreate`: the gate must see the neighbour and decide.
    let result = execute(
        &storage,
        &cognitive,
        Some(serde_json::json!({
            "content": "Don't use synchronous code for the payment callback; it blocks the worker.",
            "node_type": "fact"
        })),
    )
    .await
    .expect("ingest");

    let report = &result["contradiction"];
    assert_eq!(
        report["existingId"].as_str(),
        Some(existing.id.as_str()),
        "the report must name the memory it may deny: {result}"
    );
    assert!(
        report["confidence"].as_f64().is_some_and(|c| c >= 0.8),
        "the report carries the evidence strength: {result}"
    );
    assert!(
        report["evidence"].as_array().is_some_and(|e| !e.is_empty()),
        "a report without its evidence is not actionable: {result}"
    );
    assert!(
        report["hint"]
            .as_str()
            .is_some_and(|h| h.contains("invalidate")),
        "the hint must say how to retire the older memory if that is what was meant: {result}"
    );

    // And the older memory is still exactly as it was: reported, not retired.
    let untouched = storage.get_node(&existing.id).unwrap().unwrap();
    assert!(
        untouched.valid_until.is_none(),
        "nothing may date a memory that nobody retired"
    );
}
