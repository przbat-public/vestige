//! # MCP Tool Tests — executed against the real server
//!
//! Each test spawns the actual `vestige-mcp` binary, completes the MCP
//! handshake and then calls tools over stdio against a TempDir database (see
//! [`vestige_e2e_tests::harness::McpServerProcess`]). The assertions below are
//! about what the product *did* — a memory row that exists, is findable, is
//! still there after a server restart — not about JSON the test wrote itself.
//!
//! History: this file used to build `json!({...})` literals, run
//! `validate_tool_response` over them and assert on them. Nothing from the
//! `vestige` crates was imported at all, so the CI job could not fail for any
//! product reason. Those literals are gone; the shapes they documented are now
//! asserted on real responses (including the error envelopes, which the
//! literals never exercised).
//!
//! Prerequisite: a built `vestige-mcp` binary — see the module docs of
//! `protocol_tests.rs`; `$VESTIGE_MCP_BIN` overrides discovery.

use serde_json::json;
use vestige_e2e_tests::harness::McpServerProcess;

/// Content whose first sentence is distinctive enough to prove the search
/// pipeline (FTS5 + mock embedding + RRF) really returned *this* memory.
const E2E_CONTENT: &str =
    "Vestige E2E marker quantum-walrus-42 recorded that consolidation never deletes memories.";

const E2E_QUERY: &str = "quantum-walrus-42 consolidation deletes memories";

// ============================================================================
// FULL ROUND TRIP: ingest → search → get → restart → get
// ============================================================================

/// One memory, pushed through the real server end to end:
///
/// 1. `smart_ingest` (forceCreate) creates it and returns a UUID,
/// 2. `search` finds it by a query that is not a literal copy of the content,
/// 3. `memory(action="get")` returns exactly the stored content and metadata,
/// 4. after a **server restart** on the same database, the memory is still
///    there — the write reached SQLite instead of staying in process memory.
#[test]
fn test_ingest_search_get_survives_server_restart() {
    let Some(mut server) =
        McpServerProcess::spawn_or_skip("test_ingest_search_get_survives_server_restart")
    else {
        return;
    };
    server.initialize().expect("initialize must succeed");

    // ---- 1. ingest --------------------------------------------------------
    let ingest = server
        .call_tool(
            "smart_ingest",
            json!({
                "content": E2E_CONTENT,
                "nodeType": "fact",
                "tags": ["e2e-journey", "regression"],
                "source": "tests/e2e/tests/mcp/tool_tests.rs",
                "forceCreate": true
            }),
        )
        .expect("smart_ingest must succeed");

    assert_eq!(ingest["success"], true, "ingest payload: {ingest}");
    assert_eq!(ingest["decision"], "create", "ingest payload: {ingest}");
    let node_id = ingest["nodeId"]
        .as_str()
        .unwrap_or_else(|| panic!("ingest must return a nodeId: {ingest}"))
        .to_string();
    assert!(
        uuid::Uuid::parse_str(&node_id).is_ok(),
        "nodeId must be a UUID minted by storage, got {node_id:?}"
    );
    assert_eq!(
        ingest["hasEmbedding"], true,
        "the child process must embed with the offline mock embedder: {ingest}"
    );
    assert!(
        server.db_path().exists(),
        "the server must have created its own database at {} — isolated from the \
         developer's store",
        server.db_path().display()
    );

    // ---- 2. search --------------------------------------------------------
    let search = server
        .call_tool(
            "search",
            json!({ "query": E2E_QUERY, "limit": 5, "retrievalMode": "precise" }),
        )
        .expect("search must succeed");
    let results = search["results"]
        .as_array()
        .unwrap_or_else(|| panic!("search must return a results array: {search}"));
    assert!(
        !results.is_empty(),
        "search returned no results for a memory that was just ingested: {search}"
    );
    let ids: Vec<&str> = results.iter().filter_map(|r| r["id"].as_str()).collect();
    assert!(
        ids.contains(&node_id.as_str()),
        "search must find the ingested memory {node_id}; got {ids:?}"
    );
    let hit = results
        .iter()
        .find(|r| r["id"] == json!(node_id))
        .expect("hit must be present");
    assert!(
        hit["content"]
            .as_str()
            .is_some_and(|c| c.contains("quantum-walrus-42")),
        "search must return the stored content: {hit}"
    );

    // ---- 3. get -----------------------------------------------------------
    let fetched = server
        .call_tool("memory", json!({ "action": "get", "id": node_id }))
        .expect("memory get must succeed");
    assert_eq!(fetched["found"], true, "memory get payload: {fetched}");
    let stored_content = fetched["node"]["content"]
        .as_str()
        .unwrap_or_else(|| panic!("stored content must be a string: {fetched}"))
        .to_string();
    assert!(
        stored_content.contains("quantum-walrus-42"),
        "the stored content must keep the ingested marker (preprocessing may rewrite, \
         it must not lose the fact): {fetched}"
    );
    assert_eq!(
        hit["content"], stored_content,
        "search and get must agree on the stored content"
    );
    assert_eq!(fetched["node"]["nodeType"], "fact");
    let tags = fetched["node"]["tags"]
        .as_array()
        .unwrap_or_else(|| panic!("tags must be an array: {fetched}"));
    for expected in ["e2e-journey", "regression"] {
        assert!(
            tags.iter().any(|t| t == expected),
            "tag {expected} must round-trip through SQLite (preprocessing may add its own \
             tags, it must not drop the caller's): {fetched}"
        );
    }
    assert_eq!(
        fetched["node"]["hasEmbedding"], true,
        "the stored row must carry an embedding: {fetched}"
    );
    assert!(
        fetched["node"]["createdAt"].is_string(),
        "createdAt must be an RFC3339 timestamp: {fetched}"
    );

    // A miss must be reported as a miss, not as an error or an empty success.
    let missing = server
        .call_tool(
            "memory",
            json!({ "action": "get", "id": "00000000-0000-4000-8000-000000000000" }),
        )
        .expect("memory get for an unknown id must still answer");
    assert_eq!(
        missing["found"], false,
        "unknown ids must report found=false: {missing}"
    );

    // ---- 4. restart the server on the same database -----------------------
    server.restart().expect("server must restart");
    server
        .initialize()
        .expect("initialize after restart must succeed");

    let after_restart = server
        .call_tool("memory", json!({ "action": "get", "id": node_id }))
        .expect("memory get after restart must succeed");
    assert_eq!(
        after_restart["found"], true,
        "the memory must survive a server restart: {after_restart}"
    );
    assert_eq!(
        after_restart["node"]["content"], stored_content,
        "content must be byte-identical after the restart"
    );

    assert_eq!(
        server.shutdown(),
        Some(0),
        "server must exit 0 after stdin EOF (stderr tail:\n{})",
        server.stderr_tail()
    );
}

// ============================================================================
// TOOL-LEVEL VALIDATION ERRORS
// ============================================================================

/// Invalid arguments must come back as a *tool* error (`isError: true` with an
/// explanatory payload), not as a silent success and not as a panic that kills
/// the transport — the server has to stay usable for the next call.
#[test]
fn test_smart_ingest_validation_errors_keep_the_server_usable() {
    let Some(mut server) = McpServerProcess::spawn_or_skip(
        "test_smart_ingest_validation_errors_keep_the_server_usable",
    ) else {
        return;
    };
    server.initialize().expect("initialize must succeed");

    let (is_error, text) = server
        .call_tool_expecting_error("smart_ingest", json!({ "content": "   " }))
        .expect("empty content must produce a tool response");
    assert!(is_error, "empty content must set isError: {text}");
    assert!(
        text.contains("Content cannot be empty"),
        "error payload must explain the problem: {text}"
    );

    let (is_error, text) = server
        .call_tool_expecting_error("smart_ingest", json!({ "tags": ["no-content"] }))
        .expect("missing content must produce a tool response");
    assert!(is_error, "missing content must set isError: {text}");
    assert!(
        text.contains("Missing 'content' field"),
        "error payload must explain the problem: {text}"
    );

    // The transport survived both errors: a valid call still works.
    let ingest = server
        .call_tool(
            "smart_ingest",
            json!({ "content": E2E_CONTENT, "forceCreate": true }),
        )
        .expect("server must still serve valid calls after tool errors");
    assert_eq!(ingest["success"], true, "ingest payload: {ingest}");
}

// ============================================================================
// session_context: retrieval through a different tool
// ============================================================================

/// `session_context` must return the memory it was asked about (retrieval
/// through hybrid search + the Testing Effect) and report the automation
/// triggers on a fresh store.
#[test]
fn test_session_context_returns_ingested_memory_and_automation_triggers() {
    let Some(mut server) = McpServerProcess::spawn_or_skip(
        "test_session_context_returns_ingested_memory_and_automation_triggers",
    ) else {
        return;
    };
    server.initialize().expect("initialize must succeed");

    let ingest = server
        .call_tool(
            "smart_ingest",
            json!({
                "content": E2E_CONTENT,
                "nodeType": "fact",
                "tags": ["e2e-journey"],
                "forceCreate": true
            }),
        )
        .expect("smart_ingest must succeed");
    assert_eq!(ingest["success"], true);

    let context = server
        .call_tool(
            "session_context",
            json!({
                "queries": ["quantum-walrus-42"],
                "tokenBudget": 2000,
                "context": { "codebase": "vestige-e2e" }
            }),
        )
        .expect("session_context must succeed");

    let text = context["context"]
        .as_str()
        .unwrap_or_else(|| panic!("session_context must return a context string: {context}"));
    assert!(
        text.contains("## Session ("),
        "context must start with the session header: {text}"
    );
    assert!(
        text.contains("quantum-walrus-42"),
        "session_context must surface the ingested memory for its query: {text}"
    );

    let tokens_used = context["tokensUsed"]
        .as_i64()
        .unwrap_or_else(|| panic!("tokensUsed must be numeric: {context}"));
    let budget = context["tokenBudget"].as_i64().unwrap_or(2000);
    assert!(
        tokens_used <= budget,
        "session_context must respect its token budget ({tokens_used} > {budget})"
    );

    let triggers = &context["automationTriggers"];
    for trigger in ["needsDream", "needsBackup", "needsGc"] {
        assert!(
            triggers[trigger].is_boolean(),
            "automationTriggers.{trigger} must be a boolean: {context}"
        );
    }
    // A brand-new store has never been dreamed or backed up.
    assert_eq!(
        triggers["needsDream"], true,
        "a store that was never dreamed needs a dream: {context}"
    );
    assert_eq!(
        triggers["needsBackup"], true,
        "a store that was never backed up needs a backup: {context}"
    );
}
