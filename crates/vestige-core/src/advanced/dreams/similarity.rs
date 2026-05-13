//! Similarity helpers — cosine for embeddings, Jaccard for tags, word
//! overlap for content, plus a word-boundary `contains_word` predicate
//! and a UTF-8 safe `truncate`.

use std::collections::HashSet;

use super::types::DreamMemory;

/// Calculate similarity between two memories
pub(super) fn calculate_memory_similarity(a: &DreamMemory, b: &DreamMemory) -> f64 {
    // Use embeddings if available
    if let (Some(emb_a), Some(emb_b)) = (&a.embedding, &b.embedding) {
        return cosine_similarity(emb_a, emb_b);
    }

    // Fallback to tag + content similarity
    let tag_sim = tag_similarity(&a.tags, &b.tags);
    let content_sim = content_word_similarity(&a.content, &b.content);

    tag_sim * 0.4 + content_sim * 0.6
}

/// Calculate tag similarity (Jaccard index)
pub(super) fn tag_similarity(tags_a: &[String], tags_b: &[String]) -> f64 {
    if tags_a.is_empty() && tags_b.is_empty() {
        return 0.0;
    }

    let set_a: HashSet<_> = tags_a.iter().collect();
    let set_b: HashSet<_> = tags_b.iter().collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Calculate content similarity via word overlap
pub(super) fn content_word_similarity(content_a: &str, content_b: &str) -> f64 {
    let words_a: HashSet<_> = content_a
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 3)
        .collect();

    let words_b: HashSet<_> = content_b
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 3)
        .collect();

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Check if `haystack` contains `needle` as a whole word (not as a substring of
/// a longer word). Treats any non-alphanumeric char as a word boundary.
pub(super) fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.contains(' ') {
        return haystack.contains(needle);
    }
    for (idx, _) in haystack.match_indices(needle) {
        let before_ok = idx == 0 || !haystack.as_bytes()[idx - 1].is_ascii_alphanumeric();
        let after_idx = idx + needle.len();
        let after_ok =
            after_idx >= haystack.len() || !haystack.as_bytes()[after_idx].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Calculate cosine similarity between two vectors
pub(super) fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    (dot / (mag_a * mag_b)) as f64
}

/// Truncate string to max length (UTF-8 safe)
pub(super) fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}
