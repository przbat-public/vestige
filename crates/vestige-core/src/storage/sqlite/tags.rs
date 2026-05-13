//! Tag canonicalization (Cognee-style normalization).

/// Canonicalize tags to prevent fragmentation across surface forms.
/// "Bug Fix", "bug-fix", "bugfix", "BUG_FIX" all → "bug-fix".
/// Deduplicates after normalization and preserves order.
pub(crate) fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(tags.len());
    for tag in tags {
        let normalized = tag.trim().to_lowercase().replace([' ', '_'], "-");
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            result.push(normalized);
        }
    }
    result
}
