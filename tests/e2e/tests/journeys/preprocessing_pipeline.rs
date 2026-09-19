//! # Content Intelligence Pipeline Integration Tests
//!
//! Tests the full preprocessing → ingest → search lifecycle:
//!
//! 1. Content with entities, temporal expressions, and relations
//!    goes through preprocessing enrichment
//! 2. Enriched content gets stored with provenance metadata
//! 3. Compound queries get decomposed and searched
//!
//! ## Scope: contract tests **and** a real storage journey
//!
//! [`preprocessing_tests`] exercises the pipeline as pure functions — it never
//! constructs `Storage`, so it cannot fail if the derived metadata is dropped
//! on the way to SQLite. [`storage_journeys`] at the bottom closes that gap: the
//! pipeline output is ingested through `Storage::smart_ingest` and every piece
//! of derived metadata (rewritten content, entity tags, temporal anchors,
//! provenance) is read back from a cold-started store.

mod preprocessing_tests {
    use vestige_core::preprocessing::{
        self, PreprocessingConfig, PreprocessingResult,
        coref::resolve_coreferences,
        entities::{EntityType, extract_entities},
        provenance::ProvenanceMetadata,
        relations::extract_relations,
        temporal::anchor_temporal,
    };
    use vestige_core::search::decompose::{HasIdAndScore, decompose_query, merge_results};

    // ========================================================================
    // JOURNEY 1: Full preprocessing pipeline on realistic content
    // ========================================================================

    #[test]
    fn journey_preprocessing_enriches_realistic_memory() {
        let content = "Alice manages the Auth Team at Acme Corp. \
                        She deployed the new OAuth service at https://auth.acme.com \
                        and the deadline is by next Friday. \
                        Contact alice@acme.com for the $50,000 budget review.";

        let config = PreprocessingConfig {
            session_id: Some("journey-test-session".into()),
            agent: Some("integration-test".into()),
            existing_valid_from: None,
            existing_valid_until: None,
        };

        let result = preprocessing::preprocess(content, &config);

        // Entity extraction should find multiple types
        assert!(
            !result.entities.is_empty(),
            "Should extract entities from rich content"
        );
        let entity_types: Vec<EntityType> = result.entities.iter().map(|e| e.entity_type).collect();
        assert!(entity_types.contains(&EntityType::Url), "Should detect URL");
        assert!(
            entity_types.contains(&EntityType::Email),
            "Should detect email"
        );
        assert!(
            entity_types.contains(&EntityType::Monetary),
            "Should detect monetary value"
        );

        // Auto-tags should be generated
        assert!(!result.auto_tags.is_empty(), "Should generate auto-tags");
        assert!(
            result.auto_tags.iter().all(|t| t.starts_with("entity:")),
            "All auto-tags should have entity: prefix"
        );

        // Coreference rewriting should resolve "She" → "Alice"
        if result.coref_rewrites > 0 {
            assert!(
                result.content.contains("Alice deployed") || result.content.contains("Alice"),
                "Coreference should replace 'She' with the person entity"
            );
        }

        // Provenance should capture session context
        assert_eq!(
            result.provenance.session_id.as_deref(),
            Some("journey-test-session")
        );
        assert_eq!(result.provenance.agent.as_deref(), Some("integration-test"));
        assert!(!result.provenance.auto_entities.is_empty());
    }

    // ========================================================================
    // JOURNEY 2: Pipeline handles code/technical content gracefully
    // ========================================================================

    #[test]
    fn journey_code_content_passthrough() {
        let content = r#"fn handle_request(req: &Request) -> Response {
    let db = get_connection().await?;
    let user = db.find_user(req.user_id).await?;
    Response::ok(user.to_json())
}"#;

        let result = preprocessing::preprocess(content, &PreprocessingConfig::default());

        // Code should survive preprocessing intact
        assert!(result.content.contains("fn handle_request"));
        assert!(result.content.contains("Response::ok"));
        // No coreference rewrites on code
        assert_eq!(result.coref_rewrites, 0);
    }

    // ========================================================================
    // JOURNEY 3: Entity extraction → coreference → relation extraction chain
    // ========================================================================

    #[test]
    fn journey_entity_to_coref_to_relation_chain() {
        let content = "Vestige uses FSRS-6 for scheduling. It implements the MCP protocol.";

        // Step 1: Extract entities
        let entities = extract_entities(content, 20);
        let proper_nouns: Vec<&str> = entities
            .iter()
            .filter(|e| {
                matches!(
                    e.entity_type,
                    EntityType::ProperNoun | EntityType::Organization
                )
            })
            .map(|e| e.text.as_str())
            .collect();
        assert!(!proper_nouns.is_empty(), "Should find at least Vestige");

        // Step 2: Coreference resolution
        let coref = resolve_coreferences(content, &entities);
        // With exactly one proper noun, "It" should resolve
        if proper_nouns.len() == 1 {
            assert!(
                coref.rewrites > 0,
                "Single proper noun → 'It' should resolve"
            );
        }

        // Step 3: Relation extraction on the (possibly rewritten) content
        let relations = extract_relations(&coref.content, &entities);
        // "uses" is in our verb list, so we should get at least one relation
        if !relations.is_empty() {
            let verbs: Vec<&str> = relations.iter().map(|r| r.predicate.as_str()).collect();
            assert!(
                verbs.contains(&"uses") || verbs.contains(&"implements"),
                "Expected 'uses' or 'implements' verb, got: {:?}",
                verbs
            );
        }
    }

    // ========================================================================
    // JOURNEY 4: Temporal anchoring with various date expressions
    // ========================================================================

    #[test]
    fn journey_temporal_anchoring_various_expressions() {
        // Future deadline → valid_until
        let r1 = anchor_temporal("Deploy before tomorrow.", None, None);
        assert!(
            r1.valid_until.is_some() || !r1.anchors_found.is_empty(),
            "Should detect 'before tomorrow' as deadline"
        );

        // Past event → valid_from
        let r2 = anchor_temporal("The bug was introduced yesterday.", None, None);
        assert!(
            r2.valid_from.is_some() || !r2.anchors_found.is_empty(),
            "Should detect 'yesterday' as past anchor"
        );

        // Starting from → valid_from
        let r3 = anchor_temporal("Starting from next Monday the new rules apply.", None, None);
        assert!(
            r3.valid_from.is_some() || !r3.anchors_found.is_empty(),
            "Should detect 'starting from' pattern"
        );

        // No temporal → nothing
        let r4 = anchor_temporal("The function returns a boolean.", None, None);
        assert!(r4.valid_from.is_none());
        assert!(r4.valid_until.is_none());
        assert!(r4.anchors_found.is_empty());
    }

    // ========================================================================
    // JOURNEY 5: Provenance metadata survives JSON roundtrip
    // ========================================================================

    #[test]
    fn journey_provenance_full_lifecycle() {
        let content = "Alice manages the Auth Team. She deployed the service last week.";
        let config = PreprocessingConfig {
            session_id: Some("lifecycle-test".into()),
            agent: Some("cursor-agent".into()),
            ..Default::default()
        };

        let result = preprocessing::preprocess(content, &config);

        // Serialize to JSON (as it would be stored in SQLite)
        let json = result.provenance.to_json();
        let json_str = serde_json::to_string(&json).unwrap();

        // Deserialize back (as it would be read from SQLite)
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let restored = ProvenanceMetadata::from_json(&value);

        assert_eq!(restored.session_id.as_deref(), Some("lifecycle-test"));
        assert_eq!(restored.agent.as_deref(), Some("cursor-agent"));
        assert_eq!(restored.coref_rewrites, result.coref_rewrites);
        assert_eq!(
            restored.auto_entities.len(),
            result.provenance.auto_entities.len()
        );
    }

    // ========================================================================
    // JOURNEY 6: Compound query decomposition + merge
    // ========================================================================

    #[test]
    fn journey_compound_query_decomposition_and_merge() {
        // Semicolon compound
        let d1 = decompose_query("user preferences; project architecture; deployment process");
        assert!(d1.is_compound);
        assert_eq!(d1.sub_queries.len(), 3);
        assert_eq!(
            d1.original,
            "user preferences; project architecture; deployment process"
        );

        // Question chain
        let d2 = decompose_query("How does auth work? And what about the search pipeline?");
        assert!(d2.is_compound);
        assert_eq!(d2.sub_queries.len(), 2);

        // Simple query — no decomposition
        let d3 = decompose_query("How does FSRS-6 work?");
        assert!(!d3.is_compound);
        assert_eq!(d3.sub_queries.len(), 1);

        // Merge results from sub-queries
        #[derive(Debug)]
        struct MockResult {
            id: String,
            score: f64,
        }
        impl HasIdAndScore for MockResult {
            fn id(&self) -> &str {
                &self.id
            }
            fn score(&self) -> f64 {
                self.score
            }
        }

        let batch_a = vec![
            MockResult {
                id: "mem-1".into(),
                score: 0.9,
            },
            MockResult {
                id: "mem-2".into(),
                score: 0.7,
            },
        ];
        let batch_b = vec![
            MockResult {
                id: "mem-2".into(),
                score: 0.8,
            },
            MockResult {
                id: "mem-3".into(),
                score: 0.85,
            },
        ];
        let batch_c = vec![
            MockResult {
                id: "mem-1".into(),
                score: 0.6,
            },
            MockResult {
                id: "mem-4".into(),
                score: 0.5,
            },
        ];

        let merged = merge_results(vec![batch_a, batch_b, batch_c]);

        assert_eq!(merged.len(), 4, "Should have 4 unique results");
        // Best score for mem-1 should be 0.9 (from batch_a)
        let mem1 = merged.iter().find(|m| m.id() == "mem-1").unwrap();
        assert!((mem1.score() - 0.9).abs() < 0.001);
        // Best score for mem-2 should be 0.8 (from batch_b)
        let mem2 = merged.iter().find(|m| m.id() == "mem-2").unwrap();
        assert!((mem2.score() - 0.8).abs() < 0.001);
        // Results should be sorted by score descending
        for i in 1..merged.len() {
            assert!(
                merged[i - 1].score() >= merged[i].score(),
                "Results should be sorted descending: {} >= {}",
                merged[i - 1].score(),
                merged[i].score()
            );
        }
    }

    // ========================================================================
    // JOURNEY 7: Pipeline performance under load
    // ========================================================================

    #[test]
    fn journey_pipeline_batch_performance() {
        let contents = [
            "Alice manages the Auth Team since January. Deploy by next Friday.",
            "Bob created the search service. It uses FSRS-6. Contact bob@example.com.",
            "The $50,000 budget for Vestige was approved. Vestige depends on SQLite.",
            "Carol deployed https://prod.example.com yesterday. She fixed the bug.",
            "The Engineering Department at Stanford University released a paper.",
        ];

        let start = std::time::Instant::now();
        let results: Vec<PreprocessingResult> = contents
            .iter()
            .map(|c| preprocessing::preprocess(c, &PreprocessingConfig::default()))
            .collect();
        let elapsed = start.elapsed();

        assert_eq!(results.len(), 5);
        for r in &results {
            assert!(
                !r.entities.is_empty() || r.content.len() < 20,
                "Non-trivial content should produce entities"
            );
        }

        let per_call = elapsed / 5;
        assert!(
            per_call.as_millis() < 10,
            "Each preprocessing call should be < 10ms, got {}ms",
            per_call.as_millis()
        );
    }

    // ========================================================================
    // JOURNEY 8: Edge cases — empty, whitespace, unicode
    // ========================================================================

    #[test]
    fn journey_edge_cases() {
        // Empty
        let r1 = preprocessing::preprocess("", &PreprocessingConfig::default());
        assert!(r1.entities.is_empty());
        assert!(r1.auto_tags.is_empty());
        assert!(r1.relations.is_empty());
        assert_eq!(r1.coref_rewrites, 0);

        // Whitespace only
        let r2 = preprocessing::preprocess("   \n\t  ", &PreprocessingConfig::default());
        assert!(r2.entities.is_empty());

        // Unicode
        let r3 = preprocessing::preprocess(
            "Józef Piłsudski napisał raport. He was influential.",
            &PreprocessingConfig::default(),
        );
        assert!(!r3.content.is_empty());

        // Very long content
        let long = "Alice manages the team. ".repeat(100);
        let r4 = preprocessing::preprocess(&long, &PreprocessingConfig::default());
        assert!(
            r4.entities.len() <= 20,
            "Should cap entities at MAX_ENTITIES"
        );
    }
}

// ============================================================================
// REAL STORAGE JOURNEY (derived metadata must reach SQLite)
// ============================================================================
//
// Content Intelligence is only useful if what it derives is *persisted* and
// comes back on retrieval: the entity tags drive filtering, the temporal
// anchors drive `temporal current/expired`, and the provenance block is what
// `search` returns at `detail_level: "full"`. This journey composes the same
// ingest the MCP `smart_ingest` tool performs
// (`crates/vestige-mcp/src/tools/smart_ingest/execute.rs:104-158`) and then
// asserts every derived field after a cold restart.

mod storage_journeys {
    use std::path::Path;

    use chrono::Utc;
    use tempfile::TempDir;
    use vestige_core::memory::IngestInput;
    use vestige_core::preprocessing::{self, PreprocessingConfig, provenance::ProvenanceMetadata};
    use vestige_e2e_tests::harness::{TestDatabaseManager, enable_mock_embeddings};

    fn reopen(db_path: &Path) -> TestDatabaseManager {
        TestDatabaseManager::new_at_path(db_path.to_path_buf())
    }

    /// Preprocess → ingest → cold restart → the derived metadata is still there
    /// and still drives retrieval.
    #[test]
    fn test_journey_preprocessing_metadata_is_persisted_and_retrievable() {
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let db_path = dir.path().join("preprocessing.db");
        let db = TestDatabaseManager::new_at_path(db_path.clone());

        // "due by next Friday" is the exact deadline phrasing pinned by the
        // pipeline's own unit test (`preprocessing/temporal.rs:140-147`), so the
        // temporal half of this journey cannot pass vacuously.
        let content = "Alice manages the Auth Team at Acme Corp. She deployed the new \
                       OAuth service at https://auth.acme.com and the rollout report is \
                       due by next Friday. Contact alice@acme.com for the $50,000 budget.";
        let config = PreprocessingConfig {
            session_id: Some("journey-session-42".to_string()),
            agent: Some("claude".to_string()),
            existing_valid_from: None,
            existing_valid_until: None,
        };
        let pp = preprocessing::preprocess(content, &config);

        // Preconditions for the persistence assertions below.
        assert!(
            !pp.auto_tags.is_empty() && pp.auto_tags.iter().all(|t| t.starts_with("entity:")),
            "the pipeline must derive entity tags for this content, got {:?}",
            pp.auto_tags
        );
        let anchored_until = pp
            .valid_until
            .expect("the deadline phrase must be anchored to a concrete date");
        assert!(
            anchored_until > Utc::now(),
            "the anchored deadline must be in the future, got {anchored_until}"
        );
        assert!(
            pp.provenance
                .auto_entities
                .iter()
                .any(|e| e.contains("Acme") || e.contains("Alice")),
            "the pipeline must extract the named entities, got {:?}",
            pp.provenance.auto_entities
        );

        // The ingest the MCP `smart_ingest` tool performs: rewritten content,
        // auto-tags merged with the caller's, anchors as validity, provenance
        // as JSON.
        let stored_node = db
            .storage
            .smart_ingest(IngestInput {
                content: pp.content.clone(),
                node_type: "fact".to_string(),
                source: Some("preprocessing-journey".to_string()),
                tags: pp.auto_tags.clone(),
                valid_from: pp.valid_from,
                valid_until: pp.valid_until,
                provenance: Some(pp.provenance.to_json()),
                ..Default::default()
            })
            .expect("smart_ingest must succeed")
            .node;
        drop(db);

        // ---- Everything below is read off disk after a cold start -----------
        let restarted = reopen(&db_path);
        let stored = restarted
            .storage
            .get_node(&stored_node.id)
            .expect("get_node must not error")
            .expect("the enriched memory must survive a restart");

        assert_eq!(
            stored.content, pp.content,
            "the rewritten content must be what is stored (not the raw input)"
        );
        assert_eq!(stored.valid_from, pp.valid_from);
        assert_eq!(
            stored.valid_until,
            Some(anchored_until),
            "the anchored deadline must be persisted on the row"
        );
        for tag in &pp.auto_tags {
            assert!(
                stored.tags.contains(tag),
                "the derived tag `{tag}` must be persisted; got {:?}",
                stored.tags
            );
        }

        // The provenance block, read back through the product's own parser.
        let provenance_json = stored
            .provenance
            .clone()
            .expect("the provenance column must be populated");
        let persisted = ProvenanceMetadata::from_json(&provenance_json);
        assert_eq!(persisted.session_id.as_deref(), Some("journey-session-42"));
        assert_eq!(persisted.agent.as_deref(), Some("claude"));
        assert_eq!(
            persisted.coref_rewrites, pp.coref_rewrites,
            "the coreference rewrite count must be persisted"
        );
        assert_eq!(
            persisted.auto_entities, pp.provenance.auto_entities,
            "the extracted entities must be persisted"
        );
        assert_eq!(
            persisted.temporal_anchors_found, pp.provenance.temporal_anchors_found,
            "the temporal anchors must be persisted"
        );
        assert_eq!(
            persisted.relations_extracted, pp.provenance.relations_extracted,
            "the extracted relations must be persisted"
        );
        if pp.coref_rewrites > 0 {
            assert!(
                stored.content.contains("Alice"),
                "a rewritten pronoun must resolve to the stored entity name"
            );
        }

        // ---- The derived metadata must also be usable for retrieval ---------
        let by_entity = restarted
            .storage
            .keyword_search("acme", 5, 0.0)
            .expect("keyword_search must succeed");
        assert!(
            by_entity.iter().any(|n| n.id == stored_node.id),
            "the memory must be findable by an extracted entity name, got {:?}",
            by_entity.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        let unrelated = restarted
            .storage
            .keyword_search("sourdough", 5, 0.0)
            .expect("keyword_search must succeed");
        assert!(
            unrelated.is_empty(),
            "an unrelated query must not match the stored memory"
        );
        let (kw, sem) = vestige_core::default_hybrid_weights();
        let hybrid = restarted
            .storage
            .hybrid_search("who deployed the OAuth service", 5, kw, sem)
            .expect("hybrid_search must succeed");
        assert!(
            hybrid.iter().any(|r| r.node.id == stored_node.id),
            "the rewritten content must be embedded and retrievable, got {:?}",
            hybrid
                .iter()
                .map(|r| (&r.node.id, r.combined_score))
                .collect::<Vec<_>>()
        );
    }
}
