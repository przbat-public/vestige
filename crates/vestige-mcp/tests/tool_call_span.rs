//! Observability contract for the MCP tool path.
//!
//! A dispatched `tools/call` must produce a `tracing` span carrying the tool
//! name, a per-call id and the duration — the audit found no spans and no
//! request logs anywhere on the tool path — and that span must stay free of the
//! call's arguments: `smart_ingest`, `search`, `intention` and `restore` carry
//! user memories, reminders and file paths, and a span is a log line that an
//! OTLP exporter may ship off-process.
//!
//! The collecting subscriber is built from `tracing` + `tracing-subscriber`,
//! both already production dependencies of this crate, so this file adds no
//! dev-dependency. The dispatcher is exercised through the public
//! `McpServer::handle_request` seam, the same entry point the stdio and HTTP
//! transports use.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tempfile::TempDir;
use tokio::sync::Mutex as AsyncMutex;
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{Layer, registry};

use vestige_core::Storage;
use vestige_mcp::cognitive::CognitiveEngine;
use vestige_mcp::protocol::types::JsonRpcRequest;
use vestige_mcp::server::McpServer;

/// One span as the layer saw it.
#[derive(Debug, Clone, Default)]
struct SpanData {
    name: String,
    fields: HashMap<String, String>,
}

/// Everything the layer collected, shared with the test body.
///
/// Implements `Layer` (rather than `Subscriber`) so it can be attached to
/// `tracing_subscriber::registry()` and handed to `WithSubscriber`, which keeps
/// the dispatcher scoped to the future under test instead of leaking it into
/// whichever other test happens to run on this thread.
#[derive(Clone, Default)]
struct Collected {
    /// `(span id, data)` in creation order. The id keeps a later `on_record`
    /// (`duration_ms` is recorded when the call returns) attached to the span
    /// it belongs to instead of a fresh one.
    spans: Arc<Mutex<Vec<(u64, SpanData)>>>,
    events: Arc<Mutex<Vec<HashMap<String, String>>>>,
}

impl Collected {
    fn span(&self, name: &str) -> Option<SpanData> {
        self.spans
            .lock()
            .expect("span collector mutex poisoned")
            .iter()
            .find(|(_, span)| span.name == name)
            .map(|(_, span)| span.clone())
    }

    fn events(&self) -> Vec<HashMap<String, String>> {
        self.events
            .lock()
            .expect("event collector mutex poisoned")
            .clone()
    }
}

/// Flattens recorded fields into strings. `tool`/`request_id` are `&str`,
/// `duration_ms` is `u64`, everything else arrives as `Debug` (which for a
/// `format_args!` message renders as plain text).
struct FieldVisitor<'a>(&'a mut HashMap<String, String>);

impl Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

impl<S: Subscriber> Layer<S> for Collected {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        let mut fields = HashMap::new();
        attrs.record(&mut FieldVisitor(&mut fields));
        self.spans
            .lock()
            .expect("span collector mutex poisoned")
            .push((
                id.into_u64(),
                SpanData {
                    name: attrs.metadata().name().to_string(),
                    fields,
                },
            ));
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: Context<'_, S>,
    ) {
        let mut spans = self.spans.lock().expect("span collector mutex poisoned");
        if let Some((_, span)) = spans
            .iter_mut()
            .find(|(span_id, _)| *span_id == id.into_u64())
        {
            values.record(&mut FieldVisitor(&mut span.fields));
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = HashMap::new();
        event.record(&mut FieldVisitor(&mut fields));
        self.events
            .lock()
            .expect("event collector mutex poisoned")
            .push(fields);
    }
}

fn json_rpc(id: u64, method: &str, params: Option<serde_json::Value>) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(id)),
        method: method.to_string(),
        params,
    }
}

/// A server on throwaway storage. `TempDir` is returned so the database
/// outlives the test body.
async fn server_with_temp_storage() -> (McpServer, TempDir) {
    let dir = TempDir::new().expect("temp dir");
    let storage = Storage::new(Some(dir.path().join("test.db"))).expect("storage");
    let cognitive = Arc::new(AsyncMutex::new(CognitiveEngine::new()));
    (McpServer::new(Arc::new(storage), cognitive), dir)
}

/// A tool call must produce `mcp.tool_call` with the tool name, an id and a
/// duration, and must not carry the call's arguments into any span field.
#[tokio::test]
async fn dispatched_tool_call_emits_span_with_tool_name_and_no_arguments() {
    const SENTINEL: &str = "vestige-sentinel-2f9c-a-memory-that-must-not-be-logged";

    let (mut server, _dir) = server_with_temp_storage().await;
    let collected = Collected::default();
    let subscriber = registry().with(collected.clone());

    let response = async {
        // The dispatcher rejects `tools/call` until `initialize` has run.
        let init = server.handle_request(json_rpc(1, "initialize", None)).await;
        assert!(init.is_some(), "initialize must be answered");

        server
            .handle_request(json_rpc(
                2,
                "tools/call",
                Some(serde_json::json!({
                    "name": "system_status",
                    // `system_status` ignores its arguments, so this sentinel is
                    // pure leak bait: any code path that starts recording
                    // arguments puts it in a span field and fails this test.
                    "arguments": { "probe": SENTINEL }
                })),
            ))
            .await
    }
    .with_subscriber(subscriber)
    .await
    .expect("tools/call must be answered");

    assert!(
        response.error.is_none(),
        "unexpected JSON-RPC error: {response:?}"
    );

    let span = collected
        .span("mcp.tool_call")
        .expect("no `mcp.tool_call` span was emitted for the call");
    assert_eq!(
        span.fields.get("tool").map(String::as_str),
        Some("system_status"),
        "span must carry the effective tool name: {:?}",
        span.fields
    );

    let request_id = span
        .fields
        .get("request_id")
        .expect("span carries no request_id");
    assert!(!request_id.is_empty(), "request_id must not be empty");

    let duration = span
        .fields
        .get("duration_ms")
        .expect("span carries no duration_ms");
    duration
        .parse::<u64>()
        .expect("duration_ms must be whole milliseconds");

    // Routing metadata only. Arguments may be memories, reminders or file
    // paths, and a span can leave the process.
    assert_eq!(
        span.fields.len(),
        3,
        "unexpected span fields: {:?}",
        span.fields
    );
    for value in span.fields.values() {
        assert!(
            !value.contains(SENTINEL),
            "a tool argument leaked into a span field: {value}"
        );
    }
}

/// A failing tool call must record an `error` event inside its span. The client
/// already gets the error frame; without the event the server log says nothing
/// about a call it answered, which is the "no request logs" half of the audit
/// finding.
#[tokio::test]
async fn failed_tool_call_records_error_event_inside_the_span() {
    let (mut server, _dir) = server_with_temp_storage().await;
    let collected = Collected::default();
    let subscriber = registry().with(collected.clone());

    let response = async {
        let _ = server.handle_request(json_rpc(1, "initialize", None)).await;
        server
            .handle_request(json_rpc(
                2,
                "tools/call",
                // `temporal` answers "Arguments required" without arguments, so
                // this takes the dispatcher's tool-error branch.
                Some(serde_json::json!({ "name": "temporal" })),
            ))
            .await
    }
    .with_subscriber(subscriber)
    .await
    .expect("tools/call must be answered");

    // A tool failure is a successful JSON-RPC exchange carrying `isError`.
    assert!(
        response.error.is_none(),
        "tool errors belong in the result, not in a JSON-RPC error: {response:?}"
    );
    let result = response.result.expect("CallToolResult payload");
    assert_eq!(
        result.get("isError").and_then(serde_json::Value::as_bool),
        Some(true),
        "expected an isError result: {result}"
    );

    let span = collected
        .span("mcp.tool_call")
        .expect("no span for the failed call");
    assert_eq!(
        span.fields.get("tool").map(String::as_str),
        Some("temporal")
    );
    assert!(
        span.fields.contains_key("duration_ms"),
        "failed calls must still be timed: {:?}",
        span.fields
    );

    let failures: Vec<_> = collected
        .events()
        .into_iter()
        .filter(|fields| {
            fields
                .get("message")
                .is_some_and(|message| message.contains("tool call failed"))
        })
        .collect();
    assert_eq!(
        failures.len(),
        1,
        "expected exactly one `tool call failed` event, got {failures:?}"
    );
    let error = failures[0]
        .get("error")
        .expect("error event carries no error field");
    assert!(
        error.contains("Arguments required"),
        "unexpected error text: {error}"
    );
}
