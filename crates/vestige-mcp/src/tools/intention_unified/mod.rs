//! Unified Intention Tool
//!
//! A single unified tool that merges all 5 intention operations:
//! - set_intention -> action: "set"
//! - check_intentions -> action: "check"
//! - complete_intention -> action: "update" with status: "complete"
//! - snooze_intention -> action: "update" with status: "snooze"
//! - list_intentions -> action: "list"

mod args;
mod check;
mod execute;
mod list;
mod schema;
mod set;
mod update;

#[cfg(test)]
mod tests;

pub use execute::execute;
pub use schema::schema;
