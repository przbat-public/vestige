//! `Migration` descriptor.

/// A database migration.
#[derive(Debug, Clone)]
pub struct Migration {
    /// Version number
    pub version: u32,
    /// Description
    pub description: &'static str,
    /// SQL to apply
    pub up: &'static str,
}
