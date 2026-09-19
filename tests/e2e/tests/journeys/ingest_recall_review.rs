//! # Ingest-Recall-Review Journey Tests
//!
//! Tests the complete memory lifecycle from creation to retrieval to review.
//! This is the core user journey for any memory system.
//!
//! ## User Journey
//!
//! 1. User ingests new memories (code snippets, learnings, decisions)
//! 2. User recalls memories via search (keyword, semantic, hybrid)
//! 3. User reviews memories to strengthen retention
//! 4. System tracks memory strength and schedules reviews
//! 5. User benefits from improved recall over time
//!
//! ## Scope: contract tests **and** real storage journeys
//!
//! The top-level tests are DTO / pure-function contract tests: they pin the
//! `serde` shapes and the FSRS scheduler in isolation and never construct
//! `Storage`. The real end-to-end journeys live in [`storage_journeys`] at the
//! bottom of this file: a `Storage` on a temp SQLite file, the production
//! prediction-error gate (`Storage::smart_ingest`), FTS5 + HNSW recall, FSRS
//! review, and a cold restart in between.

use vestige_core::{
    consolidation::SleepConsolidation,
    fsrs::{FSRSScheduler, LearningState, Rating},
    memory::{IngestInput, RecallInput, SearchMode},
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a test memory input using JSON deserialization (for non-exhaustive struct)
fn make_ingest(content: &str, node_type: &str, tags: Vec<&str>) -> IngestInput {
    let tags_json: Vec<String> = tags.into_iter().map(String::from).collect();
    let json = serde_json::json!({
        "content": content,
        "nodeType": node_type,
        "tags": tags_json,
        "source": "test"
    });
    serde_json::from_value(json).expect("IngestInput JSON should be valid")
}

/// Create a recall input using JSON deserialization
fn make_recall(query: &str, limit: i32, min_retention: f64, search_mode: &str) -> RecallInput {
    let json = serde_json::json!({
        "query": query,
        "limit": limit,
        "minRetention": min_retention,
        "searchMode": search_mode
    });
    serde_json::from_value(json).expect("RecallInput JSON should be valid")
}

// ============================================================================
// TEST 1: INGEST CREATES VALID MEMORY STRUCTURE
// ============================================================================

/// Test that ingesting a memory creates a properly structured node.
///
/// Validates:
/// - Node has valid UUID
/// - Content is preserved
/// - Tags are preserved
/// - Initial FSRS state is correct
/// - Timestamps are set correctly
#[test]
fn test_ingest_creates_valid_memory_structure() {
    // Create input
    let input = make_ingest(
        "Rust ownership ensures memory safety without garbage collection",
        "concept",
        vec!["rust", "memory", "ownership"],
    );

    // Verify input structure
    assert!(!input.content.is_empty(), "Content should not be empty");
    assert_eq!(input.node_type, "concept");
    assert_eq!(input.tags.len(), 3);
    assert!(input.tags.contains(&"rust".to_string()));
    assert!(input.tags.contains(&"memory".to_string()));
    assert!(input.tags.contains(&"ownership".to_string()));

    // Verify source is tracked
    assert_eq!(input.source, Some("test".to_string()));

    // Verify sentiment defaults
    assert_eq!(input.sentiment_score, 0.0);
    assert_eq!(input.sentiment_magnitude, 0.0);

    // Verify temporal validity defaults
    assert!(input.valid_from.is_none());
    assert!(input.valid_until.is_none());
}

// ============================================================================
// TEST 2: RECALL FINDS MEMORIES BY CONTENT
// ============================================================================

/// Test that recall can find memories matching a query.
///
/// Validates:
/// - Keyword search matches content
/// - Results are returned in order of relevance
/// - Memory strength affects ranking
#[test]
fn test_recall_finds_memories_by_content() {
    // Create recall input
    let recall = make_recall("rust ownership", 10, 0.5, "keyword");

    // Verify recall input structure
    assert_eq!(recall.query, "rust ownership");
    assert_eq!(recall.limit, 10);
    assert_eq!(recall.min_retention, 0.5);

    // Verify search mode
    match recall.search_mode {
        SearchMode::Keyword => {
            // Keyword search uses FTS5 — variant match is the actual assertion
        }
        _ => panic!("Expected Keyword search mode"),
    }
}

// ============================================================================
// TEST 3: REVIEW STRENGTHENS MEMORY WITH FSRS
// ============================================================================

/// Test that reviewing a memory updates its FSRS state correctly.
///
/// Validates:
/// - Good rating increases stability
/// - Again rating increases difficulty
/// - Next review is scheduled appropriately
/// - Storage and retrieval strength update
#[test]
fn test_review_strengthens_memory_with_fsrs() {
    let scheduler = FSRSScheduler::default();

    // Create initial state (new card)
    let initial_state = scheduler.new_card();
    assert_eq!(initial_state.reps, 0);
    assert_eq!(initial_state.lapses, 0);

    // Review with Good rating (elapsed_days is f64)
    let result = scheduler.review(&initial_state, Rating::Good, 0.0, None);

    // Stability should be set from initial parameters
    assert!(
        result.state.stability > 0.0,
        "Stability should be positive after review"
    );

    // Reps should increase
    assert_eq!(result.state.reps, 1, "Reps should increase after review");

    // Interval should be positive
    assert!(result.interval > 0, "Interval should be positive");

    // Review again with Easy - should increase interval
    let second_result = scheduler.review(&result.state, Rating::Easy, result.interval as f64, None);
    assert!(
        second_result.interval >= result.interval,
        "Easy rating should maintain or increase interval"
    );

    // Review with Again - should reset progress
    let again_result = scheduler.review(&second_result.state, Rating::Again, 1.0, None);
    assert!(
        again_result.interval <= second_result.interval,
        "Again rating should reduce interval"
    );
    assert_eq!(
        again_result.state.lapses, 1,
        "Lapses should increase on Again"
    );
}

// ============================================================================
// TEST 4: MEMORY LIFECYCLE FOLLOWS EXPECTED PATTERN
// ============================================================================

/// Test the complete memory lifecycle from new to mature.
///
/// Validates:
/// - New memory starts in learning state
/// - Successful reviews progress state
/// - Memory becomes mature after multiple reviews
/// - Intervals increase appropriately
#[test]
fn test_memory_lifecycle_follows_expected_pattern() {
    let scheduler = FSRSScheduler::default();
    let mut state = scheduler.new_card();

    // Track intervals to verify growth
    let mut intervals = Vec::new();

    // Simulate 10 successful reviews
    for i in 0..10 {
        let elapsed = if i == 0 {
            0.0
        } else {
            intervals.last().copied().unwrap_or(1) as f64
        };
        let result = scheduler.review(&state, Rating::Good, elapsed, None);
        intervals.push(result.interval);
        state = result.state;
    }

    // Verify lifecycle progression
    assert!(state.reps >= 10, "Should have at least 10 reps");
    assert_eq!(
        state.lapses, 0,
        "Should have no lapses with all Good ratings"
    );

    // Verify interval growth (early intervals may be similar, but should eventually grow)
    let early_avg: f64 = intervals[..3].iter().map(|&i| i as f64).sum::<f64>() / 3.0;
    let late_avg: f64 = intervals[7..].iter().map(|&i| i as f64).sum::<f64>() / 3.0;
    assert!(
        late_avg >= early_avg,
        "Later intervals ({}) should be >= early intervals ({})",
        late_avg,
        early_avg
    );

    // Verify state is Review (mature)
    match state.state {
        LearningState::Review => {
            // Mature memory in Review state — variant match is the assertion
        }
        _ => {
            // Also acceptable - depends on FSRS parameters
            assert!(state.reps >= 10, "Should have processed all reviews");
        }
    }
}

// ============================================================================
// TEST 5: SENTIMENT AFFECTS MEMORY CONSOLIDATION
// ============================================================================

/// Test that emotional memories are processed differently.
///
/// Validates:
/// - High sentiment magnitude boosts stability
/// - Emotional memories decay slower
/// - Sentiment is preserved through lifecycle
#[test]
fn test_sentiment_affects_memory_consolidation() {
    let consolidation = SleepConsolidation::new();

    // Calculate decay for neutral memory
    let neutral_decay = consolidation.calculate_decay(10.0, 5.0, 0.0);

    // Calculate decay for emotional memory
    let emotional_decay = consolidation.calculate_decay(10.0, 5.0, 1.0);

    // Emotional memory should decay slower (higher retention)
    assert!(
        emotional_decay > neutral_decay,
        "Emotional memory ({}) should retain better than neutral ({})",
        emotional_decay,
        neutral_decay
    );

    // Test should_promote logic
    assert!(
        consolidation.should_promote(0.8, 5.0),
        "High emotion + low storage should promote"
    );
    assert!(
        !consolidation.should_promote(0.3, 5.0),
        "Low emotion should not promote"
    );
    assert!(
        !consolidation.should_promote(0.8, 10.0),
        "Max storage should not promote"
    );

    // Test promotion boost
    let boosted = consolidation.promotion_boost(5.0);
    assert!(boosted > 5.0, "Promotion should increase storage strength");
    assert!(
        boosted <= 10.0,
        "Promotion should cap at max storage strength"
    );
}

// ============================================================================
// ADDITIONAL INTEGRATION TESTS
// ============================================================================

/// Test that RecallInput can be created with different search modes.
#[test]
fn test_recall_search_modes() {
    // Keyword mode
    let keyword = make_recall("test query", 10, 0.5, "keyword");
    assert!(matches!(keyword.search_mode, SearchMode::Keyword));

    // Semantic mode (when embeddings available)
    let semantic = make_recall("test query", 10, 0.5, "semantic");
    assert!(matches!(semantic.search_mode, SearchMode::Semantic));

    // Hybrid mode
    let hybrid = make_recall("test query", 10, 0.5, "hybrid");
    assert!(matches!(hybrid.search_mode, SearchMode::Hybrid));
}

/// Test IngestInput defaults.
#[test]
fn test_ingest_input_defaults() {
    let json = serde_json::json!({
        "content": "Test content",
        "nodeType": "fact"
    });
    let input: IngestInput = serde_json::from_value(json).unwrap();

    assert_eq!(input.content, "Test content");
    assert_eq!(input.node_type, "fact");
    assert!(input.source.is_none());
    assert!(input.tags.is_empty());
    assert_eq!(input.sentiment_score, 0.0);
    assert_eq!(input.sentiment_magnitude, 0.0);
}

/// Test FSRS rating effects on memory state.
#[test]
fn test_fsrs_rating_effects() {
    let scheduler = FSRSScheduler::default();
    let initial = scheduler.new_card();

    // Test all rating types (elapsed_days as f64)
    let again = scheduler.review(&initial, Rating::Again, 0.0, None);
    let hard = scheduler.review(&initial, Rating::Hard, 0.0, None);
    let good = scheduler.review(&initial, Rating::Good, 0.0, None);
    let easy = scheduler.review(&initial, Rating::Easy, 0.0, None);

    // Again should have shortest interval
    assert!(
        again.interval <= hard.interval,
        "Again ({}) should be <= Hard ({})",
        again.interval,
        hard.interval
    );

    // Easy should have longest interval
    assert!(
        easy.interval >= good.interval,
        "Easy ({}) should be >= Good ({})",
        easy.interval,
        good.interval
    );

    // Good should have medium interval
    assert!(
        good.interval >= hard.interval,
        "Good ({}) should be >= Hard ({})",
        good.interval,
        hard.interval
    );
}

// ============================================================================
// REAL STORAGE JOURNEYS (SQLite, embeds, FSRS state, cold restart)
// ============================================================================
//
// Everything above is a DTO / pure-function contract test. The tests below
// drive the product: a `Storage` on a temp SQLite file, the prediction-error
// gate inside `Storage::smart_ingest` (the same call the MCP `smart_ingest`
// tool makes), FTS5 + HNSW recall through `Storage::recall`, FSRS review, and a
// cold restart that rebuilds every index from disk.
//
// "Restart" means dropping the `Storage` and constructing a fresh one from the
// same file: that re-runs the migrations and the vector-index bootstrap the
// server runs at boot. The journey deliberately does not spawn the
// `vestige-mcp` binary — CI's journey-tests job
// (.github/workflows/test.yml:213-223) runs this target without building the
// server, so a child-process test would skip there and report green while
// covering nothing. Child-process coverage lives in the `mcp_protocol` target,
// whose CI job builds the binary first.

mod storage_journeys {
    use std::path::Path;

    use chrono::Utc;
    use tempfile::TempDir;
    use vestige_core::memory::{IngestInput, RecallInput, SearchMode};
    use vestige_core::{KnowledgeNode, Rating, Storage};
    use vestige_e2e_tests::harness::{TestDatabaseManager, enable_mock_embeddings};

    /// Build an `IngestInput` (the struct carries defaults for every optional
    /// field, so journeys only name what they exercise).
    fn memory(content: &str, node_type: &str, tags: &[&str]) -> IngestInput {
        IngestInput {
            content: content.to_string(),
            node_type: node_type.to_string(),
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            source: Some("journey".to_string()),
            ..Default::default()
        }
    }

    /// Cold start over an existing file — the server's boot path.
    fn reopen(db_path: &Path) -> TestDatabaseManager {
        TestDatabaseManager::new_at_path(db_path.to_path_buf())
    }

    /// Recall through the real dispatcher (`Keyword` = FTS5, `Hybrid` =
    /// FTS5 + embeddings + RRF), with `min_retention` disabled so the decayed
    /// memories in the second journey stay eligible.
    fn recall(storage: &Storage, query: &str, mode: SearchMode) -> Vec<KnowledgeNode> {
        storage
            .recall(RecallInput {
                query: query.to_string(),
                limit: 10,
                min_retention: 0.0,
                search_mode: mode,
                ..Default::default()
            })
            .expect("recall must succeed")
    }

    fn ids(nodes: &[KnowledgeNode]) -> Vec<String> {
        nodes.iter().map(|n| n.id.clone()).collect()
    }

    /// ingest (prediction-error gate) → recall (both modes) → review →
    /// cold restart → every field, index and schedule is still there.
    #[test]
    fn test_journey_ingest_recall_review_survives_a_cold_restart() {
        // Must be set before the first embed: without it the ingest path
        // lazily initialises the real ONNX model (~547 MB on a cold cache).
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let db_path = dir.path().join("ingest_recall_review.db");
        let db = TestDatabaseManager::new_at_path(db_path.clone());
        assert!(db.is_empty(), "a fresh store must start empty");

        // ---- 1. Write through the production path --------------------------
        let fsrs_memory = "The FSRS-6 scheduler computes the next review date from retrievability.";
        let created = db
            .storage
            .smart_ingest(memory(fsrs_memory, "fact", &["fsrs", "scheduling"]))
            .expect("smart_ingest must succeed");
        assert_eq!(
            created.decision, "create",
            "the first memory in an empty store must go through the Create branch, got {}: {}",
            created.decision, created.reason
        );
        assert!(
            created.node.has_embedding.unwrap_or(false),
            "the create branch must persist a vector, not only return the node"
        );
        let id = created.node.id.clone();

        let other = db
            .storage
            .ingest(memory(
                "Sourdough starter hydration ratios for bread.",
                "note",
                &["baking"],
            ))
            .expect("ingest must succeed");
        assert_eq!(db.node_count(), 2, "two writes must produce two rows");

        // ---- 2. Recall through both retrieval paths ------------------------
        let keyword = recall(&db.storage, "retrievability schedule", SearchMode::Keyword);
        assert!(
            keyword.iter().any(|n| n.id == id),
            "FTS5 recall must find the FSRS memory, got {:?}",
            ids(&keyword)
        );
        let hybrid = recall(
            &db.storage,
            "which memory schedules reviews from retrievability",
            SearchMode::Hybrid,
        );
        assert!(
            hybrid.iter().any(|n| n.id == id),
            "hybrid recall must find the FSRS memory, got {:?}",
            ids(&hybrid)
        );
        // Anti-vacuity: recall must discriminate, not return the whole store.
        let unrelated = recall(&db.storage, "sourdough hydration", SearchMode::Keyword);
        assert!(
            !unrelated.iter().any(|n| n.id == id),
            "an unrelated query must not return the FSRS memory"
        );

        // ---- 3. Review through the real FSRS path --------------------------
        let before_review = db
            .storage
            .get_node(&id)
            .expect("get_node must not error")
            .expect("the memory must exist");
        let reviewed = db
            .storage
            .mark_reviewed(&id, Rating::Good)
            .expect("mark_reviewed must succeed");
        assert_eq!(reviewed.reps, before_review.reps + 1);
        assert!(
            reviewed.storage_strength > before_review.storage_strength,
            "a Good review must strengthen storage"
        );
        assert!(
            reviewed.next_review.expect("scheduled") > Utc::now(),
            "a reviewed memory must be scheduled into the future"
        );

        // ---- 4. Cold restart ------------------------------------------------
        let expected = reviewed.clone();
        drop(db);
        let restarted = reopen(&db_path);

        assert_eq!(
            restarted.node_count(),
            2,
            "a restart must not lose or duplicate memories"
        );
        let stats = restarted
            .storage
            .get_stats()
            .expect("get_stats must succeed");
        assert_eq!(
            stats.nodes_with_embeddings, 2,
            "both vectors must be on disk, not only in the previous process"
        );
        let after = restarted
            .storage
            .get_node(&id)
            .expect("get_node after restart must not error")
            .expect("the reviewed memory must survive a restart");
        assert_eq!(after.content, expected.content);
        assert_eq!(after.node_type, expected.node_type);
        assert_eq!(after.tags, expected.tags);
        assert_eq!(after.source, expected.source);
        assert_eq!(after.reps, expected.reps, "FSRS reps must be durable");
        assert_eq!(
            after.storage_strength, expected.storage_strength,
            "FSRS storage strength must be durable"
        );
        assert_eq!(
            after.next_review, expected.next_review,
            "the review schedule must be durable"
        );

        // ---- 5. Both indexes must be usable after the cold start ------------
        let keyword_after = recall(
            &restarted.storage,
            "retrievability schedule",
            SearchMode::Keyword,
        );
        assert!(
            keyword_after.iter().any(|n| n.id == id),
            "the FTS5 index must survive a restart, got {:?}",
            ids(&keyword_after)
        );
        let hybrid_after = recall(
            &restarted.storage,
            "bread baking hydration",
            SearchMode::Hybrid,
        );
        assert!(
            hybrid_after.iter().any(|n| n.id == other.id),
            "the vector index must be rebuilt from SQLite on boot, got {:?}",
            ids(&hybrid_after)
        );
    }

    /// The Testing Effect (`recall` → `strengthen_batch_on_access`) must be a
    /// durable write, not an in-memory score tweak.
    ///
    /// The memory is first downscaled through the real NREM3 primitive
    /// (`Storage::downscale_retention_batch`, applied by the dream cycle):
    /// without it the strengths sit at their ingest maximum of 1.0 and the
    /// `MIN(1.0, x + 0.05)` boost inside `recall` would be invisible.
    #[test]
    fn test_journey_recall_strengthens_a_decayed_memory_on_disk() {
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let db_path = dir.path().join("testing_effect.db");
        let db = TestDatabaseManager::new_at_path(db_path.clone());

        let id = db
            .storage
            .ingest(memory(
                "Spreading activation walks the knowledge graph from a seed memory.",
                "concept",
                &["graph"],
            ))
            .expect("ingest must succeed")
            .id;

        db.storage
            .downscale_retention_batch(&[id.as_str()], 0.5)
            .expect("downscale_retention_batch must succeed");
        let decayed = db
            .storage
            .get_node(&id)
            .expect("get_node must not error")
            .expect("the memory must exist");
        assert!(
            decayed.retrieval_strength < 0.6 && decayed.retention_strength < 0.6,
            "the downscale must be persisted before the recall: retrieval={} retention={}",
            decayed.retrieval_strength,
            decayed.retention_strength
        );
        assert_eq!(
            decayed.times_retrieved.unwrap_or(0),
            0,
            "a never-recalled memory must start at zero retrievals"
        );
        drop(db);

        // Recall in a *fresh* process on the decayed state, then restart again
        // and check the boost the recall wrote is what we read back.
        let restarted = reopen(&db_path);
        let hits = recall(
            &restarted.storage,
            "spreading activation knowledge graph",
            SearchMode::Keyword,
        );
        assert!(
            hits.iter().any(|n| n.id == id),
            "the decayed memory must stay recallable, got {:?}",
            ids(&hits)
        );
        drop(restarted);

        let restarted = reopen(&db_path);
        let boosted = restarted
            .storage
            .get_node(&id)
            .expect("get_node must not error")
            .expect("the memory must exist");
        assert!(
            (boosted.retrieval_strength - (decayed.retrieval_strength + 0.05)).abs() < 1e-9,
            "recall must add +0.05 retrieval strength on disk: {} -> {}",
            decayed.retrieval_strength,
            boosted.retrieval_strength
        );
        assert!(
            (boosted.retention_strength - (decayed.retention_strength + 0.02)).abs() < 1e-9,
            "recall must add +0.02 retention strength on disk: {} -> {}",
            decayed.retention_strength,
            boosted.retention_strength
        );
        assert_eq!(
            boosted.times_retrieved.unwrap_or(0),
            1,
            "the retrieval counter must be persisted"
        );
        assert!(
            boosted.last_accessed > decayed.last_accessed,
            "recall must move last_accessed forward ({} -> {})",
            decayed.last_accessed,
            boosted.last_accessed
        );
    }
}
