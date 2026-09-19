//! MCP Tools
//!
//! Tool implementations for the Vestige MCP server.
//!
//! The unified tools (`codebase_unified`, `intention_unified`, `memory_unified`,
//! `search_unified`) plus `smart_ingest` and the metacognitive/maintenance set
//! make up the 28 tools advertised in `tools/list`. `review` is internal —
//! used by the `mark_reviewed` dispatch in `server.rs::handle_tools_call` and
//! exercised by e2e tests, but is not advertised in `tools/list`.

// Active unified tools
pub mod codebase_unified;
pub mod intention_unified;
pub mod memory_unified;
pub mod search_unified;
pub mod smart_ingest;

// Cross-cutting helpers (destructive-op confirmation gate, etc.)
pub mod common;
pub mod untrusted;

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
pub mod precompute;
pub mod predict;
pub mod restore;

// v1.8: Context Packets
pub mod session_context;

// v1.9: Autonomic tools
pub mod graph;
pub mod health;

// v2.1: Metacognitive tools
pub mod confidence;
pub mod reflect;
pub mod temporal;

// v3.2.1: Deep Reference (cognitive reasoning engine)
pub mod cross_reference;

// Internal tool — used by `mark_reviewed` dispatch and e2e tests.
#[allow(dead_code)]
pub mod review;
