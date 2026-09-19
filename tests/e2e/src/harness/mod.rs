//! Test Harness Module
//!
//! Provides test setup utilities:
//! - `TimeTravelEnvironment` for testing time-dependent behavior (decay, scheduling)
//! - `TestDatabaseManager` for isolated test databases
//! - `McpServerProcess` for driving the real `vestige-mcp` binary over stdio
//! - `EnvGuard` for flipping process-wide switches (mock embeddings, retention target)

mod db_manager;
pub mod env;
mod mcp_process;
mod time_travel;

pub use db_manager::TestDatabaseManager;
pub use env::{EnvGuard, enable_mock_embeddings, enable_mock_embeddings_below_retention_target};
pub use mcp_process::{
    McpServerProcess, SERVER_BIN_ENV, find_server_binary, missing_binary_help, workspace_root,
};
pub use time_travel::TimeTravelEnvironment;
