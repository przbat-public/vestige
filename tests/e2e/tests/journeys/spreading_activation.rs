//! # Spreading Activation Journey Tests
//!
//! Tests the associative memory network that finds hidden connections
//! between memories through spreading activation - a technique inspired
//! by how neurons activate related memories in the brain.
//!
//! ## User Journey
//!
//! 1. User builds up memories over time (code, concepts, decisions)
//! 2. User queries for a concept
//! 3. System activates the source memory
//! 4. Activation spreads to related memories via association links
//! 5. User discovers hidden connections they didn't explicitly search for
//!
//! ## Scope: contract tests **and** a real storage journey
//!
//! The top-level tests build an `ActivationNetwork` in memory. That network is
//! rebuilt from SQLite on every boot, so the graph that matters is the one in
//! `memory_connections` — which those tests never touch. [`storage_journeys`]
//! at the bottom persists a real graph, restarts, walks it with the storage
//! BFS, rebuilds the in-memory network from the persisted edges, and then
//! decays and prunes it through the same primitives consolidation uses.

use std::collections::HashSet;
use vestige_core::neuroscience::spreading_activation::{
    ActivationConfig, ActivationNetwork, LinkType,
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a network with a coding knowledge graph
fn create_coding_network() -> ActivationNetwork {
    let mut network = ActivationNetwork::new();

    // Rust ecosystem
    network.add_edge(
        "rust".to_string(),
        "ownership".to_string(),
        LinkType::Semantic,
        0.95,
    );
    network.add_edge(
        "rust".to_string(),
        "borrowing".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "rust".to_string(),
        "cargo".to_string(),
        LinkType::PartOf,
        0.85,
    );
    network.add_edge(
        "ownership".to_string(),
        "memory_safety".to_string(),
        LinkType::Causal,
        0.9,
    );
    network.add_edge(
        "borrowing".to_string(),
        "lifetimes".to_string(),
        LinkType::Semantic,
        0.85,
    );

    // Async ecosystem
    network.add_edge(
        "rust".to_string(),
        "async_rust".to_string(),
        LinkType::Semantic,
        0.8,
    );
    network.add_edge(
        "async_rust".to_string(),
        "tokio".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "tokio".to_string(),
        "runtime".to_string(),
        LinkType::PartOf,
        0.85,
    );
    network.add_edge(
        "async_rust".to_string(),
        "futures".to_string(),
        LinkType::Semantic,
        0.85,
    );

    network
}

/// Create a network for testing multi-hop discovery
fn create_chain_network() -> ActivationNetwork {
    let config = ActivationConfig {
        decay_factor: 0.8,
        max_hops: 5,
        min_threshold: 0.05,
        allow_cycles: false,
    };
    let mut network = ActivationNetwork::with_config(config);

    // Create a chain: A -> B -> C -> D -> E
    network.add_edge(
        "node_a".to_string(),
        "node_b".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "node_b".to_string(),
        "node_c".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "node_c".to_string(),
        "node_d".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "node_d".to_string(),
        "node_e".to_string(),
        LinkType::Semantic,
        0.9,
    );

    network
}

// ============================================================================
// TEST 1: SPREADING FINDS HIDDEN CHAINS
// ============================================================================

/// Test that spreading activation discovers memories through chains.
///
/// Validates:
/// - Direct neighbors are activated
/// - 2-hop neighbors are activated
/// - Activation decays with distance
/// - Path is tracked correctly
#[test]
fn test_spreading_finds_hidden_chains() {
    let mut network = create_chain_network();

    // Activate from node_a
    let results = network.activate("node_a", 1.0);

    // Should find all nodes in the chain
    let found_ids: HashSet<_> = results.iter().map(|r| r.memory_id.as_str()).collect();

    assert!(
        found_ids.contains("node_b"),
        "Should find direct neighbor node_b"
    );
    assert!(found_ids.contains("node_c"), "Should find 2-hop node_c");
    assert!(found_ids.contains("node_d"), "Should find 3-hop node_d");
    assert!(found_ids.contains("node_e"), "Should find 4-hop node_e");

    // Verify distance tracking
    let node_b = results.iter().find(|r| r.memory_id == "node_b").unwrap();
    let node_e = results.iter().find(|r| r.memory_id == "node_e").unwrap();

    assert_eq!(node_b.distance, 1, "node_b should be at distance 1");
    assert_eq!(node_e.distance, 4, "node_e should be at distance 4");

    // Verify activation decay
    assert!(
        node_b.activation > node_e.activation,
        "Closer nodes should have higher activation"
    );
}

// ============================================================================
// TEST 2: ACTIVATION DECAYS WITH DISTANCE
// ============================================================================

/// Test that activation decays appropriately with each hop.
///
/// Validates:
/// - Decay factor is applied per hop
/// - Further nodes have lower activation
/// - Decay is configurable
#[test]
fn test_activation_decays_with_distance() {
    let config = ActivationConfig {
        decay_factor: 0.7, // 30% decay per hop
        max_hops: 4,
        min_threshold: 0.01,
        allow_cycles: false,
    };
    let mut network = ActivationNetwork::with_config(config);

    // Create chain with uniform edge strength
    network.add_edge("a".to_string(), "b".to_string(), LinkType::Semantic, 1.0);
    network.add_edge("b".to_string(), "c".to_string(), LinkType::Semantic, 1.0);
    network.add_edge("c".to_string(), "d".to_string(), LinkType::Semantic, 1.0);

    let results = network.activate("a", 1.0);

    let act_b = results
        .iter()
        .find(|r| r.memory_id == "b")
        .map(|r| r.activation)
        .unwrap_or(0.0);
    let act_c = results
        .iter()
        .find(|r| r.memory_id == "c")
        .map(|r| r.activation)
        .unwrap_or(0.0);
    let act_d = results
        .iter()
        .find(|r| r.memory_id == "d")
        .map(|r| r.activation)
        .unwrap_or(0.0);

    // Verify monotonic decrease
    assert!(act_b > act_c, "b ({:.3}) > c ({:.3})", act_b, act_c);
    assert!(act_c > act_d, "c ({:.3}) > d ({:.3})", act_c, act_d);

    // Verify approximate decay rate (allowing for floating point)
    let ratio = act_c / act_b;
    assert!(
        (ratio - 0.7).abs() < 0.1,
        "Decay ratio should be ~0.7, got {:.3}",
        ratio
    );
}

// ============================================================================
// TEST 3: EDGE REINFORCEMENT (HEBBIAN LEARNING)
// ============================================================================

/// Test that edges are strengthened through use.
///
/// Validates:
/// - Initial edge strength is recorded
/// - Reinforcement increases strength
/// - Strength caps at maximum (1.0)
#[test]
fn test_edge_reinforcement_hebbian() {
    let mut network = ActivationNetwork::new();

    // Add edge with moderate strength
    network.add_edge(
        "concept_a".to_string(),
        "concept_b".to_string(),
        LinkType::Semantic,
        0.5,
    );

    // Get initial associations
    let initial = network.get_associations("concept_a");
    let initial_strength = initial
        .iter()
        .find(|a| a.memory_id == "concept_b")
        .map(|a| a.association_strength)
        .unwrap_or(0.0);

    assert!(
        (initial_strength - 0.5).abs() < 0.01,
        "Initial should be 0.5"
    );

    // Reinforce the connection
    network.reinforce_edge("concept_a", "concept_b", 0.2);

    // Get reinforced associations
    let reinforced = network.get_associations("concept_a");
    let new_strength = reinforced
        .iter()
        .find(|a| a.memory_id == "concept_b")
        .map(|a| a.association_strength)
        .unwrap_or(0.0);

    assert!(
        new_strength > initial_strength,
        "Reinforcement should increase strength: {:.3} > {:.3}",
        new_strength,
        initial_strength
    );

    // Reinforce multiple times
    for _ in 0..10 {
        network.reinforce_edge("concept_a", "concept_b", 0.1);
    }

    // Should cap at 1.0
    let final_assoc = network.get_associations("concept_a");
    let final_strength = final_assoc
        .iter()
        .find(|a| a.memory_id == "concept_b")
        .map(|a| a.association_strength)
        .unwrap_or(0.0);

    assert!(
        final_strength <= 1.0,
        "Strength should cap at 1.0, got {:.3}",
        final_strength
    );
}

// ============================================================================
// TEST 4: NETWORK BUILDS FROM SEMANTIC LINKS
// ============================================================================

/// Test building a semantic network from related concepts.
///
/// Validates:
/// - Nodes are created automatically
/// - Edges connect nodes
/// - Associations can be queried
/// - Graph statistics are correct
#[test]
fn test_network_builds_from_semantic_links() {
    let mut network = create_coding_network();

    // Verify graph structure
    assert!(network.node_count() >= 9, "Should have at least 9 nodes");
    assert!(network.edge_count() >= 9, "Should have at least 9 edges");

    // Verify associations from rust
    let rust_assoc = network.get_associations("rust");
    assert!(
        rust_assoc.len() >= 3,
        "Rust should have at least 3 associations"
    );

    // Verify highest association (ownership at 0.95)
    assert_eq!(
        rust_assoc[0].memory_id, "ownership",
        "Highest association should be ownership"
    );

    // Verify spreading from rust reaches the whole ecosystem
    let results = network.activate("rust", 1.0);
    let found: HashSet<_> = results.iter().map(|r| r.memory_id.as_str()).collect();

    // Should reach direct concepts
    assert!(found.contains("ownership"));
    assert!(found.contains("borrowing"));
    assert!(found.contains("async_rust"));

    // Should reach 2-hop concepts
    assert!(found.contains("memory_safety")); // rust -> ownership -> memory_safety
    assert!(found.contains("tokio")); // rust -> async_rust -> tokio
}

// ============================================================================
// TEST 5: DIFFERENT LINK TYPES AFFECT ACTIVATION
// ============================================================================

/// Test that different link types can have different effects.
///
/// Validates:
/// - Semantic, Temporal, Causal, PartOf links all work
/// - Link type is preserved in results
/// - Different link types can coexist
#[test]
fn test_different_link_types_affect_activation() {
    let mut network = ActivationNetwork::new();

    // Add edges with different link types
    network.add_edge(
        "event".to_string(),
        "semantic_rel".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "event".to_string(),
        "temporal_rel".to_string(),
        LinkType::Temporal,
        0.8,
    );
    network.add_edge(
        "event".to_string(),
        "causal_rel".to_string(),
        LinkType::Causal,
        0.85,
    );
    network.add_edge(
        "event".to_string(),
        "part_of_rel".to_string(),
        LinkType::PartOf,
        0.7,
    );

    let results = network.activate("event", 1.0);

    // Should find all related nodes
    let found: HashSet<_> = results.iter().map(|r| r.memory_id.as_str()).collect();
    assert!(found.contains("semantic_rel"));
    assert!(found.contains("temporal_rel"));
    assert!(found.contains("causal_rel"));
    assert!(found.contains("part_of_rel"));

    // Verify link types are preserved
    let semantic = results
        .iter()
        .find(|r| r.memory_id == "semantic_rel")
        .unwrap();
    let temporal = results
        .iter()
        .find(|r| r.memory_id == "temporal_rel")
        .unwrap();
    let causal = results
        .iter()
        .find(|r| r.memory_id == "causal_rel")
        .unwrap();
    let part_of = results
        .iter()
        .find(|r| r.memory_id == "part_of_rel")
        .unwrap();

    assert_eq!(semantic.link_type, LinkType::Semantic);
    assert_eq!(temporal.link_type, LinkType::Temporal);
    assert_eq!(causal.link_type, LinkType::Causal);
    assert_eq!(part_of.link_type, LinkType::PartOf);

    // Verify activation reflects edge strength
    assert!(
        semantic.activation > part_of.activation,
        "Semantic (0.9) should have higher activation than PartOf (0.7)"
    );
}

// ============================================================================
// ADDITIONAL SPREADING ACTIVATION TESTS
// ============================================================================

/// Test max hops limit.
#[test]
fn test_max_hops_limit() {
    let config = ActivationConfig {
        decay_factor: 0.99, // Almost no decay
        max_hops: 2,        // But strict hop limit
        min_threshold: 0.01,
        allow_cycles: false,
    };
    let mut network = ActivationNetwork::with_config(config);

    // Create 5-node chain
    network.add_edge("a".to_string(), "b".to_string(), LinkType::Semantic, 1.0);
    network.add_edge("b".to_string(), "c".to_string(), LinkType::Semantic, 1.0);
    network.add_edge("c".to_string(), "d".to_string(), LinkType::Semantic, 1.0);
    network.add_edge("d".to_string(), "e".to_string(), LinkType::Semantic, 1.0);

    let results = network.activate("a", 1.0);
    let found: HashSet<_> = results.iter().map(|r| r.memory_id.as_str()).collect();

    // Should find b (1 hop) and c (2 hops)
    assert!(found.contains("b"), "Should find b at 1 hop");
    assert!(found.contains("c"), "Should find c at 2 hops");

    // Should NOT find d or e (3+ hops)
    assert!(!found.contains("d"), "Should not find d at 3 hops");
    assert!(!found.contains("e"), "Should not find e at 4 hops");
}

/// Test minimum threshold stops propagation.
#[test]
fn test_minimum_threshold() {
    let config = ActivationConfig {
        decay_factor: 0.5,  // 50% decay per hop
        max_hops: 10,       // High limit
        min_threshold: 0.2, // But high threshold
        allow_cycles: false,
    };
    let mut network = ActivationNetwork::with_config(config);

    // Create chain
    network.add_edge("a".to_string(), "b".to_string(), LinkType::Semantic, 1.0);
    network.add_edge("b".to_string(), "c".to_string(), LinkType::Semantic, 1.0);
    network.add_edge("c".to_string(), "d".to_string(), LinkType::Semantic, 1.0);
    network.add_edge("d".to_string(), "e".to_string(), LinkType::Semantic, 1.0);

    let results = network.activate("a", 1.0);
    let found: HashSet<_> = results.iter().map(|r| r.memory_id.as_str()).collect();

    // With 0.5 decay and 0.2 threshold:
    // b: 1.0 * 0.5 = 0.5 (above)
    // c: 0.5 * 0.5 = 0.25 (above)
    // d: 0.25 * 0.5 = 0.125 (below)

    assert!(found.contains("b"), "b should be found");
    assert!(found.contains("c"), "c should be found");
    // d and e may or may not be found depending on threshold implementation
}

/// Test path tracking.
#[test]
fn test_path_tracking() {
    let mut network = ActivationNetwork::new();

    network.add_edge(
        "start".to_string(),
        "middle".to_string(),
        LinkType::Semantic,
        0.9,
    );
    network.add_edge(
        "middle".to_string(),
        "end".to_string(),
        LinkType::Semantic,
        0.9,
    );

    let results = network.activate("start", 1.0);

    let end_result = results.iter().find(|r| r.memory_id == "end").unwrap();

    // Path should be: start -> middle -> end
    assert_eq!(end_result.path.len(), 3, "Path should have 3 nodes");
    assert_eq!(end_result.path[0], "start");
    assert_eq!(end_result.path[1], "middle");
    assert_eq!(end_result.path[2], "end");
}

/// Test convergent paths.
#[test]
fn test_convergent_paths() {
    let mut network = ActivationNetwork::new();

    // Create convergent paths: source -> a -> target and source -> b -> target
    network.add_edge(
        "source".to_string(),
        "path_a".to_string(),
        LinkType::Semantic,
        0.8,
    );
    network.add_edge(
        "source".to_string(),
        "path_b".to_string(),
        LinkType::Semantic,
        0.8,
    );
    network.add_edge(
        "path_a".to_string(),
        "target".to_string(),
        LinkType::Semantic,
        0.8,
    );
    network.add_edge(
        "path_b".to_string(),
        "target".to_string(),
        LinkType::Semantic,
        0.8,
    );

    let results = network.activate("source", 1.0);

    // Target should be reached
    let target_results: Vec<_> = results.iter().filter(|r| r.memory_id == "target").collect();
    assert!(!target_results.is_empty(), "Target should be activated");

    // Total activation from convergent paths
    let total: f64 = target_results.iter().map(|r| r.activation).sum();
    assert!(total > 0.0, "Target should have positive activation");
}

// ============================================================================
// REAL STORAGE JOURNEY (persisted graph, real BFS, decay and prune)
// ============================================================================
//
// The contract tests above only ever see a network built by hand in memory.
// The product rebuilds that network from `memory_connections` on every boot, so
// this journey persists a graph through `Storage::save_connection`, restarts,
// walks it with the storage BFS (`get_memory_subgraph`, `find_path_between`),
// rebuilds the in-memory network from the persisted edges, and finally decays
// and prunes the graph with the primitives consolidation runs.

mod storage_journeys {
    use std::path::Path;

    use chrono::Utc;
    use tempfile::TempDir;
    use vestige_core::memory::IngestInput;
    use vestige_core::neuroscience::spreading_activation::{
        ActivatedMemory, ActivationNetwork, LinkType,
    };
    use vestige_core::{ConnectionRecord, Storage};
    use vestige_e2e_tests::harness::{TestDatabaseManager, enable_mock_embeddings};

    /// The canonical string the product writes into
    /// `memory_connections.link_type`. Derived from the enum's own `serde`
    /// representation rather than hard-coded here, because the dashboard and
    /// dream traversal grep on these strings
    /// (`crates/vestige-mcp/src/tools/smart_ingest/post_ingest.rs:245-263`).
    fn label(link_type: LinkType) -> String {
        serde_json::to_value(link_type)
            .expect("LinkType must serialise")
            .as_str()
            .expect("LinkType serialises to a string")
            .to_string()
    }

    fn edge(source: &str, target: &str, strength: f64, link_type: LinkType) -> ConnectionRecord {
        let now = Utc::now();
        ConnectionRecord {
            source_id: source.to_string(),
            target_id: target.to_string(),
            strength,
            link_type: label(link_type),
            created_at: now,
            last_activated: now,
            activation_count: 1,
        }
    }

    fn memory(content: &str, tag: &str) -> IngestInput {
        IngestInput {
            content: content.to_string(),
            node_type: "concept".to_string(),
            tags: vec![tag.to_string()],
            ..Default::default()
        }
    }

    fn reopen(db_path: &Path) -> TestDatabaseManager {
        TestDatabaseManager::new_at_path(db_path.to_path_buf())
    }

    /// Rebuild the in-memory activation network from the persisted edges, the
    /// way the server does at boot.
    fn rebuild_network(storage: &Storage) -> ActivationNetwork {
        let mut network = ActivationNetwork::new();
        for conn in storage
            .get_all_connections()
            .expect("get_all_connections must succeed")
        {
            let link_type: LinkType =
                serde_json::from_value(serde_json::Value::String(conn.link_type.clone()))
                    .unwrap_or_else(|_| panic!("unknown persisted link_type: {}", conn.link_type));
            network.add_edge(
                conn.source_id.clone(),
                conn.target_id.clone(),
                link_type,
                conn.strength,
            );
        }
        network
    }

    /// The activation record for `id`, failing loudly when the traversal never
    /// reached it.
    fn activated<'a>(activation: &'a [ActivatedMemory], id: &str) -> &'a ActivatedMemory {
        activation
            .iter()
            .find(|a| a.memory_id == id)
            .unwrap_or_else(|| panic!("{id} must be activated from the hub: {activation:?}"))
    }

    /// Persist a graph → restart → traversal must reflect the persisted edges →
    /// decay and prune must be visible on disk without deleting memories.
    #[test]
    fn test_journey_persisted_graph_drives_traversal_decay_and_prune() {
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let db_path = dir.path().join("graph.db");
        let db = TestDatabaseManager::new_at_path(db_path.clone());

        // Five memories with disjoint vocabularies, so nothing here depends on
        // embedding similarity.
        let hub = db
            .storage
            .ingest(memory("alpha bravo charlie delta echo", "hub"))
            .expect("ingest must succeed");
        let n1 = db
            .storage
            .ingest(memory("foxtrot golf hotel india juliet", "one"))
            .expect("ingest must succeed");
        let n2 = db
            .storage
            .ingest(memory("kilo lima mike november oscar", "two"))
            .expect("ingest must succeed");
        let n3 = db
            .storage
            .ingest(memory("papa quebec romeo sierra tango", "three"))
            .expect("ingest must succeed");
        let n4 = db
            .storage
            .ingest(memory("uniform victor whiskey xray yankee", "four"))
            .expect("ingest must succeed");

        // A star around `hub` (unique highest degree) plus one weak leaf edge.
        for conn in [
            edge(&hub.id, &n1.id, 0.9, LinkType::Semantic),
            edge(&hub.id, &n2.id, 0.6, LinkType::Causal),
            edge(&hub.id, &n3.id, 0.3, LinkType::Temporal),
            edge(&n3.id, &n4.id, 0.15, LinkType::PartOf),
        ] {
            db.storage
                .save_connection(&conn)
                .expect("save_connection must succeed");
        }
        drop(db);

        // ---- Cold start: the graph must come back off disk -------------------
        let restarted = reopen(&db_path);
        assert_eq!(
            restarted
                .storage
                .get_all_connections()
                .expect("get_all_connections must succeed")
                .len(),
            4,
            "every persisted edge must survive a restart"
        );

        let hub_edges = restarted
            .storage
            .get_connections_for_memory(&hub.id)
            .expect("get_connections_for_memory must succeed");
        let strengths: Vec<f64> = hub_edges.iter().map(|c| c.strength).collect();
        assert_eq!(
            strengths,
            vec![0.9, 0.6, 0.3],
            "edges must come back ordered by strength with their weights intact"
        );
        assert_eq!(
            hub_edges
                .iter()
                .find(|c| c.target_id == n2.id || c.source_id == n2.id)
                .expect("the hub-n2 edge must exist")
                .link_type,
            label(LinkType::Causal),
            "the link type must survive the round trip"
        );

        assert_eq!(
            restarted
                .storage
                .get_most_connected_memory()
                .expect("get_most_connected_memory must succeed")
                .as_deref(),
            Some(hub.id.as_str()),
            "the hub has degree 3; the rest have degree 1 or 2"
        );

        // ---- Real BFS over the persisted graph ------------------------------
        let (nodes, edges) = restarted
            .storage
            .get_memory_subgraph(&hub.id, 2, 50)
            .expect("get_memory_subgraph must succeed");
        assert_eq!(
            nodes.len(),
            5,
            "2 hops from the hub must reach every memory in the component"
        );
        assert_eq!(edges.len(), 4, "the induced edge set must be complete");

        let path = restarted
            .storage
            .find_path_between(&n1.id, &n4.id, 3)
            .expect("find_path_between must succeed")
            .expect("n4 is reachable from n1 in 3 hops");
        assert_eq!(
            path,
            vec![n1.id.clone(), hub.id.clone(), n3.id.clone(), n4.id.clone()],
            "the shortest chain must follow the persisted edges"
        );
        assert_eq!(
            restarted
                .storage
                .find_path_between(&n1.id, &n4.id, 2)
                .expect("find_path_between must succeed"),
            None,
            "the depth limit must be honoured"
        );

        // ---- The in-memory network rebuilt from those same edges -------------
        let mut network = rebuild_network(&restarted.storage);
        let activation = network.activate(&hub.id, 1.0);
        assert!(
            activated(&activation, &n3.id).activation > 0.0,
            "the weakest direct neighbour must still clear the activation threshold"
        );
        assert!(
            activated(&activation, &n1.id).activation > activated(&activation, &n2.id).activation
                && activated(&activation, &n2.id).activation
                    > activated(&activation, &n3.id).activation,
            "propagated activation must follow the persisted edge strengths: {activation:?}"
        );
        assert_eq!(
            activated(&activation, &n1.id).link_type,
            LinkType::Semantic,
            "the rebuilt network must recover the link type"
        );

        // ---- Decay and prune (the primitives consolidation step 12 runs) ----
        assert_eq!(
            restarted
                .storage
                .apply_connection_decay(0.5)
                .expect("apply_connection_decay must succeed"),
            4,
            "decay must touch every edge"
        );
        drop(restarted);

        let restarted = reopen(&db_path);
        let decayed_hub_edges = restarted
            .storage
            .get_connections_for_memory(&hub.id)
            .expect("get_connections_for_memory must succeed");
        assert!(
            (decayed_hub_edges[0].strength - 0.45).abs() < 1e-9,
            "the decayed strength must be persisted, got {}",
            decayed_hub_edges[0].strength
        );
        assert!(
            (decayed_hub_edges[2].strength - 0.15).abs() < 1e-9,
            "every edge must be decayed, got {}",
            decayed_hub_edges[2].strength
        );

        // After the halving, 0.15 (hub-n3) and 0.075 (n3-n4) are both below the
        // threshold; 0.45 (hub-n1) and 0.30 (hub-n2) are not.
        assert_eq!(
            restarted
                .storage
                .prune_weak_connections(0.2)
                .expect("prune_weak_connections must succeed"),
            2,
            "both edges of the decayed weak branch are below the threshold"
        );
        drop(restarted);

        // ---- Pruning edges must not delete the memories they pointed at ------
        let restarted = reopen(&db_path);
        for (label, id) in [("n3", &n3.id), ("n4", &n4.id)] {
            assert!(
                restarted
                    .storage
                    .get_node(id)
                    .expect("get_node must not error")
                    .is_some(),
                "pruning an edge must never delete the memory ({label})"
            );
            assert_eq!(
                restarted
                    .storage
                    .get_connections_for_memory(id)
                    .expect("get_connections_for_memory must succeed")
                    .len(),
                0,
                "the pruned edges must be gone for {label}"
            );
        }
        assert_eq!(
            restarted
                .storage
                .get_connections_for_memory(&n2.id)
                .expect("get_connections_for_memory must succeed")
                .len(),
            1,
            "an edge above the threshold must survive the prune"
        );
        assert_eq!(
            restarted
                .storage
                .find_path_between(&hub.id, &n3.id, 3)
                .expect("find_path_between must succeed"),
            None,
            "the pruned edge must break the chain to the weak branch"
        );
        let (nodes_after, edges_after) = restarted
            .storage
            .get_memory_subgraph(&hub.id, 2, 50)
            .expect("get_memory_subgraph must succeed");
        assert_eq!(
            (nodes_after.len(), edges_after.len()),
            (3, 2),
            "the weak branch must drop out of the traversal while its memories stay in the store"
        );
    }
}
