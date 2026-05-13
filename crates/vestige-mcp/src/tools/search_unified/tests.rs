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
        let node_id =
            ingest_test_content(&storage, "Python is a dynamic programming language.").await;

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
