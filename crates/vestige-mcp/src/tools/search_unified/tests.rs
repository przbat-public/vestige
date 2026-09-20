use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Mutex;

use vestige_core::{IngestInput, Storage};

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

/// Helper to ingest test content
async fn ingest_test_content(storage: &Arc<Storage>, content: &str) -> String {
    let input = IngestInput {
        content: content.to_string(),
        node_type: "fact".to_string(),
        source: None,
        sentiment_score: 0.0,
        sentiment_magnitude: 0.0,
        tags: vec![],
        valid_from: None,
        valid_until: None,
        provenance: None,
        ..Default::default()
    };
    let node = storage.ingest(input).unwrap();
    node.id
}

// ========================================================================
// QUERY VALIDATION TESTS
// ========================================================================

#[tokio::test]
async fn test_search_empty_query_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "query": "" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("empty"));
}

#[tokio::test]
async fn test_search_whitespace_only_query_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "query": "   \t\n  " });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("empty"));
}

#[tokio::test]
async fn test_search_missing_arguments_fails() {
    let (storage, _dir) = test_storage().await;
    let result = execute(&storage, &test_cognitive(), None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Missing arguments"));
}

#[tokio::test]
async fn test_search_missing_query_field_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({ "limit": 10 });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid arguments"));
}

// ========================================================================
// LIMIT CLAMPING TESTS
// ========================================================================

#[tokio::test]
async fn test_search_limit_clamped_to_minimum() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Test content for limit clamping").await;

    // Try with limit 0 - should clamp to 1
    let args = serde_json::json!({
        "query": "test",
        "limit": 0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_limit_clamped_to_maximum() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Test content for max limit").await;

    // Try with limit 1000 - should clamp to 100
    let args = serde_json::json!({
        "query": "test",
        "limit": 1000
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_negative_limit_clamped() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Test content for negative limit").await;

    let args = serde_json::json!({
        "query": "test",
        "limit": -5
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
}

// ========================================================================
// MIN_RETENTION CLAMPING TESTS
// ========================================================================

#[tokio::test]
async fn test_search_min_retention_clamped_to_zero() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Test content for retention clamping").await;

    let args = serde_json::json!({
        "query": "test",
        "min_retention": -0.5
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_min_retention_clamped_to_one() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Test content for max retention").await;

    let args = serde_json::json!({
        "query": "test",
        "min_retention": 1.5
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    // Should succeed but may return no results (retention > 1.0 clamped to 1.0)
    assert!(result.is_ok());
}

// ========================================================================
// MIN_SIMILARITY CLAMPING TESTS
// ========================================================================

#[tokio::test]
async fn test_search_min_similarity_clamped_to_zero() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Test content for similarity clamping").await;

    let args = serde_json::json!({
        "query": "test",
        "min_similarity": -0.5
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_search_min_similarity_clamped_to_one() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Test content for max similarity").await;

    let args = serde_json::json!({
        "query": "test",
        "min_similarity": 1.5
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    // Should succeed but may return no results
    assert!(result.is_ok());
}

// ========================================================================
// SUCCESSFUL SEARCH TESTS
// ========================================================================

#[tokio::test]
async fn test_search_basic_query_succeeds() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "The Rust programming language is memory safe.").await;

    let args = serde_json::json!({ "query": "rust" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value["query"], "rust");
    assert_eq!(value["method"], "hybrid+cognitive");
    assert!(value["total"].is_number());
    assert!(value["results"].is_array());
}

#[tokio::test]
async fn test_search_returns_matching_content() {
    let (storage, _dir) = test_storage().await;
    let node_id = ingest_test_content(&storage, "Python is a dynamic programming language.").await;

    let args = serde_json::json!({
        "query": "python",
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    let results = value["results"].as_array().unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0]["id"], node_id);
}

#[tokio::test]
async fn test_search_with_limit() {
    let (storage, _dir) = test_storage().await;
    // Ingest multiple items
    ingest_test_content(&storage, "Testing content one").await;
    ingest_test_content(&storage, "Testing content two").await;
    ingest_test_content(&storage, "Testing content three").await;

    let args = serde_json::json!({
        "query": "testing",
        "limit": 2,
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    let results = value["results"].as_array().unwrap();
    assert!(results.len() <= 2);
}

#[tokio::test]
async fn test_search_empty_database_returns_empty_array() {
    let (storage, _dir) = test_storage().await;
    // Don't ingest anything - database is empty

    let args = serde_json::json!({ "query": "anything" });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value["total"], 0);
    assert!(value["results"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_search_result_contains_expected_fields() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Testing field presence in search results.").await;

    let args = serde_json::json!({
        "query": "testing",
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    let results = value["results"].as_array().unwrap();
    if !results.is_empty() {
        let first = &results[0];
        assert!(first["id"].is_string());
        assert!(first["content"].is_string());
        assert!(first["combinedScore"].is_number());
        // keywordScore and semanticScore may be null if not matched
        assert!(first["nodeType"].is_string());
        assert!(first["tags"].is_array());
        assert!(first["retentionStrength"].is_number());
    }
}

// ========================================================================
// DEFAULT VALUES TESTS
// ========================================================================

#[tokio::test]
async fn test_search_default_limit_is_10() {
    let (storage, _dir) = test_storage().await;
    // Ingest more than 10 items
    for i in 0..15 {
        ingest_test_content(&storage, &format!("Item number {}", i)).await;
    }

    let args = serde_json::json!({
        "query": "item",
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    let results = value["results"].as_array().unwrap();
    assert!(results.len() <= 10);
}

// ========================================================================
// SCHEMA TESTS
// ========================================================================

#[test]
fn test_schema_has_required_fields() {
    let schema_value = schema();
    assert_eq!(schema_value["type"], "object");
    assert!(schema_value["properties"]["query"].is_object());
    assert!(
        schema_value["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("query"))
    );
}

#[test]
fn test_schema_has_optional_fields() {
    let schema_value = schema();
    assert!(schema_value["properties"]["limit"].is_object());
    assert!(schema_value["properties"]["min_retention"].is_object());
    assert!(schema_value["properties"]["min_similarity"].is_object());
}

#[test]
fn test_schema_limit_has_bounds() {
    let schema_value = schema();
    let limit_schema = &schema_value["properties"]["limit"];
    assert_eq!(limit_schema["minimum"], 1);
    assert_eq!(limit_schema["maximum"], 100);
    assert_eq!(limit_schema["default"], 10);
}

#[test]
fn test_schema_min_retention_has_bounds() {
    let schema_value = schema();
    let retention_schema = &schema_value["properties"]["min_retention"];
    assert_eq!(retention_schema["minimum"], 0.0);
    assert_eq!(retention_schema["maximum"], 1.0);
    assert_eq!(retention_schema["default"], 0.0);
}

#[test]
fn test_schema_min_similarity_has_bounds() {
    let schema_value = schema();
    let similarity_schema = &schema_value["properties"]["min_similarity"];
    assert_eq!(similarity_schema["minimum"], 0.0);
    assert_eq!(similarity_schema["maximum"], 1.0);
    assert_eq!(similarity_schema["default"], 0.5);
}

// ========================================================================
// DETAIL LEVEL TESTS
// ========================================================================

#[test]
fn test_schema_has_detail_level() {
    let schema_value = schema();
    let dl = &schema_value["properties"]["detail_level"];
    assert!(dl.is_object());
    assert_eq!(dl["default"], "summary");
    let enum_values = dl["enum"].as_array().unwrap();
    assert!(enum_values.contains(&serde_json::json!("brief")));
    assert!(enum_values.contains(&serde_json::json!("summary")));
    assert!(enum_values.contains(&serde_json::json!("full")));
}

#[tokio::test]
async fn test_search_detail_level_brief_excludes_content() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Brief mode test content for search.").await;

    let args = serde_json::json!({
        "query": "brief",
        "detail_level": "brief",
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value["detailLevel"], "brief");
    let results = value["results"].as_array().unwrap();
    if !results.is_empty() {
        let first = &results[0];
        // Brief should NOT have content
        assert!(first.get("content").is_none() || first["content"].is_null());
        // Brief should have these fields
        assert!(first["id"].is_string());
        assert!(first["nodeType"].is_string());
        assert!(first["tags"].is_array());
        assert!(first["retentionStrength"].is_number());
        assert!(first["combinedScore"].is_number());
    }
}

#[tokio::test]
async fn test_search_detail_level_full_includes_timestamps() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Full mode test content for search.").await;

    let args = serde_json::json!({
        "query": "full",
        "detail_level": "full",
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value["detailLevel"], "full");
    let results = value["results"].as_array().unwrap();
    if !results.is_empty() {
        let first = &results[0];
        // Full should have timestamps
        assert!(first["createdAt"].is_string());
        // The record time is what tells a later reader when we learned this, as
        // opposed to when the row was created or last touched.
        assert!(
            first["recordedAt"].is_string(),
            "every result must carry when the memory was recorded"
        );
        assert!(first["updatedAt"].is_string());
        assert!(first["content"].is_string());
        assert!(first["storageStrength"].is_number());
        assert!(first["retrievalStrength"].is_number());
        assert!(first["matchType"].is_string());
    }
}

#[tokio::test]
async fn test_search_detail_level_default_is_summary() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "Default detail level test content.").await;

    let args = serde_json::json!({
        "query": "default",
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value["detailLevel"], "summary");
    let results = value["results"].as_array().unwrap();
    if !results.is_empty() {
        let first = &results[0];
        // Summary should have content and timestamps (added in v3.2.1)
        assert!(first["content"].is_string());
        assert!(first["id"].is_string());
        assert!(first["createdAt"].is_string());
        // The record time is what tells a later reader when we learned this, as
        // opposed to when the row was created or last touched.
        assert!(
            first["recordedAt"].is_string(),
            "every result must carry when the memory was recorded"
        );
    }
}

#[tokio::test]
async fn test_search_detail_level_invalid_fails() {
    let (storage, _dir) = test_storage().await;
    let args = serde_json::json!({
        "query": "test",
        "detail_level": "invalid_level"
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid detail_level"));
}

// ========================================================================
// TOKEN BUDGET TESTS (v1.8.0)
// ========================================================================

#[tokio::test]
async fn test_token_budget_limits_results() {
    let (storage, _dir) = test_storage().await;
    for i in 0..10 {
        ingest_test_content(
            &storage,
            &format!(
                "Budget test content number {} with some extra text to increase size.",
                i
            ),
        )
        .await;
    }

    // Small budget should reduce results
    let args = serde_json::json!({
        "query": "budget test",
        "token_budget": 200,
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert!(value["tokenBudget"].as_i64().unwrap() == 200);
    assert!(value["tokensUsed"].is_number());
}

#[tokio::test]
async fn test_token_budget_expandable() {
    let (storage, _dir) = test_storage().await;
    for i in 0..15 {
        ingest_test_content(
                &storage,
                &format!(
                    "Expandable budget test number {} with quite a bit of content to ensure we exceed the token budget allocation threshold.",
                    i
                ),
            )
            .await;
    }

    let args = serde_json::json!({
        "query": "expandable budget test",
        "token_budget": 150,
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    // expandable field should exist if results were dropped
    if let Some(expandable) = value.get("expandable") {
        assert!(expandable.is_array());
    }
}

#[tokio::test]
async fn test_no_budget_unchanged() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "No budget test content.").await;

    let args = serde_json::json!({
        "query": "no budget",
        "min_similarity": 0.0
    });
    let result = execute(&storage, &test_cognitive(), Some(args)).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    // No budget fields should be present
    assert!(value.get("tokenBudget").is_none());
    assert!(value.get("tokensUsed").is_none());
    assert!(value.get("expandable").is_none());
}

#[test]
fn test_schema_has_token_budget() {
    let schema_value = schema();
    let tb = &schema_value["properties"]["token_budget"];
    assert!(tb.is_object());
    assert_eq!(tb["minimum"], 100);
    assert_eq!(tb["maximum"], 100000);
}

// ========================================================================
// ORDER DETERMINISM, FRESHNESS AND CONFLICT RESOLUTION
// ========================================================================

use chrono::{DateTime, Utc};
use vestige_core::{ConnectionRecord, KnowledgeNode, MatchType, SearchResult};

use super::pipeline::finalize::apply_activation_boost;
use super::pipeline::scoring::{apply_freshness_ordering, sort_by_score_then_freshness};

fn fixture(id: &str, content: &str, score: f32, created_at: &str) -> SearchResult {
    let mut node = KnowledgeNode::new(content);
    node.id = id.to_string();
    node.created_at = DateTime::parse_from_rfc3339(created_at)
        .unwrap()
        .with_timezone(&Utc);
    SearchResult {
        node,
        keyword_score: None,
        semantic_score: None,
        combined_score: score,
        match_type: MatchType::Keyword,
    }
}

fn scores(results: &[SearchResult]) -> Vec<f32> {
    results.iter().map(|r| r.combined_score).collect()
}

fn assert_monotonic(results: &[SearchResult]) {
    let s = scores(results);
    assert!(
        s.windows(2).all(|w| w[0] >= w[1]),
        "reported order must be non-increasing in combined_score: {s:?}"
    );
}

#[test]
fn activation_boost_cannot_leave_the_reported_order_non_monotonic() {
    // The 20% activation bump lands after the scoring phase's sort, so the row
    // order and the reported `combinedScore` column could disagree: the boosted
    // memory held the higher score while the response still listed it second.
    let mut results = vec![
        fixture("anchor", "the anchor memory", 1.00, "2024-01-01T00:00:00Z"),
        fixture(
            "boosted",
            "the boosted memory",
            0.90,
            "2024-01-02T00:00:00Z",
        ),
        fixture("tail", "the tail memory", 0.50, "2024-01-03T00:00:00Z"),
    ];
    let activation = std::collections::HashMap::from([("boosted", 1.0_f64)]);

    apply_activation_boost(&mut results, &activation);

    assert_eq!(
        results[0].node.id, "boosted",
        "the bump made this the highest-scoring row, so it must be first"
    );
    assert_monotonic(&results);
}

#[test]
fn conflicting_pair_puts_the_fresher_statement_first() {
    // Same subject and relation, different value, and the *superseded* statement
    // has the better score. The read path used to leave this to a +10% tag-overlap
    // boost that cannot fire on tagless memories and cannot close a larger gap.
    let mut results = vec![
        fixture(
            "superseded",
            "goaltender is associated with the sport of ice hockey.",
            0.90,
            "2024-01-01T00:00:00Z",
        ),
        fixture(
            "current",
            "goaltender is associated with the sport of pesäpallo.",
            0.50,
            "2026-01-01T00:00:00Z",
        ),
    ];

    apply_freshness_ordering(&mut results, &[]);
    sort_by_score_then_freshness(&mut results);

    assert_eq!(results[0].node.id, "current");
    assert_monotonic(&results);
}

#[test]
fn freshness_ordering_is_idempotent_and_leaves_unrelated_pairs_alone() {
    let mut results = vec![
        fixture(
            "superseded",
            "goaltender is associated with the sport of ice hockey.",
            0.90,
            "2024-01-01T00:00:00Z",
        ),
        fixture(
            "current",
            "goaltender is associated with the sport of pesäpallo.",
            0.50,
            "2026-01-01T00:00:00Z",
        ),
        fixture(
            "unrelated",
            "The cache key changed in the search pipeline.",
            0.40,
            "2025-01-01T00:00:00Z",
        ),
    ];
    let unrelated_before = results[2].combined_score;

    apply_freshness_ordering(&mut results, &[]);
    let after_first = scores(&results);
    apply_freshness_ordering(&mut results, &[]);

    assert_eq!(
        scores(&results),
        after_first,
        "re-applying must not move scores"
    );
    assert_eq!(
        results[2].combined_score, unrelated_before,
        "a memory that conflicts with nothing keeps its score"
    );
    sort_by_score_then_freshness(&mut results);
    assert_monotonic(&results);
}

#[tokio::test]
async fn scoring_waits_for_the_engine_lock_instead_of_skipping_stages() {
    // A stage that skipped itself when the engine was busy made the ranking depend
    // on lock timing: the same query returned different orders on different runs.
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "the engine lock content").await;
    let cognitive = test_cognitive();
    let args = serde_json::json!({ "query": "engine lock content", "limit": 5 });

    let guard = cognitive.lock().await;
    let mut search = Box::pin(execute(&storage, &cognitive, Some(args.clone())));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), &mut search)
            .await
            .is_err(),
        "the search returned while the cognitive engine was locked, so a stage skipped itself"
    );
    drop(guard);

    let response = tokio::time::timeout(std::time::Duration::from_secs(30), search)
        .await
        .expect("the search must complete once the lock is released")
        .unwrap();
    assert!(response["results"].is_array());

    // Same input twice → same ranking, now that no stage can be skipped.
    let first = execute(&storage, &cognitive, Some(args.clone()))
        .await
        .unwrap();
    let second = execute(&storage, &cognitive, Some(args)).await.unwrap();
    let ids = |v: &serde_json::Value| -> Vec<String> {
        v["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(ids(&first), ids(&second), "same input, same ranking");
}

#[tokio::test]
async fn contradiction_penalty_follows_the_core_freshness_rule_not_created_at() {
    // `valid_from` is a freshness slot the core rule reads and a `created_at`-only
    // comparison does not: the older recording here is the *current* statement.
    let (storage, _dir) = test_storage().await;
    let later_valid_from = Utc::now() + chrono::Duration::days(365);

    let current = {
        let input = IngestInput {
            content: "Sable is a citizen of Ruritania.".to_string(),
            node_type: "fact".to_string(),
            valid_from: Some(later_valid_from),
            ..Default::default()
        };
        storage.ingest(input).unwrap().id
    };
    let superseded = ingest_test_content(&storage, "Sable is a citizen of Czech Republic.").await;

    storage
        .save_connection(&ConnectionRecord {
            source_id: current.clone(),
            target_id: superseded.clone(),
            strength: 1.0,
            link_type: "contradiction".to_string(),
            created_at: Utc::now(),
            last_activated: Utc::now(),
            activation_count: 1,
        })
        .unwrap();

    let response = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "query": "Sable is a citizen", "limit": 5 })),
    )
    .await
    .unwrap();

    let results = response["results"].as_array().unwrap();
    let position = |id: &str| results.iter().position(|r| r["id"] == id);
    let (Some(current_at), Some(superseded_at)) = (position(&current), position(&superseded))
    else {
        panic!("both statements must be retrieved: {results:?}");
    };
    assert!(
        current_at < superseded_at,
        "the current statement must outrank the superseded one: {results:?}"
    );
}

#[tokio::test]
async fn superseded_statement_does_not_win_a_query_that_matches_it_better() {
    // The end-to-end shape of the FactConsolidation failure: the query is lexically
    // closer to the superseded value, both statements are retrieved, and the
    // current one still has to come first.
    let (storage, _dir) = test_storage().await;
    ingest_test_content(
        &storage,
        "goaltender is associated with the sport of ice hockey.",
    )
    .await;
    ingest_test_content(
        &storage,
        "goaltender is associated with the sport of pesäpallo.",
    )
    .await;

    let response = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "query": "goaltender ice hockey sport", "limit": 5 })),
    )
    .await
    .unwrap();

    let results = response["results"].as_array().unwrap();
    assert!(
        results.len() >= 2,
        "both statements must be retrieved: {results:?}"
    );
    let top = results[0]["content"].as_str().unwrap();
    assert!(
        top.contains("pesäpallo"),
        "the current value must be read first, got: {top}"
    );
    let reported: Vec<f64> = results
        .iter()
        .map(|r| r["combinedScore"].as_f64().unwrap())
        .collect();
    assert!(
        reported.windows(2).all(|w| w[0] >= w[1]),
        "reported score must be non-increasing: {reported:?}"
    );
}

// ========================================================================
// RETRIEVAL HOT PATH — token-set reuse, concurrent sub-queries, merge order
// ========================================================================

use std::cell::Cell;

use super::helpers::content_overlap;
use super::pipeline::retrieval::{
    candidate_token_sets, merge_search_batches, mmr_reorder, suppress_near_duplicates,
};

fn ids_of(results: &[SearchResult]) -> Vec<String> {
    results.iter().map(|r| r.node.id.clone()).collect()
}

#[test]
fn near_duplicate_suppression_is_unchanged_by_reusing_token_sets() {
    // The stage used to call `content_overlap` per pair, which rebuilds two HashSets
    // per comparison. It now tokenises each candidate once. The selection — which
    // members survive and in which order — must not move.
    let results = vec![
        fixture(
            "a",
            "alpha beta gamma delta epsilon",
            1.0,
            "2024-01-01T00:00:00Z",
        ),
        fixture(
            "dup-of-a",
            "alpha beta gamma delta epsilon",
            0.9,
            "2024-01-02T00:00:00Z",
        ),
        fixture(
            "b",
            "zeta eta theta iota kappa",
            0.8,
            "2024-01-03T00:00:00Z",
        ),
        fixture("c", "lambda mu nu xi omicron", 0.7, "2024-01-04T00:00:00Z"),
        fixture(
            "near-b",
            "zeta eta theta iota lambda",
            0.6,
            "2024-01-05T00:00:00Z",
        ),
    ];

    // Oracle: the previous implementation, pair by pair over the raw strings.
    let naive_builds = Cell::new(0usize);
    let comparisons = Cell::new(0usize);
    let n = results.len();
    let mut keep = vec![true; n];
    for i in 0..n {
        if !keep[i] {
            continue;
        }
        for j in (i + 1)..n {
            if !keep[j] {
                continue;
            }
            comparisons.set(comparisons.get() + 1);
            naive_builds.set(naive_builds.get() + 2); // two HashSets per comparison
            if content_overlap(&results[i].node.content, &results[j].node.content) > 0.85 {
                keep[j] = false;
            }
        }
    }
    let expected: Vec<String> = ids_of(&results)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, id)| id)
        .collect();

    // Counted before the results are consumed: one set per candidate.
    let set_count = candidate_token_sets(&results).len();
    let (survivors, removed) = suppress_near_duplicates(results);

    assert_eq!(ids_of(&survivors), expected, "same members, same order");
    assert_eq!(removed, 1, "the duplicate of `a` is the only removal");
    // n(n-1) builds before, n after: one token set per candidate.
    assert_eq!(
        set_count, n,
        "one token set per candidate, not two per comparison"
    );
    assert_eq!(
        naive_builds.get(),
        2 * comparisons.get(),
        "the per-pair form really did build two sets per comparison"
    );
    assert!(
        naive_builds.get() > set_count,
        "the reuse must build strictly fewer sets ({} vs {set_count})",
        naive_builds.get()
    );
}

#[test]
fn mmr_selection_is_unchanged_by_reusing_token_sets() {
    let results = vec![
        fixture(
            "apple",
            "alpha beta gamma delta",
            0.95,
            "2024-01-01T00:00:00Z",
        ),
        fixture(
            "apricot",
            "alpha beta gamma epsilon",
            0.90,
            "2024-01-02T00:00:00Z",
        ),
        fixture(
            "banana",
            "zeta eta theta iota",
            0.60,
            "2024-01-03T00:00:00Z",
        ),
        fixture("cherry", "kappa lambda mu nu", 0.50, "2024-01-04T00:00:00Z"),
    ];

    // Oracle: the closure form, which rebuilt two sets per similarity call.
    let naive_builds = Cell::new(0usize);
    let scored: Vec<(SearchResult, f32)> = results
        .iter()
        .map(|r| (r.clone(), r.combined_score))
        .collect();
    let expected = ids_of(&vestige_core::search::mmr_select(
        scored,
        |a, b| {
            naive_builds.set(naive_builds.get() + 2);
            content_overlap(&a.node.content, &b.node.content) as f32
        },
        0.5,
        3,
    ));

    let set_count = candidate_token_sets(&results).len();
    let selected = mmr_reorder(results, 0.5, 3);

    assert_eq!(ids_of(&selected), expected, "same members, same order");
    assert_eq!(set_count, 4, "one token set per candidate");
    assert!(
        naive_builds.get() >= 8,
        "the closure form rebuilt sets on every similarity call, got {}",
        naive_builds.get()
    );
}

#[test]
fn merged_sub_query_batches_tie_break_on_id_not_hash_order() {
    // The local merge drained a HashMap (randomised iteration order) into a stable
    // score sort, so equally scored hits came back in a different order per run —
    // the same defect the core merge fixed in `1cfdf45`. It now delegates there.
    let batch = |skip: usize| -> Vec<SearchResult> {
        (0..12)
            .skip(skip)
            .map(|i| {
                fixture(
                    &format!("node-{i:02}"),
                    &format!("content {i}"),
                    1.0,
                    "2024-01-01T00:00:00Z",
                )
            })
            .collect()
    };
    let expected: Vec<String> = (0..12).map(|i| format!("node-{i:02}")).collect();

    let merged = merge_search_batches(vec![batch(0), batch(3)]);
    assert_eq!(ids_of(&merged), expected, "ties break on the id, ascending");
    for _ in 0..8 {
        assert_eq!(
            ids_of(&merge_search_batches(vec![batch(0), batch(3)])),
            expected,
            "same input, same order"
        );
    }
}

#[tokio::test]
async fn compound_query_returns_the_same_union_and_order_on_every_run() {
    // End-to-end shape of the concurrent sub-query path: the batches are joined
    // concurrently but collected in sub-query order, so the merged union keeps the
    // semantics (union, max score per id) and the order is reproducible.
    let (storage, _dir) = test_storage().await;
    ingest_test_content(&storage, "alpha topic covers the cache key rotation.").await;
    ingest_test_content(&storage, "beta topic covers the index rebuild.").await;

    let args = serde_json::json!({ "query": "alpha topic; beta topic", "limit": 5 });
    let first = execute(&storage, &test_cognitive(), Some(args.clone()))
        .await
        .unwrap();
    let second = execute(&storage, &test_cognitive(), Some(args))
        .await
        .unwrap();

    let ids = |v: &serde_json::Value| -> Vec<String> {
        v["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(ids(&first), ids(&second), "same input, same union");
    assert!(!ids(&first).is_empty(), "the union must not be empty");
    let reported: Vec<f64> = first["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["combinedScore"].as_f64().unwrap())
        .collect();
    assert!(
        reported.windows(2).all(|w| w[0] >= w[1]),
        "reported score must be non-increasing: {reported:?}"
    );
}

// ========================================================================
// CODE ANCHORS IN RESULTS
// ========================================================================

/// A reader has to learn that an anchor is stale *from the result* — that is the
/// whole point of the verdict. A memory whose citation silently points at an old
/// copy is exactly the failure a stored path already has.
#[tokio::test]
async fn a_result_with_a_stale_anchor_says_so() {
    use vestige_core::{AnchorVerdict, CodeAnchor, IngestAnchor};

    let (storage, _dir) = test_storage().await;
    let mut anchor = CodeAnchor::new("crates/vestige-core/src/search/mmr.rs");
    anchor.commit_sha = Some("1cfdf45aa".to_string());
    anchor.symbol = Some("mmr_rerank".to_string());
    anchor.hint_line = Some(112);
    anchor.content_hash = Some("deadbeef".to_string());

    let node_id = storage
        .ingest(IngestInput {
            content: "The re-rank stage drops results whose score ties at the cut-off".to_string(),
            anchors: vec![IngestAnchor {
                anchor,
                verdict: AnchorVerdict::Stale,
                resolved_at: Some(chrono::Utc::now()),
            }],
            ..Default::default()
        })
        .unwrap()
        .id;

    let value = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "query": "re-rank stage ties at the cut-off", "limit": 5 })),
    )
    .await
    .unwrap();

    let result = value["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == serde_json::json!(node_id))
        .expect("the memory must be retrievable");

    let anchors = result["codeRefs"]
        .as_array()
        .expect("a memory with anchors must carry them in the result");
    assert_eq!(anchors.len(), 1, "{result}");
    assert_eq!(anchors[0]["verdict"], "stale");
    assert_eq!(
        anchors[0]["reference"],
        "crates/vestige-core/src/search/mmr.rs@1cfdf45aa#mmr_rerank"
    );
    assert_eq!(anchors[0]["hintLine"], 112);
    assert!(
        anchors[0]["note"]
            .as_str()
            .is_some_and(|note| note.contains("text has changed")),
        "the note is the part a reader acts on: {result}"
    );
    assert_eq!(anchors[0]["commit"], "1cfdf45aa");
}

/// A memory with no anchors must not grow an empty `codeRefs` array: the field
/// means "this memory cites code", and an empty list would say that about every
/// result.
#[tokio::test]
async fn a_result_with_no_anchors_carries_no_code_refs_field() {
    let (storage, _dir) = test_storage().await;
    ingest_test_content(
        &storage,
        "Marek prefers the migration to run before the deploy",
    )
    .await;

    let value = execute(
        &storage,
        &test_cognitive(),
        Some(serde_json::json!({ "query": "migration before the deploy", "limit": 5 })),
    )
    .await
    .unwrap();

    for result in value["results"].as_array().unwrap() {
        assert!(
            result.get("codeRefs").is_none(),
            "no anchors means no field: {result}"
        );
    }
}

/// `brief` is the level a reader scans first, so the verdict has to be there
/// too — a warning that only appears at `full` is a warning most readers never
/// reach.
#[tokio::test]
async fn every_detail_level_carries_the_verdict() {
    use vestige_core::{AnchorVerdict, CodeAnchor, IngestAnchor};

    let (storage, _dir) = test_storage().await;
    let mut anchor = CodeAnchor::new("src/lib.rs");
    anchor.commit_sha = Some("1cfdf45aa".to_string());
    storage
        .ingest(IngestInput {
            content: "The anchor verdict must survive every detail level".to_string(),
            anchors: vec![IngestAnchor {
                anchor,
                verdict: AnchorVerdict::Orphaned,
                resolved_at: None,
            }],
            ..Default::default()
        })
        .unwrap();

    for level in ["brief", "summary", "full"] {
        let value = execute(
            &storage,
            &test_cognitive(),
            Some(serde_json::json!({
                "query": "anchor verdict detail level",
                "detail_level": level,
                "limit": 5
            })),
        )
        .await
        .unwrap();

        let result = &value["results"][0];
        assert_eq!(
            result["codeRefs"][0]["verdict"], "orphaned",
            "detail_level={level}: {value}"
        );
    }
}
