//! Smart Ingest Tool
//!
//! Intelligent memory ingestion with Prediction Error Gating.
//! Automatically decides whether to create, update, or supersede memories
//! based on semantic similarity to existing content.
//!
//! This solves the "bad vs good similar memory" problem by:
//! - Detecting when new content is similar to existing memories
//! - Updating existing memories when appropriate (low prediction error)
//! - Creating new memories when content is substantially different (high PE)
//! - Superseding demoted/outdated memories with better alternatives
//!
//! v1.5.0: Enhanced with cognitive pipeline:
//!   Pre-ingest: importance scoring (4-channel) + intent detection → auto-tag
//!   Post-ingest: synaptic tagging + novelty update + hippocampal indexing

mod anchors;
mod args;
mod batch;
mod compound;
mod execute;
mod post_ingest;
mod schema;

/// Self-containedness gate: notices when a memory will not be understandable
/// without the conversation that produced it.
mod self_contained;

#[cfg(test)]
mod tests;

pub use execute::execute;
pub use schema::schema;
