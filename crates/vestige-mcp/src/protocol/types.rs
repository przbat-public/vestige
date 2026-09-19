//! MCP JSON-RPC Types
//!
//! Core types for JSON-RPC 2.0 protocol used by MCP.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP Protocol Version
pub const MCP_VERSION: &str = "2025-11-25";

/// Protocol revisions this server can speak, oldest first.
///
/// Used to validate the `MCP-Protocol-Version` header on the HTTP transport.
/// `initialize` negotiation accepts any *older* version string than
/// [`MCP_VERSION`]; this list is the stricter set of revisions whose wire format
/// we actually implement, so a client cannot pin us to an invented revision.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2024-11-05", "2025-03-26", "2025-06-18", MCP_VERSION];

/// JSON-RPC version
pub const JSONRPC_VERSION: &str = "2.0";

// ============================================================================
// JSON-RPC REQUEST/RESPONSE
// ============================================================================

/// JSON-RPC Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC Response
///
/// `id` is deliberately **always** serialized. JSON-RPC 2.0 §5 requires every
/// response object to carry the member, and requires `null` when the id could not
/// be determined (parse error, invalid request). Omitting it — which
/// `skip_serializing_if` used to do — produces a frame no conforming client can
/// correlate, and strict clients (e.g. the TS SDK's `JSONRPCErrorSchema`) reject it
/// outright.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

// ============================================================================
// JSON-RPC ERROR
// ============================================================================

/// JSON-RPC Error Codes (standard + MCP-specific)
#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
    // Standard JSON-RPC errors
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,

    // MCP-specific errors (-32000 to -32099)
    ConnectionClosed = -32000,
    RequestTimeout = -32001,
    ResourceNotFound = -32002,
    ServerNotInitialized = -32003,
}

impl From<ErrorCode> for i32 {
    fn from(code: ErrorCode) -> Self {
        code as i32
    }
}

/// JSON-RPC Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
            data: None,
        }
    }

    pub fn parse_error() -> Self {
        Self::new(ErrorCode::ParseError, "Parse error")
    }

    pub fn method_not_found() -> Self {
        Self::new(ErrorCode::MethodNotFound, "Method not found")
    }

    pub fn invalid_params(message: &str) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }

    pub fn internal_error(message: &str) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }

    pub fn server_not_initialized() -> Self {
        Self::new(ErrorCode::ServerNotInitialized, "Server not initialized")
    }

    /// `-32002 Resource not found`, per MCP `server/resources.md` Error Handling.
    ///
    /// Used by `resources/read` for a URI this server does not serve — a case the
    /// client must be able to tell apart from a server-side failure (`-32603`).
    pub fn resource_not_found(uri: &str) -> Self {
        Self::new(
            ErrorCode::ResourceNotFound,
            &format!("Resource not found: {}", uri),
        )
    }
}
impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for JsonRpcError {}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let request = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(Value::Number(1.into())),
            method: "test".to_string(),
            params: Some(serde_json::json!({"key": "value"})),
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.method, "test");
        assert!(parsed.id.is_some()); // Has id, not a notification
    }

    #[test]
    fn test_notification() {
        let notification = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: None,
            method: "notify".to_string(),
            params: None,
        };

        assert!(notification.id.is_none()); // No id = notification
    }

    #[test]
    fn test_response_success() {
        let response = JsonRpcResponse::success(
            Some(Value::Number(1.into())),
            serde_json::json!({"result": "ok"}),
        );

        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_response_error() {
        let response = JsonRpcResponse::error(
            Some(Value::Number(1.into())),
            JsonRpcError::method_not_found(),
        );

        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[test]
    fn error_without_id_serializes_id_as_null() {
        // JSON-RPC 2.0 §5: the `id` member MUST be present on every response and
        // MUST be `null` when it could not be determined. An omitted member is a
        // protocol violation that strict clients reject.
        let response = JsonRpcResponse::error(None, JsonRpcError::parse_error());
        let json = serde_json::to_value(&response).unwrap();

        assert!(
            json.as_object().unwrap().contains_key("id"),
            "`id` must be serialized even when unknown: {json}"
        );
        assert_eq!(json["id"], Value::Null);
        assert_eq!(json["error"]["code"], -32700);
    }

    #[test]
    fn error_codes_match_the_spec() {
        assert_eq!(JsonRpcError::method_not_found().code, -32601);
        assert_eq!(JsonRpcError::invalid_params("x").code, -32602);
        assert_eq!(JsonRpcError::internal_error("x").code, -32603);
        assert_eq!(
            JsonRpcError::resource_not_found("memory://nope").code,
            -32002
        );
        assert_eq!(
            JsonRpcError::resource_not_found("memory://nope").message,
            "Resource not found: memory://nope"
        );
    }
}
