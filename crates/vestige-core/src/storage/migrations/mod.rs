//! Database migrations for the storage layer.
//!
//! ## Module Layout
//!
//! - `migration` — `Migration` descriptor struct
//! - `sql` — version-by-version SQL bodies (`MIGRATION_V*_UP`)
//! - `registry` — ordered `MIGRATIONS` slice the runner walks
//! - `runner` — `get_current_version` + `apply_migrations`

mod migration;
mod registry;
mod runner;
mod sql;

#[cfg(test)]
mod tests;

pub(crate) use runner::apply_migrations;
