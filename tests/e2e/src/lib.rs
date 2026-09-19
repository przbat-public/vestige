//! E2E Test Infrastructure for Vestige
//!
//! Provides shared testing utilities for the `tests/e2e` targets:
//!
//! - **Harness** — `TestDatabaseManager` (isolated TempDir storage),
//!   `McpServerProcess` (spawns the real `vestige-mcp` binary and speaks
//!   JSON-RPC over stdio), `EnvGuard` (safe process-wide switches) and
//!   `TimeTravelEnvironment` (pure-function time travel)
//! - **Mocks** — `MockEmbeddingService` (FxHash-based), `TestDataFactory`
//! - **Assertions** — custom assertions for memory states, decay, etc.
//!
//! <!-- The `TimeTravelEnvironment`, `MockEmbeddingService` and
//! `TestDataFactory` are used by this crate's own unit tests only; the E2E
//! targets use `TestDatabaseManager`, `McpServerProcess` and `EnvGuard`. -->
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use vestige_e2e_tests::harness::{McpServerProcess, TestDatabaseManager};
//!
//! // Drive the real server over stdio (skips loudly if it is not built).
//! if let Some(mut server) = McpServerProcess::spawn_or_skip("my_test") {
//!     server.initialize().unwrap();
//!     let tools = server.tools_list().unwrap();
//!     assert!(!tools.is_empty());
//! }
//!
//! // Or exercise Storage directly on a throwaway database.
//! let db = TestDatabaseManager::new_temp();
//! assert!(db.is_empty());
//! ```

pub mod assertions;
pub mod harness;
pub mod mocks;

// Re-export commonly used items
pub use harness::{McpServerProcess, TestDatabaseManager, TimeTravelEnvironment};
pub use mocks::{MockEmbeddingService, TestDataFactory};

/// Convenient imports for tests
pub mod prelude {
    pub use crate::assertions::*;
    pub use crate::harness::{TestDatabaseManager, TimeTravelEnvironment};
    pub use crate::mocks::{MockEmbeddingService, TestDataFactory};

    // Re-export vestige-core essentials
    pub use vestige_core::{
        FSRSScheduler, FSRSState, IngestInput, KnowledgeNode, NodeType, Rating, RecallInput,
        Result, SearchMode, Storage, StorageError,
    };
}
