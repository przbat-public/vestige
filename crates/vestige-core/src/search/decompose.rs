//! Compound query decomposition — split multi-part queries for improved recall.
//!
//! ACL 2025 (aclanthology.org/2025.acl-srw.32) shows +36.7% MRR@10 and +11.6% F1
//! when using question decomposition + reranking vs single query.
//!
//! Detection patterns:
//! - Explicit conjunctions: "X AND Y", "X and also Y", "X as well as Y"
//! - Semicolons: "X; Y"
//! - Question chains: "What about X? And what about Y?"

use std::collections::HashMap;

/// Result of query decomposition.
#[derive(Debug, Clone)]
pub struct DecompositionResult {
    /// The original query (unchanged)
    pub original: String,
    /// Sub-queries (if compound), or vec![original] if simple
    pub sub_queries: Vec<String>,
    /// Whether the query was detected as compound
    pub is_compound: bool,
}

/// Detect and split compound queries.
///
/// Returns a `DecompositionResult` with sub-queries if the input
/// is a compound query, otherwise wraps the original as a single sub-query.
pub fn decompose_query(query: &str) -> DecompositionResult {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return DecompositionResult {
            original: trimmed.to_string(),
            sub_queries: vec![trimmed.to_string()],
            is_compound: false,
        };
    }

    // Try splitting strategies in priority order
    if let Some(parts) = split_by_semicolons(trimmed) {
        return DecompositionResult {
            original: trimmed.to_string(),
            sub_queries: parts,
            is_compound: true,
        };
    }

    if let Some(parts) = split_by_question_chains(trimmed) {
        return DecompositionResult {
            original: trimmed.to_string(),
            sub_queries: parts,
            is_compound: true,
        };
    }

    if let Some(parts) = split_by_conjunctions(trimmed) {
        return DecompositionResult {
            original: trimmed.to_string(),
            sub_queries: parts,
            is_compound: true,
        };
    }

    DecompositionResult {
        original: trimmed.to_string(),
        sub_queries: vec![trimmed.to_string()],
        is_compound: false,
    }
}

/// Merge results from multiple sub-query searches.
///
/// Strategy: union of results, deduplicate by node_id, keep max score per node.
pub fn merge_results<T: HasIdAndScore>(results: Vec<Vec<T>>) -> Vec<T> {
    let mut best_by_id: HashMap<String, T> = HashMap::new();

    for batch in results {
        for item in batch {
            let id = item.id().to_string();
            let score = item.score();

            match best_by_id.get(&id) {
                Some(existing) if existing.score() >= score => {}
                _ => {
                    best_by_id.insert(id, item);
                }
            }
        }
    }

    let mut merged: Vec<T> = best_by_id.into_values().collect();
    merged.sort_by(|a, b| {
        b.score()
            .partial_cmp(&a.score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    merged
}

/// Trait for search results that can be merged.
pub trait HasIdAndScore {
    fn id(&self) -> &str;
    fn score(&self) -> f64;
}

// --- Splitting strategies ---

fn split_by_semicolons(query: &str) -> Option<Vec<String>> {
    let parts: Vec<String> = query
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.len() >= 2 { Some(parts) } else { None }
}

fn split_by_question_chains(query: &str) -> Option<Vec<String>> {
    // Split on "? " followed by connectors
    let parts: Vec<String> = query
        .split('?')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            // Strip leading connectors. Matching is ASCII-case-insensitive on
            // the original `&str`: deriving a slice offset from
            // `s.to_lowercase()` is unsafe because lowercasing can change byte
            // lengths (U+212A KELVIN, U+0130, U+1E9E) and the offset can land
            // inside a character. See `find_ascii_case_insensitive`.
            let stripped = s.trim_start_matches(|c: char| c.is_whitespace());
            for prefix in ["and ", "also ", "what about "] {
                if let Some(rest) = strip_ascii_prefix_ci(stripped, prefix) {
                    return rest.trim().to_string();
                }
            }
            stripped.to_string()
        })
        .filter(|s| !s.is_empty())
        .collect();

    if parts.len() >= 2 { Some(parts) } else { None }
}

fn split_by_conjunctions(query: &str) -> Option<Vec<String>> {
    // "X and Y" / "X and also Y" / "X as well as Y"
    // Only split on " and " if both sides are substantial (>= 3 words each or >= 15 chars)
    let conjunctions = [" and also ", " as well as ", " AND "];

    for conj in &conjunctions {
        if let Some(pos) = query.find(conj) {
            let left = query[..pos].trim();
            let right = query[pos + conj.len()..].trim();

            if is_substantial(left) && is_substantial(right) {
                return Some(vec![left.to_string(), right.to_string()]);
            }
        }
    }

    // " and " — only split if both parts are substantial (avoids "bread and butter" false positives)
    if let Some((start, end)) = find_ascii_case_insensitive(query, " and ") {
        let left = query[..start].trim();
        let right = query[end..].trim();

        if is_substantial(left) && is_substantial(right) {
            return Some(vec![left.to_string(), right.to_string()]);
        }
    }

    None
}

/// Find an ASCII `needle` in `haystack` case-insensitively, returning a byte range
/// **inside `haystack`**.
///
/// `haystack.to_lowercase().find(needle)` cannot be used to slice `haystack`:
/// lowercasing may change byte lengths (U+212A KELVIN SIGN → `k`, U+0130 → `i̇`,
/// U+1E9E → `ß`), so the offset can land inside a multi-byte character and
/// `haystack[..pos]` panics — and with `panic = "abort"` in the release profile that
/// kills the whole server process on a user query. Matching byte-wise against an ASCII
/// needle is boundary-safe: every byte of a multi-byte UTF-8 character is >= 0x80, so
/// it can never equal an ASCII needle byte, and a match therefore starts and ends on a
/// character boundary.
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let hay = haystack.as_bytes();
    let ndl = needle.as_bytes();
    if ndl.is_empty() || hay.len() < ndl.len() {
        return None;
    }

    (0..=hay.len() - ndl.len())
        .find(|&i| hay[i..i + ndl.len()].eq_ignore_ascii_case(ndl))
        .map(|i| (i, i + ndl.len()))
}

fn is_substantial(text: &str) -> bool {
    text.len() >= 15 || text.split_whitespace().count() >= 3
}

/// Strip an ASCII `prefix` from `text` case-insensitively, returning the rest.
///
/// The returned slice starts at a character boundary because every matched
/// byte is ASCII (< 0x80), and a byte of a multi-byte UTF-8 character is
/// always >= 0x80.
fn strip_ascii_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let bytes = text.as_bytes();
    if bytes.len() >= prefix.len() && bytes[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_query_no_decomposition() {
        let result = decompose_query("What is FSRS-6?");
        assert!(!result.is_compound);
        assert_eq!(result.sub_queries.len(), 1);
        assert_eq!(result.sub_queries[0], "What is FSRS-6?");
    }

    #[test]
    fn test_conjunction_split_survives_length_changing_lowercase() {
        // Regression: the " and " branch searched `query.to_lowercase()` but sliced
        // `query`. U+212A KELVIN SIGN lowercases to a 1-byte `k`, so the offset landed
        // inside a multi-byte character and `query[..pos]` panicked — and with
        // `panic = "abort"` in release that killed the MCP server process.
        let result = decompose_query("the sensor reads 300 \u{212A} and the fan spins at 900 rpm");
        assert!(result.is_compound, "expected a split, got {result:?}");
        assert_eq!(result.sub_queries.len(), 2);
        assert!(result.sub_queries[0].contains("300 \u{212A}"));
        assert!(result.sub_queries[1].contains("fan spins"));

        // Same class, different character (U+0130 lowercases to two chars).
        let result = decompose_query(
            "\u{0130}stanbul deployment pipeline and the search indexing subsystem",
        );
        assert!(result.is_compound, "expected a split, got {result:?}");
        assert!(result.sub_queries[1].starts_with("the search indexing"));
    }

    #[test]
    fn test_conjunction_split_is_case_insensitive() {
        let result = decompose_query("the cache is cold AND the index is stale");
        assert!(result.is_compound);
        assert_eq!(result.sub_queries.len(), 2);
        // Surrounding text keeps its original casing; only the search is case-insensitive.
        let result = decompose_query("The Cache Is Cold And The Index Is Stale");
        assert!(result.is_compound, "expected a split, got {result:?}");
        assert_eq!(result.sub_queries[0], "The Cache Is Cold");
        assert_eq!(result.sub_queries[1], "The Index Is Stale");
    }

    #[test]
    fn test_semicolon_split() {
        let result = decompose_query("user preferences; project context");
        assert!(result.is_compound);
        assert_eq!(result.sub_queries.len(), 2);
        assert_eq!(result.sub_queries[0], "user preferences");
        assert_eq!(result.sub_queries[1], "project context");
    }

    #[test]
    fn test_question_chain() {
        let result = decompose_query("What is FSRS-6? And what about spreading activation?");
        assert!(result.is_compound);
        assert_eq!(result.sub_queries.len(), 2);
    }

    #[test]
    fn test_question_chain_strips_connectors_without_lowercase_offsets() {
        // Same anti-pattern as `split_by_conjunctions`: connector stripping used
        // an offset derived from `to_lowercase()`, whose byte length can differ
        // from the original (U+212A → `k`, U+0130 → `i̇`), so the slice could
        // land inside a character.
        let result = decompose_query("What is FSRS-6? ALSO what about the scheduler?");
        assert!(result.is_compound, "expected a split, got {result:?}");
        assert_eq!(result.sub_queries.len(), 2, "got {result:?}");
        assert_eq!(result.sub_queries[0], "What is FSRS-6");
        assert!(
            !result.sub_queries[1].to_lowercase().starts_with("also "),
            "leading connector must be stripped, got {:?}",
            result.sub_queries[1]
        );

        // A length-changing character in the connector position must not panic.
        let result = decompose_query("What is FSRS-6? \u{212A}nd the scheduler?");
        assert!(result.is_compound, "expected a split, got {result:?}");
        assert_eq!(result.sub_queries.len(), 2);
    }

    #[test]
    fn test_conjunction_split() {
        let result = decompose_query(
            "How does the embedding service work and also how does the search pipeline function",
        );
        assert!(result.is_compound);
        assert_eq!(result.sub_queries.len(), 2);
    }

    #[test]
    fn test_short_and_not_split() {
        // "bread and butter" — too short on both sides, should NOT split
        let result = decompose_query("bread and butter");
        assert!(!result.is_compound);
    }

    #[test]
    fn test_empty_query() {
        let result = decompose_query("");
        assert!(!result.is_compound);
        assert_eq!(result.sub_queries.len(), 1);
    }

    #[test]
    fn test_merge_results_dedup() {
        let batch1 = vec![
            FakeResult {
                id: "a".into(),
                score: 0.8,
            },
            FakeResult {
                id: "b".into(),
                score: 0.6,
            },
        ];
        let batch2 = vec![
            FakeResult {
                id: "a".into(),
                score: 0.9,
            },
            FakeResult {
                id: "c".into(),
                score: 0.7,
            },
        ];

        let merged = merge_results(vec![batch1, batch2]);
        assert_eq!(merged.len(), 3, "Should have 3 unique results");
        assert_eq!(merged[0].id(), "a");
        assert!(
            (merged[0].score() - 0.9).abs() < 0.001,
            "Should keep max score for 'a'"
        );
    }

    #[test]
    fn test_multiple_semicolons() {
        let result = decompose_query("topic A; topic B; topic C");
        assert!(result.is_compound);
        assert_eq!(result.sub_queries.len(), 3);
    }

    #[test]
    fn test_as_well_as_conjunction() {
        let result =
            decompose_query("How does the auth system work as well as the payment pipeline");
        assert!(result.is_compound);
        assert_eq!(result.sub_queries.len(), 2);
    }

    #[test]
    fn test_question_chain_with_also() {
        let result = decompose_query("Where is the config? Also where are the tests?");
        assert!(result.is_compound);
        assert_eq!(result.sub_queries.len(), 2);
    }

    #[test]
    fn test_original_preserved() {
        let result = decompose_query("topic A; topic B");
        assert_eq!(result.original, "topic A; topic B");
    }

    #[test]
    fn test_whitespace_only() {
        let result = decompose_query("   ");
        assert!(!result.is_compound);
    }

    #[test]
    fn test_merge_empty_batches() {
        let merged = merge_results::<FakeResult>(vec![vec![], vec![]]);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_single_batch() {
        let batch = vec![FakeResult {
            id: "x".into(),
            score: 0.5,
        }];
        let merged = merge_results(vec![batch]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id(), "x");
    }

    #[test]
    fn test_merge_preserves_sort_order() {
        let batch1 = vec![
            FakeResult {
                id: "a".into(),
                score: 0.3,
            },
            FakeResult {
                id: "b".into(),
                score: 0.9,
            },
        ];
        let batch2 = vec![FakeResult {
            id: "c".into(),
            score: 0.6,
        }];
        let merged = merge_results(vec![batch1, batch2]);
        assert_eq!(merged[0].id(), "b");
        assert_eq!(merged[1].id(), "c");
        assert_eq!(merged[2].id(), "a");
    }

    #[derive(Debug)]
    struct FakeResult {
        id: String,
        score: f64,
    }
    impl HasIdAndScore for FakeResult {
        fn id(&self) -> &str {
            &self.id
        }
        fn score(&self) -> f64 {
            self.score
        }
    }
}
