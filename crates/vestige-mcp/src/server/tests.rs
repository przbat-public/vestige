//! Tests for the MCP server (initialization, dispatch, errors).

use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use crate::protocol::types::{JsonRpcRequest, MCP_VERSION};
use vestige_core::Storage;

use super::McpServer;

/// Create a test storage instance with a temporary database
async fn test_storage() -> (Arc<Storage>, TempDir) {
    let dir = TempDir::new().unwrap();
    let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
    (Arc::new(storage), dir)
}

/// Create a test server with temporary storage
async fn test_server() -> (McpServer, TempDir) {
    let (storage, dir) = test_storage().await;
    let cognitive = Arc::new(Mutex::new(CognitiveEngine::new()));
    let server = McpServer::new(storage, cognitive);
    (server, dir)
}

/// Create a JSON-RPC request
fn make_request(method: &str, params: Option<serde_json::Value>) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: method.to_string(),
        params,
    }
}

// ========================================================================
// INITIALIZATION TESTS
// ========================================================================

#[tokio::test]
async fn test_initialize_sets_initialized_flag() {
    let (mut server, _dir) = test_server().await;
    assert!(!server.initialized);

    let request = make_request(
        "initialize",
        Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        })),
    );

    let response = server.handle_request(request).await;
    assert!(response.is_some());
    let response = response.unwrap();
    assert!(response.result.is_some());
    assert!(response.error.is_none());
    assert!(server.initialized);
}

#[tokio::test]
async fn test_initialize_returns_server_info() {
    let (mut server, _dir) = test_server().await;
    // Send with current protocol version to get it back
    let params = serde_json::json!({
        "protocolVersion": MCP_VERSION,
        "capabilities": {},
        "clientInfo": { "name": "test", "version": "1.0" }
    });
    let request = make_request("initialize", Some(params));

    let response = server.handle_request(request).await.unwrap();
    let result = response.result.unwrap();

    assert_eq!(result["protocolVersion"], MCP_VERSION);
    assert_eq!(result["serverInfo"]["name"], "vestige");
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());
    assert!(result["instructions"].is_string());
}

#[tokio::test]
async fn test_initialize_with_default_params() {
    let (mut server, _dir) = test_server().await;
    let request = make_request("initialize", None);

    let response = server.handle_request(request).await.unwrap();
    assert!(response.result.is_some());
    assert!(response.error.is_none());
}

// ========================================================================
// UNINITIALIZED SERVER TESTS
// ========================================================================

#[tokio::test]
async fn test_request_before_initialize_returns_error() {
    let (mut server, _dir) = test_server().await;

    let request = make_request("tools/list", None);
    let response = server.handle_request(request).await.unwrap();

    assert!(response.result.is_none());
    assert!(response.error.is_some());
    let error = response.error.unwrap();
    assert_eq!(error.code, -32003); // ServerNotInitialized
}

#[tokio::test]
async fn test_ping_before_initialize_returns_error() {
    let (mut server, _dir) = test_server().await;

    let request = make_request("ping", None);
    let response = server.handle_request(request).await.unwrap();

    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, -32003);
}

// ========================================================================
// NOTIFICATION TESTS
// ========================================================================

#[tokio::test]
async fn test_initialized_notification_returns_none() {
    let (mut server, _dir) = test_server().await;

    // First initialize
    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    // Send initialized notification
    let notification = make_request("notifications/initialized", None);
    let response = server.handle_request(notification).await;

    // Notifications should return None
    assert!(response.is_none());
}

// ========================================================================
// TOOLS/LIST TESTS
// ========================================================================

#[tokio::test]
async fn test_tools_list_returns_all_tools() {
    let (mut server, _dir) = test_server().await;

    // Initialize first
    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request("tools/list", None);
    let response = server.handle_request(request).await.unwrap();

    let result = response.result.unwrap();
    let tools = result["tools"].as_array().unwrap();

    // v3.3.0: 28 tools advertised. Authoritative list lives in
    // `server::catalog::build_tools_list` (see b15 split). The
    // `tools_list_has_exactly_28_entries` test there catches drift first
    // — this end-to-end test only verifies the JSON-RPC plumbing forwards
    // the catalog faithfully and asserts a few well-known tool names.
    assert_eq!(tools.len(), 28, "Expected exactly 28 tools in v3.3.0+");

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    // Unified tools
    assert!(tool_names.contains(&"search"));
    assert!(tool_names.contains(&"memory"));
    assert!(tool_names.contains(&"codebase"));
    assert!(tool_names.contains(&"intention"));

    // Core memory (smart_ingest absorbs ingest + checkpoint in v1.7)
    assert!(tool_names.contains(&"smart_ingest"));
    assert!(
        !tool_names.contains(&"ingest"),
        "ingest should be removed in v1.7"
    );
    assert!(
        !tool_names.contains(&"session_checkpoint"),
        "session_checkpoint should be removed in v1.7"
    );

    // Feedback merged into memory tool (v1.7)
    assert!(
        !tool_names.contains(&"promote_memory"),
        "promote_memory should be removed in v1.7"
    );
    assert!(
        !tool_names.contains(&"demote_memory"),
        "demote_memory should be removed in v1.7"
    );

    // Temporal tools (v1.2)
    assert!(tool_names.contains(&"memory_timeline"));
    assert!(tool_names.contains(&"memory_changelog"));

    // Maintenance tools (v1.7: system_status replaces health_check + stats)
    assert!(tool_names.contains(&"system_status"));
    assert!(
        !tool_names.contains(&"health_check"),
        "health_check should be removed in v1.7"
    );
    assert!(
        !tool_names.contains(&"stats"),
        "stats should be removed in v1.7"
    );
    assert!(tool_names.contains(&"consolidate"));
    assert!(tool_names.contains(&"backup"));
    assert!(tool_names.contains(&"export"));
    assert!(tool_names.contains(&"gc"));
    assert!(tool_names.contains(&"split_memories"));
    assert!(tool_names.contains(&"regenerate_embeddings"));

    // Auto-save & dedup tools (v1.3)
    assert!(tool_names.contains(&"importance_score"));
    assert!(tool_names.contains(&"find_duplicates"));

    // Cognitive tools (v1.5+)
    assert!(tool_names.contains(&"dream"));
    assert!(tool_names.contains(&"explore_connections"));
    assert!(tool_names.contains(&"predict"));
    assert!(tool_names.contains(&"precompute_for_context"));
    assert!(tool_names.contains(&"restore"));

    // Context packets (v1.8)
    assert!(tool_names.contains(&"session_context"));

    // Autonomic tools (v1.9)
    assert!(tool_names.contains(&"memory_health"));
    assert!(tool_names.contains(&"memory_graph"));

    // Metacognitive tools (v2.1)
    assert!(tool_names.contains(&"reflect"));
    assert!(tool_names.contains(&"temporal"));
    assert!(tool_names.contains(&"confidence"));

    // Cognitive reasoning (v3.2.1)
    assert!(tool_names.contains(&"deep_reference"));
}

#[tokio::test]
async fn test_tools_have_descriptions_and_schemas() {
    let (mut server, _dir) = test_server().await;

    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request("tools/list", None);
    let response = server.handle_request(request).await.unwrap();

    let result = response.result.unwrap();
    let tools = result["tools"].as_array().unwrap();

    for tool in tools {
        assert!(tool["name"].is_string(), "Tool should have a name");
        assert!(
            tool["description"].is_string(),
            "Tool should have a description"
        );
        assert!(
            tool["inputSchema"].is_object(),
            "Tool should have an input schema"
        );
    }
}

// ========================================================================
// RESOURCES/LIST TESTS
// ========================================================================

#[tokio::test]
async fn test_resources_list_returns_all_resources() {
    let (mut server, _dir) = test_server().await;

    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request("resources/list", None);
    let response = server.handle_request(request).await.unwrap();

    let result = response.result.unwrap();
    let resources = result["resources"].as_array().unwrap();

    // Verify expected resources are present
    let resource_uris: Vec<&str> = resources
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();

    assert!(resource_uris.contains(&"memory://stats"));
    assert!(resource_uris.contains(&"memory://recent"));
    assert!(resource_uris.contains(&"memory://decaying"));
    assert!(resource_uris.contains(&"memory://due"));
    assert!(resource_uris.contains(&"memory://intentions"));
    assert!(resource_uris.contains(&"codebase://structure"));
    assert!(resource_uris.contains(&"codebase://patterns"));
    assert!(resource_uris.contains(&"codebase://decisions"));
}

#[tokio::test]
async fn test_resources_have_descriptions() {
    let (mut server, _dir) = test_server().await;

    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request("resources/list", None);
    let response = server.handle_request(request).await.unwrap();

    let result = response.result.unwrap();
    let resources = result["resources"].as_array().unwrap();

    for resource in resources {
        assert!(resource["uri"].is_string(), "Resource should have a URI");
        assert!(resource["name"].is_string(), "Resource should have a name");
        assert!(
            resource["description"].is_string(),
            "Resource should have a description"
        );
    }
}

// ========================================================================
// UNKNOWN METHOD TESTS
// ========================================================================

#[tokio::test]
async fn test_unknown_method_returns_error() {
    let (mut server, _dir) = test_server().await;

    // Initialize first
    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request("unknown/method", None);
    let response = server.handle_request(request).await.unwrap();

    assert!(response.result.is_none());
    assert!(response.error.is_some());
    let error = response.error.unwrap();
    assert_eq!(error.code, -32601); // MethodNotFound
}

#[tokio::test]
async fn test_unknown_tool_returns_error() {
    let (mut server, _dir) = test_server().await;

    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request(
        "tools/call",
        Some(serde_json::json!({
            "name": "nonexistent_tool",
            "arguments": {}
        })),
    );

    let response = server.handle_request(request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, -32601);
}

// ========================================================================
// PING TESTS
// ========================================================================

#[tokio::test]
async fn test_ping_returns_empty_object() {
    let (mut server, _dir) = test_server().await;

    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request("ping", None);
    let response = server.handle_request(request).await.unwrap();

    assert!(response.result.is_some());
    assert!(response.error.is_none());
    assert_eq!(response.result.unwrap(), serde_json::json!({}));
}

// ========================================================================
// TOOLS/CALL TESTS
// ========================================================================

#[tokio::test]
async fn test_tools_call_missing_params_returns_error() {
    let (mut server, _dir) = test_server().await;

    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request("tools/call", None);
    let response = server.handle_request(request).await.unwrap();

    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, -32602); // InvalidParams
}

#[tokio::test]
async fn test_tools_call_invalid_params_returns_error() {
    let (mut server, _dir) = test_server().await;

    let init_request = make_request("initialize", None);
    server.handle_request(init_request).await;

    let request = make_request(
        "tools/call",
        Some(serde_json::json!({
            "invalid": "params"
        })),
    );

    let response = server.handle_request(request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, -32602);
}
