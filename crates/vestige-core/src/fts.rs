//! FTS5 Query Sanitization
//!
//! Always-available utilities for SQLite FTS5 full-text search.
//! Separated from the `search` module (which requires the `vector-search` feature)
//! because FTS5 keyword search is a core capability that works without embeddings.

/// Dangerous FTS5 operators that could be used for injection or DoS
const FTS5_OPERATORS: &[&str] = &["OR", "AND", "NOT", "NEAR"];

/// Upper bound on terms handed to FTS5 (a 1000-char query can otherwise build a
/// many-hundred-term OR expression that costs more than it can possibly return).
const MAX_TERMS: usize = 32;

/// Sanitize input for FTS5 MATCH queries.
///
/// Returns a **term query** (`"alpha" OR "beta"`), not a quoted phrase. The previous
/// implementation wrapped the whole input in double quotes, which silently turned
/// every keyword search into an *adjacency* search: `cargo cannot find crate` only
/// matched a memory containing that exact phrase, so ordinary multi-word queries
/// returned nothing. FTS5's own `bm25` ranking already scores documents matching more
/// terms higher, and our hybrid scorer re-ranks the result set afterwards, so a term
/// query buys recall without giving up ordering.
///
/// Prevents:
/// - Boolean operator injection (OR, AND, NOT, NEAR) — the words are dropped and every
///   surviving term is quoted, so FTS5 never parses user text as syntax
/// - Column targeting attacks (content:secret), prefix wildcards, unbalanced quotes —
///   a single allow-list rule keeps alphanumerics and treats everything else as a
///   separator (the old deny-list let `can't` open a string literal and return zero rows)
/// - DoS via complex query patterns — 1000-char cap plus a term cap
pub fn sanitize_fts5_query(query: &str) -> String {
    // Limit query length to prevent DoS (char-aware to avoid UTF-8 boundary issues)
    let limited: String = query.chars().take(1000).collect();

    let terms: Vec<String> = limited
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .filter(|term| {
            !FTS5_OPERATORS
                .iter()
                .any(|op| term.eq_ignore_ascii_case(op))
        })
        .take(MAX_TERMS)
        .map(|term| format!("\"{}\"", term))
        .collect();

    if terms.is_empty() {
        return "\"\"".to_string(); // Empty phrase - matches nothing safely
    }

    terms.join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_fts5_query_basic() {
        assert_eq!(sanitize_fts5_query("hello world"), "\"hello\" OR \"world\"");
    }

    #[test]
    fn test_sanitize_fts5_query_operators() {
        // Operator words are dropped, never interpreted — the remaining terms are ORed.
        assert_eq!(
            sanitize_fts5_query("hello OR world"),
            "\"hello\" OR \"world\""
        );
        assert_eq!(
            sanitize_fts5_query("hello AND world"),
            "\"hello\" OR \"world\""
        );
        assert_eq!(sanitize_fts5_query("NOT hello"), "\"hello\"");
    }

    #[test]
    fn test_sanitize_fts5_query_special_chars() {
        assert_eq!(
            sanitize_fts5_query("hello* world"),
            "\"hello\" OR \"world\""
        );
        assert_eq!(
            sanitize_fts5_query("content:secret"),
            "\"content\" OR \"secret\""
        );
        assert_eq!(sanitize_fts5_query("^boost"), "\"boost\"");
    }

    #[test]
    fn test_sanitize_fts5_query_no_longer_forces_adjacency() {
        // Regression: the whole query used to be wrapped in quotes, so this matched
        // only the literal phrase. Multi-word queries must stay term-based.
        assert_eq!(
            sanitize_fts5_query("cargo cannot find crate"),
            "\"cargo\" OR \"cannot\" OR \"find\" OR \"crate\""
        );
        // Apostrophes no longer open an FTS5 string literal.
        assert_eq!(
            sanitize_fts5_query("cargo can't find crate"),
            "\"cargo\" OR \"can\" OR \"t\" OR \"find\" OR \"crate\""
        );
        // Unicode letters survive (Polish memories are searchable).
        assert_eq!(
            sanitize_fts5_query("wdrożenie poniedziałek"),
            "\"wdrożenie\" OR \"poniedziałek\""
        );
    }

    #[test]
    fn test_sanitize_fts5_query_empty() {
        assert_eq!(sanitize_fts5_query(""), "\"\"");
        assert_eq!(sanitize_fts5_query("   "), "\"\"");
        assert_eq!(sanitize_fts5_query("* : ^"), "\"\"");
        assert_eq!(sanitize_fts5_query("AND OR NOT"), "\"\"");
    }

    #[test]
    fn test_sanitize_fts5_query_term_cap() {
        let long_query = (0..200)
            .map(|i| format!("term{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let sanitized = sanitize_fts5_query(&long_query);
        assert_eq!(sanitized.matches(" OR ").count(), MAX_TERMS - 1);
    }

    #[test]
    fn test_sanitize_fts5_query_length_limit() {
        let long_query = "a".repeat(2000);
        let sanitized = sanitize_fts5_query(&long_query);
        // One term survives the 1000-char cap, quoted.
        assert_eq!(sanitized, format!("\"{}\"", "a".repeat(1000)));
    }
}
