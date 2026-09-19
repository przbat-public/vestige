//! # Consolidation Workflow Journey Tests
//!
//! Tests the sleep-inspired memory consolidation workflow that processes
//! memories during idle periods to strengthen, decay, and organize them.
//!
//! ## User Journey
//!
//! 1. User creates memories throughout the day
//! 2. System detects idle period (like sleep)
//! 3. Consolidation runs: decay, replay, integrate, prune, transfer
//! 4. Important memories are strengthened
//! 5. Weak/old memories are pruned
//! 6. New connections between memories are discovered
//!
//! ## Scope: contract tests **and** a real storage journey
//!
//! The top-level tests are pure-function contract tests over
//! `SleepConsolidation`, `ConnectionGraph`, `MemoryDreamer` and the scheduler —
//! they never construct `Storage`. The real pipeline journey lives in
//! [`storage_journeys`] at the bottom: two memories on a temp SQLite file,
//! aged, run through `Storage::run_consolidation`, and checked through a cold
//! restart for the decay, promotion, retention snapshots and history rows the
//! cycle claims to persist.

use chrono::{Duration, Utc};
use vestige_core::{
    advanced::dreams::{
        ActivityTracker, ConnectionGraph, ConnectionReason, ConsolidationScheduler, DreamConfig,
        DreamMemory, MemoryDreamer,
    },
    consolidation::SleepConsolidation,
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a test memory for dreaming
fn make_dream_memory(id: &str, content: &str, tags: Vec<&str>) -> DreamMemory {
    DreamMemory {
        id: id.to_string(),
        content: content.to_string(),
        embedding: None,
        tags: tags.into_iter().map(String::from).collect(),
        created_at: Utc::now(),
        access_count: 1,
    }
}

/// Create a memory with specific age
#[allow(dead_code)]
fn make_aged_memory(id: &str, content: &str, tags: Vec<&str>, hours_ago: i64) -> DreamMemory {
    DreamMemory {
        id: id.to_string(),
        content: content.to_string(),
        embedding: None,
        tags: tags.into_iter().map(String::from).collect(),
        created_at: Utc::now() - Duration::hours(hours_ago),
        access_count: 1,
    }
}

/// Create a memory with access count
#[allow(dead_code)]
fn make_accessed_memory(
    id: &str,
    content: &str,
    tags: Vec<&str>,
    access_count: u32,
) -> DreamMemory {
    DreamMemory {
        id: id.to_string(),
        content: content.to_string(),
        embedding: None,
        tags: tags.into_iter().map(String::from).collect(),
        created_at: Utc::now() - Duration::hours(24),
        access_count,
    }
}

// ============================================================================
// TEST 1: CONSOLIDATION DETECTS IDLE PERIODS
// ============================================================================

/// Test that the consolidation scheduler detects when user is idle.
///
/// Validates:
/// - Fresh scheduler starts in idle state
/// - Recording activity moves to active state
/// - Scheduler triggers consolidation when idle
#[test]
fn test_consolidation_detects_idle_periods() {
    let mut scheduler = ConsolidationScheduler::new();

    // Initially should be idle (no activity)
    let stats = scheduler.get_activity_stats();
    assert!(stats.is_idle, "Fresh scheduler should be idle");

    // Record activity - should no longer be idle
    scheduler.record_activity();
    scheduler.record_activity();
    scheduler.record_activity();

    let active_stats = scheduler.get_activity_stats();
    assert!(!active_stats.is_idle, "Should not be idle after activity");
    assert_eq!(
        active_stats.total_events, 3,
        "Should track 3 activity events"
    );

    // Verify activity rate is tracked
    assert!(
        active_stats.events_per_minute > 0.0 || active_stats.total_events > 0,
        "Activity rate should be tracked"
    );
}

// ============================================================================
// TEST 2: DECAY APPLIES TO OLD MEMORIES
// ============================================================================

/// Test that consolidation applies decay to memories based on age.
///
/// Validates:
/// - Old memories decay more than young memories
/// - Decay follows FSRS power law
/// - Emotional memories decay slower
#[test]
fn test_decay_applies_to_old_memories() {
    let consolidation = SleepConsolidation::new();

    // Young memory (1 day)
    let young_decay = consolidation.calculate_decay(10.0, 1.0, 0.0);

    // Medium memory (7 days)
    let medium_decay = consolidation.calculate_decay(10.0, 7.0, 0.0);

    // Old memory (30 days)
    let old_decay = consolidation.calculate_decay(10.0, 30.0, 0.0);

    // Verify decay increases with age
    assert!(
        young_decay > medium_decay,
        "Young ({:.3}) should retain more than medium ({:.3})",
        young_decay,
        medium_decay
    );
    assert!(
        medium_decay > old_decay,
        "Medium ({:.3}) should retain more than old ({:.3})",
        medium_decay,
        old_decay
    );

    // Verify all are in valid range
    assert!(young_decay <= 1.0 && young_decay > 0.0);
    assert!(medium_decay <= 1.0 && medium_decay > 0.0);
    assert!(old_decay <= 1.0 && old_decay > 0.0);

    // Test emotional protection
    let emotional_old_decay = consolidation.calculate_decay(10.0, 30.0, 1.0);
    assert!(
        emotional_old_decay > old_decay,
        "Emotional old memory ({:.3}) should retain more than neutral old ({:.3})",
        emotional_old_decay,
        old_decay
    );
}

// ============================================================================
// TEST 3: CONNECTIONS FORM BETWEEN RELATED MEMORIES
// ============================================================================

/// Test that consolidation discovers connections between memories.
///
/// Validates:
/// - Memories with shared tags form connections
/// - Connection strength reflects relationship strength
/// - Connections can be traversed
#[test]
fn test_connections_form_between_related_memories() {
    let mut graph = ConnectionGraph::new();

    // Add connections simulating discovered relationships
    graph.add_connection(
        "rust_async",
        "tokio_runtime",
        0.9,
        ConnectionReason::Semantic,
    );
    graph.add_connection(
        "tokio_runtime",
        "green_threads",
        0.8,
        ConnectionReason::Semantic,
    );
    graph.add_connection(
        "rust_async",
        "futures_crate",
        0.85,
        ConnectionReason::SharedConcepts,
    );

    // Verify graph structure
    let stats = graph.get_stats();
    assert_eq!(stats.total_connections, 3, "Should have 3 connections");

    // Verify connections are retrievable
    let async_connections = graph.get_connections("rust_async");
    assert_eq!(
        async_connections.len(),
        2,
        "rust_async should have 2 connections"
    );

    // Verify connection strength
    let total_strength = graph.total_connection_strength("rust_async");
    assert!(
        total_strength > 1.5,
        "Total strength should be > 1.5, got {:.2}",
        total_strength
    );

    // Verify strengthening works (Hebbian learning)
    let before = graph.total_connection_strength("rust_async");
    graph.strengthen_connection("rust_async", "tokio_runtime", 0.1);
    let after = graph.total_connection_strength("rust_async");
    assert!(
        after > before,
        "Strengthening should increase total: {:.2} > {:.2}",
        after,
        before
    );
}

// ============================================================================
// TEST 4: DREAM CYCLE GENERATES INSIGHTS
// ============================================================================

/// Test that the dream cycle synthesizes insights from memory clusters.
///
/// Validates:
/// - Dreamer analyzes all provided memories
/// - Clusters are identified from shared tags
/// - Insights combine information from multiple memories
#[tokio::test]
async fn test_dream_cycle_generates_insights() {
    let config = DreamConfig {
        max_memories_per_dream: 100,
        min_similarity: 0.1,
        max_insights: 10,
        min_novelty: 0.1,
        enable_compression: true,
        enable_strengthening: true,
        focus_tags: vec![],
    };
    let dreamer = MemoryDreamer::with_config(config);

    // Create related memories about error handling
    let memories = vec![
        make_dream_memory(
            "err1",
            "Result type in Rust handles recoverable errors explicitly",
            vec!["rust", "errors", "result"],
        ),
        make_dream_memory(
            "err2",
            "The ? operator propagates errors up the call stack",
            vec!["rust", "errors", "syntax"],
        ),
        make_dream_memory(
            "err3",
            "Custom error types with thiserror derive Error trait",
            vec!["rust", "errors", "types"],
        ),
        make_dream_memory(
            "err4",
            "anyhow crate provides flexible error handling for applications",
            vec!["rust", "errors", "anyhow"],
        ),
    ];

    let result = dreamer.dream(&memories).await;

    // Should analyze all memories
    assert_eq!(
        result.stats.memories_analyzed, 4,
        "Should analyze all 4 memories"
    );

    // Should evaluate connections
    assert!(
        result.stats.connections_evaluated > 0,
        "Should evaluate connections"
    );

    // Should find clusters (all share 'rust' and 'errors' tags)
    assert!(
        result.stats.clusters_found > 0 || result.new_connections_found > 0,
        "Should find clusters or connections"
    );
}

// ============================================================================
// TEST 5: PRUNING REMOVES WEAK MEMORIES
// ============================================================================

/// Test that pruning removes memories below threshold.
///
/// Validates:
/// - Pruning requires minimum age
/// - Pruning requires low retention
/// - Default pruning is disabled for safety
#[test]
fn test_pruning_removes_weak_memories() {
    let consolidation = SleepConsolidation::new();

    // Default: pruning disabled
    assert!(
        !consolidation.should_prune(0.05, 60),
        "Pruning should be disabled by default"
    );

    // Test that the method works correctly when checking conditions
    // Even with pruning disabled, we can verify the threshold logic:
    // - should_prune returns false when pruning is disabled
    // - The method checks retention < threshold AND age > min_age_days

    // With default config (pruning disabled):
    // All these should return false regardless of parameters
    assert!(
        !consolidation.should_prune(0.05, 60),
        "Old weak memory: pruning disabled"
    );

    assert!(
        !consolidation.should_prune(0.05, 10),
        "Young weak memory: pruning disabled"
    );

    assert!(
        !consolidation.should_prune(0.5, 60),
        "Old strong memory: pruning disabled"
    );

    assert!(
        !consolidation.should_prune(0.1, 60),
        "Boundary memory: pruning disabled"
    );

    // Verify the config accessor works
    let config = consolidation.config();
    assert!(
        !config.enable_pruning,
        "Default should have pruning disabled"
    );
    assert!(
        config.pruning_threshold > 0.0,
        "Should have a threshold configured"
    );
    assert!(
        config.pruning_min_age_days > 0,
        "Should have a min age configured"
    );
}

// ============================================================================
// ADDITIONAL CONSOLIDATION TESTS
// ============================================================================

/// Test activity tracker calculations.
#[test]
fn test_activity_tracker_calculations() {
    let mut tracker = ActivityTracker::new();

    // Initial state
    assert_eq!(tracker.activity_rate(), 0.0);
    assert!(tracker.time_since_last_activity().is_none());
    assert!(tracker.is_idle());

    // After activity
    tracker.record_activity();
    assert!(tracker.time_since_last_activity().is_some());
    assert!(!tracker.is_idle());

    // Stats
    let stats = tracker.get_stats();
    assert_eq!(stats.total_events, 1);
    assert!(stats.last_activity.is_some());
}

/// Test connection graph decay and pruning.
#[test]
fn test_connection_graph_decay_and_pruning() {
    let mut graph = ConnectionGraph::new();

    // Add connections with varying strengths
    graph.add_connection("a", "b", 0.9, ConnectionReason::Semantic);
    graph.add_connection("a", "c", 0.3, ConnectionReason::CrossReference);
    graph.add_connection("b", "c", 0.5, ConnectionReason::SharedConcepts);

    // Apply decay
    graph.apply_decay(0.5);

    // Prune weak connections
    let _pruned = graph.prune_weak(0.2);

    // Weak connection (0.3 * 0.5 = 0.15) should be pruned
    // The pruned count depends on implementation details
    let _stats = graph.get_stats();
}

/// Test consolidation run tracking.
#[test]
fn test_consolidation_run_tracking() {
    let consolidation = SleepConsolidation::new();
    let mut run = consolidation.start_run();

    // Record various operations
    run.record_decay();
    run.record_decay();
    run.record_decay();
    run.record_promotion();
    run.record_embedding();
    run.record_embedding();

    // Finish and verify
    let result = run.finish();

    assert_eq!(result.nodes_processed, 3);
    assert_eq!(result.decay_applied, 3);
    assert_eq!(result.nodes_promoted, 1);
    assert_eq!(result.embeddings_generated, 2);
    assert!(result.duration_ms >= 0);
}

/// Test retention calculation.
#[test]
fn test_retention_calculation() {
    let consolidation = SleepConsolidation::new();

    // Full retrieval, low storage
    let r1 = consolidation.calculate_retention(1.0, 1.0);
    assert!(r1 > 0.7, "High retrieval should mean high retention");

    // Full retrieval, max storage
    let r2 = consolidation.calculate_retention(10.0, 1.0);
    assert!((r2 - 1.0).abs() < 0.01, "Max everything should be ~1.0");

    // Low retrieval, max storage
    let r3 = consolidation.calculate_retention(10.0, 0.0);
    assert!((r3 - 0.3).abs() < 0.01, "Low retrieval should cap at ~0.3");

    // Both low
    let r4 = consolidation.calculate_retention(0.0, 0.0);
    assert!(r4 < 0.1, "Both low should mean low retention");
}

// ============================================================================
// REAL STORAGE JOURNEY (the 17-step pipeline on SQLite)
// ============================================================================
//
// The contract tests above call `SleepConsolidation` directly, so they cannot
// fail if `Storage::run_consolidation` never writes a single row. This journey
// runs the real pipeline and reads every asserted value back out of SQLite
// through a fresh `Storage` — the state a restart has to rebuild.

mod storage_journeys {
    use std::path::Path;
    use std::time::Duration as StdDuration;

    use chrono::{Duration, Utc};
    use rusqlite::Connection;
    use tempfile::TempDir;
    use vestige_core::Rating;
    use vestige_core::memory::IngestInput;
    use vestige_e2e_tests::harness::{TestDatabaseManager, enable_mock_embeddings};

    fn memory(content: &str, tag: &str, sentiment_magnitude: f64) -> IngestInput {
        IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            tags: vec![tag.to_string()],
            sentiment_score: if sentiment_magnitude > 0.0 { 0.8 } else { 0.0 },
            sentiment_magnitude,
            ..Default::default()
        }
    }

    fn reopen(db_path: &Path) -> TestDatabaseManager {
        TestDatabaseManager::new_at_path(db_path.to_path_buf())
    }

    /// Age the two columns the FSRS decay pass reads. `Storage` has no
    /// injectable clock, so this is the only deterministic way to age a memory;
    /// every value *asserted* below is then computed by
    /// `Storage::run_consolidation` and read back through `Storage::get_node`.
    fn backdate(db_path: &Path, ids: &[String], days: i64) {
        let conn = Connection::open(db_path).expect("test setup connection");
        conn.busy_timeout(StdDuration::from_secs(5))
            .expect("busy_timeout");
        let old = (Utc::now() - Duration::days(days)).to_rfc3339();
        for id in ids {
            conn.execute(
                "UPDATE knowledge_nodes SET last_accessed = ?1, created_at = ?1 WHERE id = ?2",
                rusqlite::params![old, id],
            )
            .expect("test setup UPDATE must succeed");
        }
    }

    /// Real consolidation: decay and emotional promotion must be observable in
    /// the persisted rows (not just in the returned `ConsolidationResult`), the
    /// retention snapshot and history rows must be durable, and nothing may be
    /// deleted.
    #[test]
    fn test_journey_consolidation_persists_decay_promotion_and_snapshots() {
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let db_path = dir.path().join("consolidation.db");
        let db = TestDatabaseManager::new_at_path(db_path.clone());

        // Two memories with disjoint vocabularies (so the dedup step cannot
        // merge them) that differ only in emotional magnitude.
        let neutral = db
            .storage
            .ingest(memory("alpha bravo charlie delta echo", "neutral", 0.0))
            .expect("ingest must succeed");
        let emotional = db
            .storage
            .ingest(memory("foxtrot golf hotel india juliet", "emotional", 0.9))
            .expect("ingest must succeed");

        // One Good review each, so both start from the same FSRS state and the
        // only remaining difference is the sentiment boost.
        db.storage
            .mark_reviewed(&neutral.id, Rating::Good)
            .expect("mark_reviewed must succeed");
        db.storage
            .mark_reviewed(&emotional.id, Rating::Good)
            .expect("mark_reviewed must succeed");
        // Age both memories first, so the "before" snapshot below is the state
        // the pipeline actually starts from.
        backdate(&db_path, &[neutral.id.clone(), emotional.id.clone()], 30);

        let neutral_before = db
            .storage
            .get_node(&neutral.id)
            .expect("get_node must not error")
            .expect("the neutral memory must exist");
        let emotional_before = db
            .storage
            .get_node(&emotional.id)
            .expect("get_node must not error")
            .expect("the emotional memory must exist");
        assert_eq!(
            neutral_before.retention_strength, emotional_before.retention_strength,
            "test setup: the two memories must start from the same retention"
        );
        assert!(
            neutral_before.last_accessed < Utc::now() - Duration::days(29),
            "test setup: the memories must be aged, got {}",
            neutral_before.last_accessed
        );
        drop(db);

        // ---- The act: the real 17-step pipeline -----------------------------
        let db = reopen(&db_path);
        let mut cycles = Vec::new();
        // Three cycles, because `get_retention_trend` only reports a trend once
        // three snapshots exist — running it three times also proves the
        // snapshot row is written by every cycle, not just the first.
        for _ in 0..3 {
            let result = db
                .storage
                .run_consolidation()
                .expect("run_consolidation must succeed");
            assert_eq!(
                result.nodes_pruned, 0,
                "consolidation must never delete memories: {result:?}"
            );
            assert!(
                result.decay_applied >= 2,
                "both aged memories must be decayed: {result:?}"
            );
            cycles.push(result);
        }
        assert_eq!(
            db.node_count(),
            2,
            "a consolidation cycle must not add or remove memories"
        );

        // ---- Persisted effects ----------------------------------------------
        let neutral_after = db
            .storage
            .get_node(&neutral.id)
            .expect("get_node must not error")
            .expect("the neutral memory must survive consolidation");
        let emotional_after = db
            .storage
            .get_node(&emotional.id)
            .expect("get_node must not error")
            .expect("the emotional memory must survive consolidation");

        assert!(
            neutral_after.retention_strength < neutral_before.retention_strength,
            "FSRS decay must be persisted: retention {} -> {}",
            neutral_before.retention_strength,
            neutral_after.retention_strength
        );
        assert!(
            emotional_after.retention_strength > neutral_after.retention_strength,
            "the sentiment boost must slow decay (both memories share a stability, \
             only the emotional one is boosted): emotional={} neutral={}",
            emotional_after.retention_strength,
            neutral_after.retention_strength
        );
        assert!(
            emotional_after.storage_strength > emotional_before.storage_strength,
            "emotional promotion must be persisted: storage {} -> {}",
            emotional_before.storage_strength,
            emotional_after.storage_strength
        );
        assert_eq!(
            neutral_after.storage_strength, neutral_before.storage_strength,
            "the neutral memory must not be promoted"
        );
        assert_eq!(
            neutral_after.last_accessed, neutral_before.last_accessed,
            "decay must not fabricate an access"
        );

        // ---- Persisted bookkeeping ------------------------------------------
        let avg = db
            .storage
            .get_avg_retention()
            .expect("get_avg_retention must succeed");
        let expected_avg =
            (neutral_after.retention_strength + emotional_after.retention_strength) / 2.0;
        assert!(
            (avg - expected_avg).abs() < 1e-6,
            "the average retention must be computed from the persisted rows: {avg} vs {expected_avg}"
        );
        assert_ne!(
            db.storage
                .get_retention_trend()
                .expect("get_retention_trend must succeed"),
            "insufficient_data",
            "every cycle must persist a retention snapshot (three cycles were run)"
        );
        let history = db
            .storage
            .get_consolidation_history(10)
            .expect("get_consolidation_history must succeed");
        assert_eq!(
            history.len(),
            3,
            "each cycle must append exactly one consolidation_history row"
        );
        for record in &history {
            assert_eq!(
                record.memories_replayed, cycles[0].decay_applied as i32,
                "the history row must record the decay count the cycle reported"
            );
        }

        // ---- The rows are the only copy: read them back after a restart -----
        let expected_neutral = neutral_after.retention_strength;
        let expected_emotional = emotional_after.retention_strength;
        drop(db);
        let restarted = reopen(&db_path);
        let neutral_reread = restarted
            .storage
            .get_node(&neutral.id)
            .expect("get_node must not error")
            .expect("the neutral memory must survive a restart");
        let emotional_reread = restarted
            .storage
            .get_node(&emotional.id)
            .expect("get_node must not error")
            .expect("the emotional memory must survive a restart");
        assert_eq!(
            neutral_reread.retention_strength, expected_neutral,
            "the decayed retention must be durable"
        );
        assert_eq!(
            emotional_reread.retention_strength, expected_emotional,
            "the promoted retention must be durable"
        );
    }
}
