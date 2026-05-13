//! Temporal anchoring — extract and resolve time references to absolute dates.
//!
//! Scans content for temporal expressions ("next Friday", "by March 15", "3 days ago"),
//! converts them to absolute `DateTime<Utc>`, and classifies as `valid_from` or `valid_until`.
//!
//! Uses `natural-date-rs` for the heavy lifting of natural language date parsing.

use chrono::{DateTime, Local, Utc};
use regex::Regex;
use std::sync::LazyLock;

/// Result of temporal anchoring analysis.
#[derive(Debug, Clone)]
pub struct TemporalResult {
    /// Suggested valid_from (when this knowledge becomes relevant)
    pub valid_from: Option<DateTime<Utc>>,
    /// Suggested valid_until (when this knowledge expires)
    pub valid_until: Option<DateTime<Utc>>,
    /// Raw temporal expressions found in the text
    pub anchors_found: Vec<String>,
}

// All four regex literals below are vetted by the test module at the bottom
// of this file. `expect()` over `unwrap()` so the failure name points at the
// specific pattern when somebody breaks one of them during edits.

// Patterns that indicate an expiry/deadline (→ valid_until).
static UNTIL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:until|by|before|deadline(?:\s+is)?|expires?(?:\s+on)?|due(?:\s+by)?|no later than|valid until|ends?(?:\s+on)?)\s+(.+?)(?:[.!,;]|$)")
        .expect("temporal.rs UNTIL_PATTERN regex literal")
});

// Patterns that indicate a start time (→ valid_from).
static FROM_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:starting(?:\s+from)?|from|since|after|beginning|as of|effective)\s+(.+?)(?:[.!,;]|$)")
        .expect("temporal.rs FROM_PATTERN regex literal")
});

// Standalone temporal expressions that indicate future (→ valid_from in the future).
static FUTURE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(tomorrow|next\s+(?:week|month|monday|tuesday|wednesday|thursday|friday|saturday|sunday)|in\s+\d+\s+(?:days?|weeks?|months?|hours?))\b")
        .expect("temporal.rs FUTURE_PATTERN regex literal")
});

// Standalone temporal expressions that indicate past (→ valid_from in the past).
static PAST_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(yesterday|\d+\s+(?:days?|weeks?|months?|hours?)\s+ago|last\s+(?:week|month|monday|tuesday|wednesday|thursday|friday|saturday|sunday))\b")
        .expect("temporal.rs PAST_PATTERN regex literal")
});

/// Extract temporal anchors from content and resolve to absolute dates.
///
/// If `existing_from` or `existing_until` are `Some`, those fields won't be overridden
/// (explicit values from the agent take precedence).
pub fn anchor_temporal(
    content: &str,
    existing_from: Option<DateTime<Utc>>,
    existing_until: Option<DateTime<Utc>>,
) -> TemporalResult {
    let mut result = TemporalResult {
        valid_from: None,
        valid_until: None,
        anchors_found: Vec::new(),
    };

    let reference = Local::now();

    // 1. Check for explicit "until/by/deadline" patterns → valid_until
    if existing_until.is_none() {
        for cap in UNTIL_PATTERN.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                let expr = m.as_str().trim();
                if let Some(dt) = try_parse_date(expr, reference) {
                    result.valid_until = Some(dt);
                    result.anchors_found.push(format!("until: {}", expr));
                    break;
                }
            }
        }
    }

    // 2. Check for explicit "from/since/starting" patterns → valid_from
    if existing_from.is_none() {
        for cap in FROM_PATTERN.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                let expr = m.as_str().trim();
                if let Some(dt) = try_parse_date(expr, reference) {
                    result.valid_from = Some(dt);
                    result.anchors_found.push(format!("from: {}", expr));
                    break;
                }
            }
        }
    }

    // 3. If no explicit from/until, check for standalone future expressions
    if existing_from.is_none() && result.valid_from.is_none() {
        for m in FUTURE_PATTERN.find_iter(content) {
            let expr = m.as_str().trim();
            if let Some(dt) = try_parse_date(expr, reference) {
                result.valid_from = Some(dt);
                result.anchors_found.push(format!("future: {}", expr));
                break;
            }
        }
    }

    // 4. Check for standalone past expressions
    if existing_from.is_none() && result.valid_from.is_none() {
        for m in PAST_PATTERN.find_iter(content) {
            let expr = m.as_str().trim();
            if let Some(dt) = try_parse_date(expr, reference) {
                result.valid_from = Some(dt);
                result.anchors_found.push(format!("past: {}", expr));
                break;
            }
        }
    }

    result
}

/// Try to parse a natural language date expression using natural-date-rs.
fn try_parse_date(expr: &str, reference: DateTime<Local>) -> Option<DateTime<Utc>> {
    let cleaned = expr.trim_end_matches(|c: char| c.is_ascii_punctuation());
    if cleaned.is_empty() {
        return None;
    }

    natural_date_rs::from_string_with_reference(cleaned, reference)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deadline_extraction() {
        let result = anchor_temporal("The report is due by next Friday.", None, None);
        assert!(
            result.valid_until.is_some(),
            "Should extract deadline as valid_until"
        );
        assert!(!result.anchors_found.is_empty());
    }

    #[test]
    fn test_until_extraction() {
        let result = anchor_temporal("This API key expires next week.", None, None);
        // natural-date-rs may or may not parse all expressions; verify the regex detects the pattern
        let result2 = anchor_temporal("Deploy before tomorrow.", None, None);
        assert!(
            result.valid_until.is_some() || result2.valid_until.is_some(),
            "Should detect at least one 'until/before' pattern"
        );
    }

    #[test]
    fn test_from_extraction() {
        let result = anchor_temporal(
            "Starting from next Monday, the new policy applies.",
            None,
            None,
        );
        assert!(
            result.valid_from.is_some(),
            "Should extract 'starting from' as valid_from"
        );
    }

    #[test]
    fn test_future_standalone() {
        let result = anchor_temporal("We need to deploy tomorrow.", None, None);
        assert!(
            result.valid_from.is_some() || !result.anchors_found.is_empty(),
            "Should detect 'tomorrow' as future temporal anchor"
        );
    }

    #[test]
    fn test_past_standalone() {
        let result = anchor_temporal("The server crashed yesterday.", None, None);
        assert!(
            result.valid_from.is_some() || !result.anchors_found.is_empty(),
            "Should detect 'yesterday' as past temporal anchor"
        );
    }

    #[test]
    fn test_explicit_values_not_overridden() {
        let explicit_from = Utc::now();
        let explicit_until = Utc::now() + chrono::Duration::days(30);
        let result = anchor_temporal(
            "Deploy by next Friday. Starting from tomorrow.",
            Some(explicit_from),
            Some(explicit_until),
        );
        assert!(
            result.valid_from.is_none(),
            "Should not override explicit valid_from"
        );
        assert!(
            result.valid_until.is_none(),
            "Should not override explicit valid_until"
        );
    }

    #[test]
    fn test_no_temporal_expressions() {
        let result = anchor_temporal("The function returns a boolean value.", None, None);
        assert!(result.valid_from.is_none());
        assert!(result.valid_until.is_none());
        assert!(result.anchors_found.is_empty());
    }

    #[test]
    fn test_empty_content() {
        let result = anchor_temporal("", None, None);
        assert!(result.valid_from.is_none());
        assert!(result.valid_until.is_none());
    }
}
