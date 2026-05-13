//! Context capture for codebase memory.
//!
//! Captures the current working context — what branch you're on, what files you're
//! editing, what the project structure looks like. This context is critical for:
//!
//! - Storing memories with full context for later retrieval
//! - Providing relevant suggestions based on current work
//! - Maintaining continuity across sessions
//!
//! ## Module Layout
//!
//! - `errors` — `ContextError`, `Result`
//! - `project_type` — `ProjectType` (Rust, TypeScript, …)
//! - `framework` — `Framework` (React, Axum, FastAPI, …)
//! - `working_context` — `WorkingContext`, `GitContextInfo`
//! - `file_context` — `FileContext` (per-file metadata)
//! - `capture` — `ContextCapture` (the public collector)

mod capture;
mod errors;
mod file_context;
mod framework;
mod project_type;
mod working_context;

#[cfg(test)]
mod tests;

pub use capture::ContextCapture;
pub use errors::{ContextError, Result};
pub use file_context::FileContext;
pub use framework::Framework;
pub use project_type::ProjectType;
pub use working_context::{GitContextInfo, WorkingContext};
