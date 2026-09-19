//! Bearer token authentication for the HTTP transport.
//!
//! Token priority:
//! 1. `VESTIGE_AUTH_TOKEN` env var (override)
//! 2. Read from `<data_dir>/auth_token` file
//! 3. Generate `uuid::Uuid::new_v4()`, write to file with 0o600 permissions
//!
//! Security: The token file is created with restricted permissions from the
//! start (via OpenOptionsExt on Unix) to prevent a TOCTOU race where another
//! process could read the token before permissions are set.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use tracing::{info, warn};

/// Minimum recommended token length when provided via env var.
const MIN_TOKEN_LENGTH: usize = 32;

/// Return the auth token file path inside the Vestige data directory.
fn token_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dirs = ProjectDirs::from("com", "vestige", "core")
        .ok_or("could not determine project directories")?;
    Ok(dirs.data_dir().join("auth_token"))
}

/// Get (or create) the bearer token used for HTTP transport authentication.
///
/// Priority:
/// 1. `VESTIGE_AUTH_TOKEN` environment variable
/// 2. Existing `auth_token` file in the data directory
/// 3. Newly generated UUID v4, persisted to file
pub fn get_or_create_auth_token() -> Result<String, Box<dyn std::error::Error>> {
    // 1. Env var override
    if let Ok(token) = std::env::var("VESTIGE_AUTH_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            if token.len() < MIN_TOKEN_LENGTH {
                warn!(
                    "VESTIGE_AUTH_TOKEN is only {} chars (recommended >= {}). \
                     Short tokens are vulnerable to brute-force attacks.",
                    token.len(),
                    MIN_TOKEN_LENGTH
                );
            }
            info!("Using auth token from VESTIGE_AUTH_TOKEN env var");
            return Ok(token);
        }
    }

    let path = token_path()?;

    // 2. Read existing file
    if path.exists() {
        let token = fs::read_to_string(&path)?.trim().to_string();
        if !token.is_empty() {
            info!("Using auth token from {}", path.display());
            return Ok(token);
        }
    }

    // 3. Generate new token and persist
    let token = uuid::Uuid::new_v4().to_string();

    // Ensure parent directory exists with restricted permissions
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;

        // Restrict parent directory permissions on Unix (owner only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }

    // Write token file with restricted permissions from the start.
    // On Unix, we use OpenOptionsExt to set mode 0o600 at creation time,
    // avoiding the TOCTOU race of write-then-chmod.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600) // Owner read/write only — set at creation, no race window
            .open(&path)?;
        file.write_all(token.as_bytes())?;
        file.sync_all()?;
    }

    // On non-Unix (Windows), fall back to regular write (Windows ACLs are different)
    #[cfg(not(unix))]
    {
        fs::write(&path, &token)?;
    }

    info!("Generated new auth token at {}", path.display());
    Ok(token)
}

/// First up-to-8 **characters** of a token, for display in startup banners.
///
/// Slicing a `&str` by byte offset — `&token[..8]` — panics when the token is
/// shorter than 8 bytes or when byte 8 lands inside a multi-byte UTF-8 character.
/// `VESTIGE_AUTH_TOKEN` accepts any non-empty string (it only warns below 32
/// chars), so both are reachable from a documented configuration, and the panic
/// used to take the whole server down before the transport even started.
pub fn token_display_prefix(token: &str) -> String {
    token.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_prefix_truncates_instead_of_panicking() {
        // The first two panicked with `&token[..8]`; the third is fine either way
        // and pins the familiar 8-character hint.
        assert_eq!(token_display_prefix("abc"), "abc");
        assert_eq!(token_display_prefix(""), "");
        assert_eq!(
            token_display_prefix("0123456789abcdef0123456789abcdef"),
            "01234567"
        );
    }

    #[test]
    fn token_prefix_never_splits_a_multibyte_character() {
        // U+017C (LATIN SMALL LETTER Z WITH DOT ABOVE) is two bytes in UTF-8, so a
        // token of eight of them is 16 bytes and byte offset 8 falls inside the 5th
        // character — `&token[..8]` panicked here. Written with escapes so the test
        // cannot be broken by an encoding mishap.
        let token = "\u{17C}\u{17C}\u{17C}\u{17C}\u{17C}\u{17C}\u{17C}\u{17C}";
        let prefix = token_display_prefix(token);
        assert_eq!(prefix.chars().count(), 8);
        assert_eq!(prefix, token);

        // Nine of them: the 9th character must be dropped, not half of it kept.
        let longer = format!("{token}\u{17C}");
        assert_eq!(token_display_prefix(&longer), token);
    }
}
