//! Pure helpers used by the search pipeline (compression, overlap, gating).

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

// ============================================================================
// READ PATH GATING (Oblivion pattern)
// ============================================================================

/// Greetings, acknowledgments, and social phrases that never need memory retrieval.
const TRIVIAL_PHRASES: &[&str] = &[
    "hi",
    "hello",
    "hey",
    "thanks",
    "thank you",
    "ok",
    "okay",
    "sure",
    "yes",
    "no",
    "bye",
    "goodbye",
    "got it",
    "right",
    "cool",
    "nice",
    "please",
    "welcome",
    "cheers",
    "np",
    "ty",
    "thx",
    "ack",
    "roger",
    "dzieki",
    "dzięki",
    "hej",
    "cześć",
    "tak",
    "nie",
    "dobra",
    "spoko",
    "siema",
    "nara",
    "ok ok",
    "oki",
    "jasne",
    "luzik",
];

/// Compress content to fit within a token budget ratio.
/// Keeps the most information-dense sentences, prioritizing the first and last
/// sentences (primacy/recency effect), plus any sentences containing key markers
/// like "BUG", "DECISION", "because", "root cause".
pub(super) fn compress_content(content: &str, ratio: f64) -> String {
    let sentences: Vec<&str> = content
        .split(['.', '\n'])
        .map(|s| s.trim())
        .filter(|s| s.len() > 5)
        .collect();

    if sentences.len() <= 2 || ratio >= 0.9 {
        return content.to_string();
    }

    let target_count = ((sentences.len() as f64 * ratio).ceil() as usize).max(2);
    let priority_markers = [
        "BUG",
        "DECISION",
        "because",
        "root cause",
        "solution",
        "fix",
        "important",
        "note",
        "warning",
        "error",
        "pattern",
    ];

    let mut scored: Vec<(usize, f64)> = sentences
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut score = 0.0_f64;
            if i == 0 {
                score += 2.0;
            }
            if i == sentences.len() - 1 {
                score += 1.5;
            }
            let lower = s.to_lowercase();
            for marker in &priority_markers {
                if lower.contains(marker) {
                    score += 1.0;
                }
            }
            score += s.len() as f64 / 200.0;
            (i, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut selected: Vec<usize> = scored.iter().take(target_count).map(|s| s.0).collect();
    selected.sort();

    let compressed: Vec<&str> = selected.iter().map(|&i| sentences[i]).collect();
    let result = compressed.join(". ");
    if sentences.len() > target_count {
        format!(
            "{}. [{} of {} segments]",
            result,
            target_count,
            sentences.len()
        )
    } else {
        result
    }
}

/// Word-level Jaccard overlap between two content strings.
/// Returns 0.0 (no overlap) to 1.0 (identical word sets).
pub(super) fn content_overlap(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<&str> = a
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    let words_b: std::collections::HashSet<&str> = b
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();

    let union_size = words_a.union(&words_b).count();
    if union_size == 0 {
        return 0.0;
    }
    let intersection_size = words_a.intersection(&words_b).count();
    intersection_size as f64 / union_size as f64
}

/// Tag-level Jaccard similarity between two tag vectors.
pub(super) fn tag_jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let set_a: std::collections::HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: std::collections::HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let union_size = set_a.union(&set_b).count();
    if union_size == 0 {
        return 0.0;
    }
    let intersection_size = set_a.intersection(&set_b).count();
    intersection_size as f64 / union_size as f64
}

pub(super) fn is_trivial_query(query: &str) -> bool {
    let trimmed = query.trim();
    let lower = trimmed.to_lowercase();
    let word_count = trimmed.split_whitespace().count();

    if word_count == 0 {
        return true;
    }
    if word_count <= 3 {
        return TRIVIAL_PHRASES.iter().any(|p| lower == *p);
    }
    false
}
