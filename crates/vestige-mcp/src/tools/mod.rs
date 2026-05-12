//! MCP Tools
//!
//! Tool implementations for the Vestige MCP server.
//!
//! The unified tools (`codebase_unified`, `intention_unified`, `memory_unified`,
//! `search_unified`) plus `smart_ingest` and the metacognitive/maintenance set
//! make up the 27 tools advertised in `tools/list`. The granular tools listed
//! under "Internal tools" below are kept for backwards-compatible dispatch in
//! `server.rs::handle_tools_call` — they're called when a deprecated tool name
//! is received but are NOT exposed in `tools/list`. New code should use the
//! unified APIs.

// Active unified tools
pub mod codebase_unified;
pub mod intention_unified;
pub mod memory_unified;
pub mod search_unified;
pub mod smart_ingest;

// v1.2: Temporal query tools
pub mod changelog;
pub mod timeline;

// v1.2: Maintenance tools
pub mod maintenance;

// v1.3: Auto-save and dedup tools
pub mod dedup;
pub mod importance;

// v1.5: Cognitive tools
pub mod dream;
pub mod explore;
pub mod predict;
pub mod restore;

// v1.8: Context Packets
pub mod session_context;

// v1.9: Autonomic tools
pub mod health;
pub mod graph;

// v2.1: Metacognitive tools
pub mod reflect;
pub mod temporal;
pub mod confidence;

// v3.2.1: Deep Reference (cognitive reasoning engine)
pub mod cross_reference;

// Internal tools — not advertised in MCP tools/list but actively dispatched
// for backwards-compatible operations in server.rs.
#[allow(dead_code)]
pub mod context;
#[allow(dead_code)]
pub mod feedback;
#[allow(dead_code)]
pub mod memory_states;
#[allow(dead_code)]
pub mod review;
#[allow(dead_code)]
pub mod tagging;
