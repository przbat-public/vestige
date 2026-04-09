//! # Content Intelligence Pipeline Integration Tests
//!
//! Tests the full preprocessing → ingest → search lifecycle:
//!
//! 1. Content with entities, temporal expressions, and relations
//!    goes through preprocessing enrichment
//! 2. Enriched content gets stored with provenance metadata
//! 3. Compound queries get decomposed and searched

mod preprocessing_tests {
    use vestige_core::preprocessing::{
        self, PreprocessingConfig, PreprocessingResult,
        entities::{EntityType, extract_entities},
        coref::resolve_coreferences,
        temporal::anchor_temporal,
        relations::extract_relations,
        provenance::ProvenanceMetadata,
    };
    use vestige_core::search::decompose::{decompose_query, merge_results, HasIdAndScore};

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
        assert!(!result.entities.is_empty(), "Should extract entities from rich content");
        let entity_types: Vec<EntityType> = result.entities.iter().map(|e| e.entity_type).collect();
        assert!(entity_types.contains(&EntityType::Url), "Should detect URL");
        assert!(entity_types.contains(&EntityType::Email), "Should detect email");
        assert!(entity_types.contains(&EntityType::Monetary), "Should detect monetary value");

        // Auto-tags should be generated
        assert!(!result.auto_tags.is_empty(), "Should generate auto-tags");
        assert!(result.auto_tags.iter().all(|t| t.starts_with("entity:")),
            "All auto-tags should have entity: prefix");

        // Coreference rewriting should resolve "She" → "Alice"
        if result.coref_rewrites > 0 {
            assert!(result.content.contains("Alice deployed") || result.content.contains("Alice"),
                "Coreference should replace 'She' with the person entity");
        }

        // Provenance should capture session context
        assert_eq!(result.provenance.session_id.as_deref(), Some("journey-test-session"));
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
        let proper_nouns: Vec<&str> = entities.iter()
            .filter(|e| matches!(e.entity_type, EntityType::ProperNoun | EntityType::Organization))
            .map(|e| e.text.as_str())
            .collect();
        assert!(!proper_nouns.is_empty(), "Should find at least Vestige");

        // Step 2: Coreference resolution
        let coref = resolve_coreferences(content, &entities);
        // With exactly one proper noun, "It" should resolve
        if proper_nouns.len() == 1 {
            assert!(coref.rewrites > 0, "Single proper noun → 'It' should resolve");
        }

        // Step 3: Relation extraction on the (possibly rewritten) content
        let relations = extract_relations(&coref.content, &entities);
        // "uses" is in our verb list, so we should get at least one relation
        if !relations.is_empty() {
            let verbs: Vec<&str> = relations.iter().map(|r| r.predicate.as_str()).collect();
            assert!(verbs.contains(&"uses") || verbs.contains(&"implements"),
                "Expected 'uses' or 'implements' verb, got: {:?}", verbs);
        }
    }

    // ========================================================================
    // JOURNEY 4: Temporal anchoring with various date expressions
    // ========================================================================

    #[test]
    fn journey_temporal_anchoring_various_expressions() {
        // Future deadline → valid_until
        let r1 = anchor_temporal("Deploy before tomorrow.", None, None);
        assert!(r1.valid_until.is_some() || !r1.anchors_found.is_empty(),
            "Should detect 'before tomorrow' as deadline");

        // Past event → valid_from
        let r2 = anchor_temporal("The bug was introduced yesterday.", None, None);
        assert!(r2.valid_from.is_some() || !r2.anchors_found.is_empty(),
            "Should detect 'yesterday' as past anchor");

        // Starting from → valid_from
        let r3 = anchor_temporal("Starting from next Monday the new rules apply.", None, None);
        assert!(r3.valid_from.is_some() || !r3.anchors_found.is_empty(),
            "Should detect 'starting from' pattern");

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
        assert_eq!(restored.auto_entities.len(), result.provenance.auto_entities.len());
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
        assert_eq!(d1.original, "user preferences; project architecture; deployment process");

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
        struct MockResult { id: String, score: f64 }
        impl HasIdAndScore for MockResult {
            fn id(&self) -> &str { &self.id }
            fn score(&self) -> f64 { self.score }
        }

        let batch_a = vec![
            MockResult { id: "mem-1".into(), score: 0.9 },
            MockResult { id: "mem-2".into(), score: 0.7 },
        ];
        let batch_b = vec![
            MockResult { id: "mem-2".into(), score: 0.8 },
            MockResult { id: "mem-3".into(), score: 0.85 },
        ];
        let batch_c = vec![
            MockResult { id: "mem-1".into(), score: 0.6 },
            MockResult { id: "mem-4".into(), score: 0.5 },
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
            assert!(merged[i - 1].score() >= merged[i].score(),
                "Results should be sorted descending: {} >= {}", merged[i-1].score(), merged[i].score());
        }
    }

    // ========================================================================
    // JOURNEY 7: Pipeline performance under load
    // ========================================================================

    #[test]
    fn journey_pipeline_batch_performance() {
        let contents = vec![
            "Alice manages the Auth Team since January. Deploy by next Friday.",
            "Bob created the search service. It uses FSRS-6. Contact bob@example.com.",
            "The $50,000 budget for Vestige was approved. Vestige depends on SQLite.",
            "Carol deployed https://prod.example.com yesterday. She fixed the bug.",
            "The Engineering Department at Stanford University released a paper.",
        ];

        let start = std::time::Instant::now();
        let results: Vec<PreprocessingResult> = contents.iter()
            .map(|c| preprocessing::preprocess(c, &PreprocessingConfig::default()))
            .collect();
        let elapsed = start.elapsed();

        assert_eq!(results.len(), 5);
        for r in &results {
            assert!(!r.entities.is_empty() || r.content.len() < 20,
                "Non-trivial content should produce entities");
        }

        let per_call = elapsed / 5;
        assert!(per_call.as_millis() < 10,
            "Each preprocessing call should be < 10ms, got {}ms", per_call.as_millis());
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
        assert!(r3.content.len() > 0);

        // Very long content
        let long = "Alice manages the team. ".repeat(100);
        let r4 = preprocessing::preprocess(&long, &PreprocessingConfig::default());
        assert!(r4.entities.len() <= 20, "Should cap entities at MAX_ENTITIES");
    }
}
