//! End-to-end orchestration: configuration, the public `HippocampalIndex`
//! type, and migration helpers from legacy in-memory layouts.
//!
//! ## Module Layout
//!
//! - `config` — `HippocampalIndexConfig` and its `Default`
//! - `engine` — `HippocampalIndex` + `HippocampalIndexStats`
//! - `migration` — `MigrationResult`, `MigrationNode`, bulk-migration impl

mod config;
mod engine;
mod migration;

pub use config::HippocampalIndexConfig;
pub use engine::{HippocampalIndex, HippocampalIndexStats};
pub use migration::{MigrationNode, MigrationResult};
