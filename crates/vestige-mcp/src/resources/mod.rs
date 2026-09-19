//! MCP Resources
//!
//! Resource implementations for the Vestige MCP server.

pub mod codebase;
pub mod memory;

/// Why a `resources/read` call failed.
///
/// The distinction is part of the wire contract, not a detail: MCP
/// `server/resources.md` (Error Handling) reserves `-32002` for "resource not
/// found" and `-32603` for internal errors. Collapsing both into `-32603`, as
/// this server used to, tells the client that a URI it simply does not serve is
/// a server crash — and hides genuine failures behind the same code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceError {
    /// The URI is not one this server publishes. Carries the URI so the client
    /// can see exactly what was not found.
    NotFound(String),
    /// The resource exists but could not be produced (storage failure, …).
    Internal(String),
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(uri) => write!(f, "Resource not found: {uri}"),
            Self::Internal(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ResourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_and_internal_stay_distinguishable() {
        // The transport maps these to different JSON-RPC codes, so they must not
        // be collapsed into one variant.
        assert_ne!(
            ResourceError::NotFound("memory://nope".into()),
            ResourceError::Internal("memory://nope".into())
        );
        assert_eq!(
            ResourceError::NotFound("memory://nope".to_string()).to_string(),
            "Resource not found: memory://nope"
        );
    }
}
