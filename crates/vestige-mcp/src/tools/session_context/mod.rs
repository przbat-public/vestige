//! Session Context Tool — One-call session initialization.
//!
//! Combines search, intentions, status, predictions, and codebase context
//! into a single token-budgeted response. Replaces 5 separate calls at
//! session start.

mod args;
mod execute;
mod helpers;
mod schema;

#[cfg(test)]
mod tests;

pub use execute::execute;
pub use schema::schema;
