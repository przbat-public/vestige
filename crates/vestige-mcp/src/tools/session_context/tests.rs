#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Mutex;

use vestige_core::{IngestInput, Storage};

use crate::cognitive::CognitiveEngine;

use super::execute::execute;
use super::helpers::first_sentence;
use super::schema::schema;

fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
    Arc::new(Mutex::new(CognitiveEngine::new()))
}

async fn test_storage() -> (Arc<Storage>, TempDir) {
    let dir = TempDir::new().unwrap();
    let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
    (Arc::new(storage), dir)
}

    async fn ingest_test_content(storage: &Arc<Storage>, content: &str, tags: Vec<&str>) -> String {
        let input = IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            source: None,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            valid_from: None,
            valid_until: None,
            provenance: None,
            ..Default::default()
        };
        let node = storage.ingest(input).unwrap();
        node.id
    }

    // ========================================================================
    // SCHEMA TESTS
    // ========================================================================

    #[test]
    fn test_schema_has_properties() {
        let s = schema();
        assert_eq!(s["type"], "object");
        assert!(s["properties"]["queries"].is_object());
        assert!(s["properties"]["token_budget"].is_object());
        assert!(s["properties"]["context"].is_object());
        assert!(s["properties"]["include_status"].is_object());
        assert!(s["properties"]["include_intentions"].is_object());
        assert!(s["properties"]["include_predictions"].is_object());
    }

    #[test]
    fn test_schema_token_budget_bounds() {
        let s = schema();
        let tb = &s["properties"]["token_budget"];
        assert_eq!(tb["minimum"], 100);
        assert_eq!(tb["maximum"], 100000);
        assert_eq!(tb["default"], 1000);
    }

    // ========================================================================
    // EXECUTE TESTS
    // ========================================================================

    #[tokio::test]
    async fn test_default_no_args() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value["context"].is_string());
        assert!(value["tokensUsed"].is_number());
        assert!(value["tokenBudget"].is_number());
        assert_eq!(value["tokenBudget"], 1000);
        assert!(value["expandable"].is_array());
        assert!(value["automationTriggers"].is_object());
    }

    #[tokio::test]
    async fn test_with_queries() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(
            &storage,
            "Sam prefers Rust and TypeScript for all projects.",
            vec![],
        )
        .await;

        let args = serde_json::json!({
            "queries": ["Sam preferences", "project context"]
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let ctx = value["context"].as_str().unwrap();
        assert!(ctx.contains("Session"));
    }

    #[tokio::test]
    async fn test_token_budget_respected() {
        let (storage, _dir) = test_storage().await;
        // Ingest several memories to generate content
        for i in 0..20 {
            ingest_test_content(
                &storage,
                &format!(
                    "Memory number {} contains detailed information about topic {} that is quite long and verbose to fill up the token budget.",
                    i, i
                ),
                vec![],
            )
            .await;
        }

        let args = serde_json::json!({
            "queries": ["memory"],
            "token_budget": 200
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let _ctx = value["context"].as_str().unwrap();
        // Context should be within budget (200 tokens * 4 = 800 chars + header overhead)
        // The actual char count of context should be reasonable
        let tokens_used = value["tokensUsed"].as_u64().unwrap();
        // Allow some overhead for the header
        assert!(
            tokens_used <= 300,
            "tokens_used {} should be near budget 200",
            tokens_used
        );
    }

    #[tokio::test]
    async fn test_expandable_ids() {
        let (storage, _dir) = test_storage().await;
        // Ingest many memories
        for i in 0..20 {
            ingest_test_content(
                &storage,
                &format!(
                    "Expandable test memory {} with enough content to take up space in the token budget allocation.",
                    i
                ),
                vec![],
            )
            .await;
        }

        let args = serde_json::json!({
            "queries": ["expandable test memory"],
            "token_budget": 150
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        // expandable should be a valid array (may be empty if all fit within budget)
        assert!(value["expandable"].is_array());
    }

    #[tokio::test]
    async fn test_automation_triggers_booleans() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, &test_cognitive(), None).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let triggers = &value["automationTriggers"];
        assert!(triggers["needsDream"].is_boolean());
        assert!(triggers["needsBackup"].is_boolean());
        assert!(triggers["needsGc"].is_boolean());
    }

    #[tokio::test]
    async fn test_disable_sections() {
        let (storage, _dir) = test_storage().await;
        ingest_test_content(&storage, "Test memory for disable sections.", vec![]).await;

        let args = serde_json::json!({
            "include_status": false,
            "include_intentions": false,
            "include_predictions": false
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let context_str = value["context"].as_str().unwrap();
        // Should NOT contain status line when disabled
        assert!(!context_str.contains("**Status:**"));
        // automationTriggers should still be present (always computed)
        assert!(value["automationTriggers"].is_object());
    }

    #[tokio::test]
    async fn test_with_codebase_context() {
        let (storage, _dir) = test_storage().await;
        // Ingest a pattern with codebase tag
        let input = IngestInput {
            content: "Code pattern: Use Arc<Mutex<>> for shared state in async contexts."
                .to_string(),
            node_type: "pattern".to_string(),
            source: None,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: vec!["pattern".to_string(), "codebase:vestige".to_string()],
            valid_from: None,
            valid_until: None,
            provenance: None,
            ..Default::default()
        };
        storage.ingest(input).unwrap();

        let args = serde_json::json!({
            "context": {
                "codebase": "vestige",
                "topics": ["performance"]
            }
        });
        let result = execute(&storage, &test_cognitive(), Some(args)).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        let ctx = value["context"].as_str().unwrap();
        // Should contain codebase section
        assert!(ctx.contains("vestige"));
    }

    // ========================================================================
    // HELPER TESTS
    // ========================================================================

    #[test]
    fn test_first_sentence_period() {
        assert_eq!(
            first_sentence("Hello world. More text here."),
            "Hello world."
        );
    }

    #[test]
    fn test_first_sentence_newline() {
        assert_eq!(first_sentence("First line\nSecond line"), "First line");
    }

    #[test]
    fn test_first_sentence_short() {
        assert_eq!(first_sentence("Short"), "Short");
    }

    #[test]
    fn test_first_sentence_long_truncated() {
        let long = "A".repeat(200);
        let result = first_sentence(&long);
        assert!(result.len() <= 150);
    }

    #[test]
    fn test_first_sentence_empty() {
        assert_eq!(first_sentence(""), "");
    }

    #[test]
    fn test_first_sentence_whitespace() {
        assert_eq!(first_sentence("  Hello world.  "), "Hello world.");
    }
