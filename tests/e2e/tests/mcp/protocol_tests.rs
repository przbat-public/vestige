//! # MCP Protocol Compliance Tests — against the real server
//!
//! These tests spawn the actual `vestige-mcp` binary and speak JSON-RPC 2.0 to
//! it over stdio (see [`vestige_e2e_tests::harness::McpServerProcess`]). Every
//! assertion below is made on a frame the server produced, so a regression in
//! the transport, the dispatcher or the tool catalog fails them.
//!
//! History: this file used to build `serde_json::json!` literals in-process and
//! assert on those literals — the server was never started, so the CI job named
//! "MCP E2E Tests" passed even with a server that could not boot. The literal
//! shape tests were removed rather than renamed: the real responses asserted
//! here are a strict superset of what they checked.
//!
//! Prerequisite: `vestige-mcp` must be built (`cargo build -p vestige-mcp`,
//! or `cargo build --release -p vestige-mcp` as CI does). When no binary is
//! found the tests print a `[SKIP]` banner with that instruction instead of
//! failing; `$VESTIGE_MCP_BIN` overrides the lookup.

use serde_json::json;
use vestige_e2e_tests::harness::McpServerProcess;

/// Exact size of the catalog in `vestige-mcp/src/server/catalog.rs`
/// (guarded there by `tools_list_has_exactly_28_entries`). Asserting it here
/// too means a catalog change must be a conscious, end-to-end visible act.
const EXPECTED_TOOL_COUNT: usize = 28;

/// Tools whose presence downstream clients depend on. Names, not counts, are
/// the contract that breaks integrations.
const REQUIRED_TOOLS: [&str; 10] = [
    "search",
    "memory",
    "smart_ingest",
    "session_context",
    "system_status",
    "deep_reference",
    "explore_connections",
    "intention",
    "temporal",
    "confidence",
];

/// Extract `error.code` from a response, failing with the whole frame if the
/// server answered successfully.
fn error_code(response: &serde_json::Value) -> i64 {
    response["error"]["code"]
        .as_i64()
        .unwrap_or_else(|| panic!("expected a JSON-RPC error, got: {response}"))
}

// ============================================================================
// LIFECYCLE: initialize handshake over a real stdio pipe
// ============================================================================

/// The MCP lifecycle, driven end to end: pre-init rejection → `initialize` →
/// `notifications/initialized` producing no reply → `ping` → clean shutdown on
/// stdin EOF.
#[test]
fn test_lifecycle_enforces_initialize_then_serves_requests() {
    let Some(mut server) =
        McpServerProcess::spawn_or_skip("test_lifecycle_enforces_initialize_then_serves_requests")
    else {
        return;
    };

    // ---- Before initialize: MCP forbids serving anything else -------------
    let pre_init = server
        .request("tools/list", None)
        .expect("server must answer (with an error) before initialize");
    assert_eq!(
        pre_init["jsonrpc"], "2.0",
        "pre-init error must still be JSON-RPC 2.0: {pre_init}"
    );
    assert_eq!(
        error_code(&pre_init),
        -32003,
        "pre-init tools/list must be rejected with ServerNotInitialized: {pre_init}"
    );
    assert!(
        pre_init["result"].is_null(),
        "an error response must not carry a result: {pre_init}"
    );

    let pre_init_call = server
        .request(
            "tools/call",
            Some(json!({ "name": "smart_ingest", "arguments": { "content": "x" } })),
        )
        .expect("server must answer before initialize");
    assert_eq!(error_code(&pre_init_call), -32003);

    // ---- initialize -------------------------------------------------------
    let init = server.initialize().expect("initialize must succeed");

    assert!(
        init["protocolVersion"].is_string(),
        "initialize result must carry protocolVersion: {init}"
    );
    assert_eq!(
        init["serverInfo"]["name"], "vestige",
        "serverInfo.name is part of the client contract: {init}"
    );
    assert!(
        init["serverInfo"]["version"]
            .as_str()
            .is_some_and(|v| !v.is_empty()),
        "serverInfo.version must be a non-empty string: {init}"
    );
    assert!(
        init["capabilities"]["tools"].is_object(),
        "server must advertise the tools capability: {init}"
    );
    assert!(
        init["capabilities"]["resources"].is_object(),
        "server must advertise the resources capability: {init}"
    );
    assert!(
        init["instructions"]
            .as_str()
            .is_some_and(|i| i.contains("memory")),
        "servers should explain themselves in `instructions`: {init}"
    );

    // ---- notifications/initialized must NOT be answered -------------------
    // (the harness records id-less frames; the server must not emit any
    // response to a notification).
    server
        .notify("notifications/initialized", None)
        .expect("notification must be writable");
    let pong = server.request("ping", None).expect("ping must succeed");
    assert_eq!(
        pong["result"],
        json!({}),
        "MCP `ping` must return an empty result object: {pong}"
    );
    assert!(
        server.notifications().is_empty(),
        "server replied to a notification (or emitted an unsolicited frame): {:?}",
        server.notifications()
    );

    // ---- stdin EOF shuts the server down cleanly --------------------------
    assert_eq!(
        server.shutdown(),
        Some(0),
        "server must exit 0 after stdin EOF (stderr tail:\n{})",
        server.stderr_tail()
    );
}

// ============================================================================
// TOOL DISCOVERY: the advertised catalog is the server's real catalog
// ============================================================================

/// `tools/list` must return the live catalog: exact count, unique snake_case
/// names, the tools clients call, and a JSON Schema for each.
#[test]
fn test_tools_list_exposes_the_real_tool_catalog() {
    let Some(mut server) =
        McpServerProcess::spawn_or_skip("test_tools_list_exposes_the_real_tool_catalog")
    else {
        return;
    };
    server.initialize().expect("initialize must succeed");

    let tools = server.tools_list().expect("tools/list must succeed");
    assert_eq!(
        tools.len(),
        EXPECTED_TOOL_COUNT,
        "tools/list advertised {} tools, expected {EXPECTED_TOOL_COUNT} \
         (update catalog.rs, scripts/check-version-and-tools.sh and this constant together)",
        tools.len()
    );

    let mut names: Vec<String> = Vec::with_capacity(tools.len());
    for tool in &tools {
        let name = tool["name"]
            .as_str()
            .unwrap_or_else(|| panic!("tool entry without a name: {tool}"));
        assert!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "tool names are part of the wire contract and must be snake_case, got {name:?}"
        );
        assert!(
            tool["description"].as_str().is_some_and(|d| !d.is_empty()),
            "tool {name} must describe itself for the model: {tool}"
        );
        let schema = &tool["inputSchema"];
        assert!(
            schema.is_object(),
            "tool {name} must publish an inputSchema object: {tool}"
        );
        assert_eq!(
            schema["type"], "object",
            "tool {name} inputSchema must be an object schema: {schema}"
        );
        assert!(
            schema["properties"].is_object(),
            "tool {name} inputSchema must declare properties: {schema}"
        );
        names.push(name.to_string());
    }

    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        names.len(),
        "duplicate tool names would make dispatch ambiguous: {names:?}"
    );

    for required in REQUIRED_TOOLS {
        assert!(
            names.iter().any(|n| n == required),
            "tool `{required}` is missing from the live catalog: {names:?}"
        );
    }
}

// ============================================================================
// ERROR HANDLING: codes the server actually returns
// ============================================================================

/// Unknown methods, unknown tools, missing params and malformed JSON must be
/// rejected with the JSON-RPC codes clients rely on.
#[test]
fn test_jsonrpc_error_codes_are_returned_by_the_server() {
    let Some(mut server) =
        McpServerProcess::spawn_or_skip("test_jsonrpc_error_codes_are_returned_by_the_server")
    else {
        return;
    };

    // Parse errors are produced before initialization is even considered.
    server.send_raw("{ this is not json").unwrap();
    let parse_error = server
        .read_frame()
        .expect("server must answer a malformed frame");
    assert_eq!(
        parse_error["error"]["code"], -32700,
        "malformed JSON must yield a parse error: {parse_error}"
    );
    assert!(
        parse_error.get("id").is_none() || parse_error["id"].is_null(),
        "a parse error cannot know the request id: {parse_error}"
    );

    server.initialize().expect("initialize must succeed");

    // Unknown JSON-RPC method.
    let unknown_method = server
        .request("no/such/method", None)
        .expect("server must answer unknown methods");
    assert_eq!(error_code(&unknown_method), -32601);
    assert!(
        unknown_method["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("Method not found")),
        "unknown method should say so: {unknown_method}"
    );

    // Unknown tool name — the dispatch fallback, not a silent success.
    let unknown_tool = server
        .request(
            "tools/call",
            Some(json!({ "name": "definitely_not_a_tool", "arguments": {} })),
        )
        .expect("server must answer unknown tools");
    assert_eq!(
        error_code(&unknown_tool),
        -32601,
        "unknown tool must be reported as Method not found: {unknown_tool}"
    );
    assert!(
        unknown_tool["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("Unknown tool: definitely_not_a_tool")),
        "unknown tool error must name the tool: {unknown_tool}"
    );

    // Missing params (`tools/call` requires {name, arguments?}).
    let missing_params = server
        .request("tools/call", None)
        .expect("server must answer a params-less tools/call");
    assert_eq!(
        error_code(&missing_params),
        -32602,
        "missing tool-call parameters must be Invalid params: {missing_params}"
    );

    // `name` with the wrong type is a deserialization failure too.
    let bad_params = server
        .request("tools/call", Some(json!({ "name": 123 })))
        .expect("server must answer malformed params");
    assert_eq!(
        error_code(&bad_params),
        -32602,
        "a non-string tool name must be Invalid params: {bad_params}"
    );

    // Malformed *arguments* are deliberately NOT a JSON-RPC error: the
    // envelope carries arbitrary JSON and the tool layer reports the validation
    // failure as `isError: true` so the model can read it and correct itself.
    let bad_arguments = server
        .call_tool_raw("search", json!("not-an-object"))
        .expect("server must answer a tools/call with malformed arguments");
    assert_eq!(
        bad_arguments["isError"], true,
        "malformed arguments must be reported as a tool error: {bad_arguments}"
    );
    assert!(
        bad_arguments["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("Invalid arguments")),
        "the tool error must explain the validation failure: {bad_arguments}"
    );
}

// ============================================================================
// RESOURCES: served from the live database
// ============================================================================

/// `resources/list` + `resources/read` must be backed by the real storage: an
/// empty temp database reports `totalNodes: 0`, and the mock embedder switch
/// must be visible in the report (proving the child process honours
/// `VESTIGE_TEST_MOCK_EMBEDDINGS` instead of reaching for ONNX).
#[test]
fn test_resources_are_served_from_the_live_database() {
    let Some(mut server) =
        McpServerProcess::spawn_or_skip("test_resources_are_served_from_the_live_database")
    else {
        return;
    };
    server.initialize().expect("initialize must succeed");

    let list = server
        .request("resources/list", None)
        .expect("resources/list must succeed");
    let resources = list["result"]["resources"]
        .as_array()
        .unwrap_or_else(|| panic!("resources/list must return an array: {list}"));
    assert!(
        !resources.is_empty(),
        "server advertises resources but returned none: {list}"
    );
    let uris: Vec<&str> = resources.iter().filter_map(|r| r["uri"].as_str()).collect();
    for expected in ["memory://stats", "memory://recent", "codebase://decisions"] {
        assert!(
            uris.contains(&expected),
            "resource {expected} missing from {uris:?}"
        );
    }
    for resource in resources {
        assert!(
            resource["name"].as_str().is_some_and(|n| !n.is_empty()),
            "every resource needs a human-readable name: {resource}"
        );
    }

    // Read stats from the freshly created (empty) database.
    let stats_frame = server
        .request("resources/read", Some(json!({ "uri": "memory://stats" })))
        .expect("resources/read must succeed");
    let text = stats_frame["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("resources/read must return text content: {stats_frame}"));
    let stats: serde_json::Value = serde_json::from_str(text).expect("memory://stats must be JSON");
    assert_eq!(
        stats["totalNodes"], 0,
        "a fresh temp database must report zero memories: {stats}"
    );
    assert_eq!(
        stats["embeddingServiceReady"], true,
        "the child process must have honoured VESTIGE_TEST_MOCK_EMBEDDINGS \
         (otherwise this suite would need the real ONNX model): {stats}"
    );

    // Recent memories on an empty store is still valid JSON.
    let recent_frame = server
        .request("resources/read", Some(json!({ "uri": "memory://recent" })))
        .expect("resources/read memory://recent must succeed");
    let recent_text = recent_frame["result"]["contents"][0]["text"]
        .as_str()
        .expect("memory://recent must return text");
    serde_json::from_str::<serde_json::Value>(recent_text).expect("memory://recent must be JSON");

    // Unknown resource paths surface as server errors, not empty successes.
    let unknown = server
        .request(
            "resources/read",
            Some(json!({ "uri": "memory://no-such-resource" })),
        )
        .expect("server must answer unknown resource URIs");
    assert_eq!(
        error_code(&unknown),
        -32603,
        "unknown resource must be an internal error: {unknown}"
    );
}
