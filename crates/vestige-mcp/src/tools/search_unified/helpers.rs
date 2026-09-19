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

/// Split content into segments on sentence boundaries.
///
/// A `.` ends a segment only when it is followed by whitespace or the end of the
/// string. Splitting on every `.` tore `v2.1.0` into `v2`/`1`/`0` and
/// `src/search/mmr.rs` into `.../mmr`/`rs`; the pieces were then dropped by the
/// length filter and the reader lost the exact version, path or file name the
/// memory existed to record.
fn split_segments(content: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut chars = content.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        let boundary = match ch {
            '\n' => true,
            '.' => chars.peek().is_none_or(|(_, next)| next.is_whitespace()),
            _ => false,
        };
        if boundary {
            let end = idx + ch.len_utf8();
            let segment = content[start..end].trim();
            if !segment.is_empty() {
                segments.push(segment);
            }
            start = end;
        }
    }
    if start < content.len() {
        let segment = content[start..].trim();
        if !segment.is_empty() {
            segments.push(segment);
        }
    }
    segments
}

/// Whether a segment carries an exact value the reader cannot reconstruct: a
/// version, a path, an identifier, a code fragment. This is the narrow test —
/// digits or code punctuation — for the segments a length-only filter throws
/// away first (`v2`, `1.0`, `rs`, `C++`, `fn f()`).
fn carries_exact_value(segment: &str) -> bool {
    segment.chars().any(|c| c.is_ascii_digit())
        || segment.contains([
            '/', '\\', '_', ':', '=', '(', ')', '{', '}', '[', ']', '<', '>', '#', '+', '*', '&',
            '|', '!', '$', '@',
        ])
}

/// A segment worth keeping as a compression candidate: long enough to be prose,
/// or short but carrying an exact value.
fn is_information_dense(segment: &str) -> bool {
    segment.len() > 5 || carries_exact_value(segment)
}

/// Compress content to fit within a token budget ratio.
/// Keeps the most information-dense sentences, prioritizing the first and last
/// sentences (primacy/recency effect), plus any sentences containing key markers
/// like "BUG", "DECISION", "because", "root cause".
pub(super) fn compress_content(content: &str, ratio: f64) -> String {
    // Drop only segments that are both short and value-free: `v2`, `rs` and
    // `fn f()` are short *and* worth keeping, so length alone cannot be the gate.
    let sentences: Vec<&str> = split_segments(content)
        .into_iter()
        .filter(|s| is_information_dense(s))
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
            // A segment carrying an exact value outranks prose of the same
            // length: `v2.1.0` and `src/lib.rs` are the payload of the memory,
            // and length alone must not be what keeps them out of the reader's
            // view. Long prose keeps the ranking it had before.
            if carries_exact_value(s) {
                score += 1.0;
            }
            score += s.len() as f64 / 200.0;
            (i, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut selected: Vec<usize> = scored.iter().take(target_count).map(|s| s.0).collect();
    selected.sort();

    let compressed: Vec<&str> = selected.iter().map(|&i| sentences[i]).collect();
    // Segments keep their own terminator (`split_segments` includes it), so the
    // join is a plain space; joining on ". " produced ". ." runs.
    let result = compressed.join(" ");
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

/// The tokens `content_overlap` compares: whitespace-separated, outer
/// punctuation trimmed, tokens of three characters or fewer dropped as noise.
fn content_tokens(content: &str) -> impl Iterator<Item = &str> {
    content
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
}

/// Word-level Jaccard overlap between two content strings.
/// Returns 0.0 (no overlap) to 1.0 (identical word sets).
pub(super) fn content_overlap(a: &str, b: &str) -> f64 {
    content_overlap_sets(&content_token_set(a), &content_token_set(b))
}

/// How many of the leading content tokens two strings share, in order.
///
/// The cheap stand-in for "these two memories are about the same subject and
/// relation": versioned statements about one fact (`X is married to A` →
/// `X is married to B`) keep their opening tokens and differ at the value,
/// while unrelated memories diverge immediately. Allocation-free and bounded by
/// `limit`, so it can run on every pair the pipeline retrieves.
fn shared_leading_tokens(a: &str, b: &str, limit: usize) -> usize {
    content_tokens(a)
        .zip(content_tokens(b))
        .take(limit)
        .take_while(|(x, y)| x == y)
        .count()
}

/// Whether two contents look like two versions of the same statement, the only
/// cheap signal the read path has that they may conflict.
///
/// This is a *gate*, not a verdict: the winner (if any) is decided by
/// `vestige_core::memory::FreshnessKey`. The thresholds are calibrated against
/// the FactConsolidation `sh_6k` conflict groups: requiring two shared leading
/// tokens plus a 0.25 word overlap recognises 59/59 pairs while a random pair of
/// haystack statements trips it ~1% of the time (`content_overlap` alone at 0.4
/// misses one and fires on 3.7%). The limitation is real and worth naming: without
/// a relation extractor this cannot separate "same subject, same relation,
/// different value" (a conflict) from "same subject, different relation"
/// (compatible facts), so pairs of the second shape are ordered newer-first too.
/// That only ever moves a score down by a fraction of a percent and never drops a
/// result, which is why a cheap gate is preferred to inventing an extractor here.
pub(super) fn looks_like_conflicting_versions(a: &str, b: &str) -> bool {
    shared_leading_tokens(a, b, 2) == 2 && content_overlap(a, b) >= 0.25
}

/// The token set [`content_overlap`] compares, exposed so a caller that compares
/// one candidate against many can build it once instead of per pair. The dedup and
/// MMR stages are both O(n²) in candidates and rebuild two sets per call today
/// (`pipeline/retrieval.rs`); this is the cheapest shape of that fix that does not
/// need the candidate type here.
pub(super) fn content_token_set(content: &str) -> std::collections::HashSet<&str> {
    content_tokens(content).collect()
}

/// Jaccard overlap of two precomputed token sets. Same value as
/// [`content_overlap`], no allocation.
pub(super) fn content_overlap_sets(
    a: &std::collections::HashSet<&str>,
    b: &std::collections::HashSet<&str>,
) -> f64 {
    let union_size = a.union(b).count();
    if union_size == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f64 / union_size as f64
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_keeps_versions_paths_and_code_fragments() {
        // Regression: the compressor split on every '.', so `v2.1.0` became
        // `v2`/`1`/`0` and `mmr.rs` became `mmr`/`rs`, and the length filter then
        // dropped every fragment. The reader got a memory with the version and the
        // file path removed — the two things a "which version, which file" memory
        // exists to carry.
        let content = "The deploy pipeline runs on Friday. The pinned version is v2.1.0. \
             See crates/vestige-core/src/search/mmr.rs for the constant. \
             The helper fn main() was removed. \
             Another sentence about the weather that is long enough to score well. \
             Yet another sentence about the release process. \
             Nothing else in this memory matters at all.";

        let compressed = compress_content(content, 0.6);

        assert!(
            compressed.contains("v2.1.0"),
            "version must survive verbatim, got: {compressed}"
        );
        assert!(
            compressed.contains("crates/vestige-core/src/search/mmr.rs"),
            "path must survive verbatim, got: {compressed}"
        );
        assert!(
            compressed.contains("fn main()"),
            "code fragment must survive, got: {compressed}"
        );
        assert!(
            compressed.len() < content.len(),
            "the fixture must actually exercise compression"
        );
    }

    #[test]
    fn dotted_tokens_are_not_split_into_fragments() {
        let segments = split_segments("Version v2.1.0 shipped. Path src/main.rs changed.");
        assert_eq!(
            segments,
            vec!["Version v2.1.0 shipped.", "Path src/main.rs changed."]
        );
    }

    #[test]
    fn overlap_is_unchanged_by_the_shared_tokenizer() {
        // Pins the metric the dedup and MMR stages consume: the refactor to one
        // tokenizer must not move a single value.
        assert_eq!(content_overlap("alpha beta gamma", "alpha beta delta"), 0.5);
        assert_eq!(content_overlap("alpha beta gamma", "alpha beta gamma"), 1.0);
        assert_eq!(content_overlap("alpha beta", "gamma delta"), 0.0);
        assert_eq!(content_overlap("a bb ccc dddd", "ccc dddd"), 1.0);
        assert_eq!(content_overlap("", ""), 0.0);
    }

    #[test]
    fn conflicting_versions_are_recognised_by_shared_subject_not_by_tags() {
        // Calibrated on the FactConsolidation `sh_6k` conflict groups (59/59) and
        // on the haystack's unrelated statements (~1%): two shared leading tokens
        // plus a 0.25 word overlap.
        assert!(looks_like_conflicting_versions(
            "goaltender is associated with the sport of ice hockey.",
            "goaltender is associated with the sport of pesäpallo."
        ));
        assert!(looks_like_conflicting_versions(
            "Sable is a citizen of United States of America.",
            "Sable is a citizen of Czech Republic."
        ));
        // Same template, different subject: not a conflict.
        assert!(!looks_like_conflicting_versions(
            "basketball was created in the country of United States of America.",
            "baseball was created in the country of Japan."
        ));
        // Unrelated memories.
        assert!(!looks_like_conflicting_versions(
            "The cache key changed in the search pipeline.",
            "Amy Winehouse died in the city of Camden Town."
        ));
    }
}
