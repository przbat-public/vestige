//! Maintenance MCP Tools
//!
//! Exposes CLI-only operations as MCP tools so the agent can trigger them
//! automatically: system_status, consolidate, backup, export, gc,
//! regenerate_embeddings, split_memories.

mod backup;
mod consolidate;
mod export;
mod gc;
mod regenerate;
mod split_memories;
mod system_status;

#[cfg(test)]
mod tests;

pub use backup::{backup_schema, execute_backup};
pub use consolidate::{consolidate_schema, execute_consolidate};
pub use export::{execute_export, export_schema};
pub use gc::{execute_gc, gc_schema};
pub use regenerate::{execute_regenerate_embeddings, regenerate_embeddings_schema};
pub use split_memories::{execute_split_memories, split_memories_schema};
pub use system_status::{execute_system_status, system_status_schema};
