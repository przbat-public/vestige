//! Maintenance MCP Tools
//!
//! Exposes CLI-only operations as MCP tools so the agent can trigger them
//! automatically: system_status, consolidate, backup, export, gc,
//! regenerate_embeddings, split_memories.

use std::path::{Path, PathBuf};

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

/// Where an artifact subdirectory lives, without touching the filesystem.
///
/// Split out from [`artifact_dir`] so the location itself is testable: the whole
/// point of the fix is *where* the files go, and creating the directory in a unit
/// test would write into the developer's real data directory.
pub(super) fn artifact_path(name: &str) -> Result<PathBuf, String> {
    let project_dirs = directories::ProjectDirs::from("com", "vestige", "core")
        .ok_or("Could not determine data directory")?;
    Ok(project_dirs.data_dir().join(name))
}

/// Subdirectory of the Vestige data directory holding user-visible artifacts
/// (exports, backups), created with owner-only permissions.
///
/// Both callers used to write to `data_dir().parent()` — the *shared* parent of
/// `com.vestige.core`, which on macOS is `~/Library/Application Support/` and on
/// Linux `~/.local/share/`. That directory belongs to every other application on
/// the machine and sits outside the 0700/0600 hardening
/// `storage::sqlite::init` applies to the data directory. A full database snapshot
/// there — every memory, embedding and insight, in plaintext — was readable by any
/// local user, which defeats the point of the hardening.
pub(super) fn artifact_dir(name: &str) -> Result<PathBuf, String> {
    let dir = artifact_path(name)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {name} directory: {e}"))?;
    restrict_dir_to_owner(&dir);
    Ok(dir)
}

/// Restrict a directory to its owner. Best-effort: on a filesystem without Unix
/// modes this is a no-op, and the fallback is the pre-existing behaviour.
pub(super) fn restrict_dir_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Create (or truncate) a file only the owner can read, from the first byte.
///
/// `File::create` leaves the process umask in charge, which on a default Unix
/// setup means 0644 — world-readable. `OpenOptions::mode(0o600)` sets the mode at
/// creation, so there is no window in which the file exists with wider permissions
/// (the same TOCTOU-free pattern `protocol::auth` uses for the auth token).
pub(super) fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
    }

    #[cfg(not(unix))]
    {
        std::fs::File::create(path)
    }
}

/// Tighten an already-written file to owner-only.
///
/// Needed for artifacts a library call creates itself (`VACUUM INTO` for backups),
/// where we cannot pass a create mode. The parent directory is already 0700 by
/// then, so the exposure window is bounded by the directory, not the file mode.
pub(super) fn restrict_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}
