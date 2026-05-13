//! Unified Memory Tool
//!
//! Merges get_knowledge, delete_knowledge, and get_memory_state into a single
//! `memory` tool with action-based dispatch.

mod actions;
mod args;
mod execute;
mod helpers;
mod schema;

#[cfg(test)]
mod tests;

pub use execute::execute;
pub use helpers::{
    ACCESSIBILITY_ACTIVE, ACCESSIBILITY_DORMANT, ACCESSIBILITY_SILENT, compute_accessibility,
    state_from_accessibility,
};
pub use schema::schema;
