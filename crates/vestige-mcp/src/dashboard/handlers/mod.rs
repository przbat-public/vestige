//! Dashboard API endpoint handlers
//!
//! Organised by HTTP route family. Each submodule owns its handlers,
//! request/response DTOs, and any error-mapping helpers used only by
//! handlers in that submodule. Cross-cutting helpers (e.g. `log_err`)
//! live here so they can be shared via `super::log_err`.
//!
//! v2.0: Adds cognitive operation endpoints (dream, explore, predict,
//! importance, consolidation). v3.2.2: split into submodules.

use axum::http::StatusCode;

pub(super) fn log_err(context: &'static str) -> impl Fn(vestige_core::StorageError) -> StatusCode {
    move |e| {
        tracing::error!(error = %e, context = context, "Dashboard handler error");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// Helper for the `JoinError` returned by `tokio::task::spawn_blocking`.
///
/// We treat a panicked blocking task as `500` and log it loudly — there is no
/// safe way to recover, but we want operators to notice. Kept as a sibling of
/// `log_err` so handler files stay terse.
pub(super) fn log_join_err(context: &'static str) -> impl Fn(tokio::task::JoinError) -> StatusCode {
    move |e| {
        tracing::error!(error = %e, context = context, "Blocking task panicked");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

mod cognitive;
mod graph;
mod history;
mod intentions;
mod maintenance;
mod memory;
mod metacognitive;
mod observability;
mod pages;
mod review;
mod search;

pub use cognitive::*;
pub use graph::*;
pub use history::*;
pub use intentions::*;
pub use maintenance::*;
pub use memory::*;
pub use metacognitive::*;
pub use observability::*;
pub use pages::*;
pub use review::*;
pub use search::*;
