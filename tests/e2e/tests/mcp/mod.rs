//! MCP Protocol E2E Tests
//!
//! Comprehensive tests for the Model Context Protocol implementation.
//!
//! **These tests run the real `vestige-mcp` binary** (spawned over stdio by
//! `vestige_e2e_tests::harness::McpServerProcess`) against a TempDir database,
//! so they validate the transport, dispatcher and tool catalog that ships —
//! not a JSON literal written inside the test file.
//!
//! They validate:
//! - JSON-RPC 2.0 framing and error codes returned by the server
//! - MCP initialization, lifecycle enforcement and clean shutdown
//! - Tool discovery (`tools/list`) against the live catalog
//! - Tool execution (`smart_ingest`, `search`, `memory`, `session_context`)
//! - Resource access (`resources/list`, `resources/read`) from real storage
//!
//! Note: this target compiles `protocol_tests` and `tool_tests` as modules,
//! which is why both are also declared as standalone `[[test]]` targets in
//! `Cargo.toml` — each of them spawns its own server processes.

mod protocol_tests;
mod tool_tests;
