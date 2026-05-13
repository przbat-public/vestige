//! Compound-content detector that nudges agents toward atomic memories.

/// Detects content that likely contains multiple distinct topics and should be split
/// into atomic memories. Returns a warning message if compound content is detected.
pub(super) fn detect_compound_content(content: &str) -> Option<String> {
    let len = content.len();
    if len < 300 {
        return None;
    }

    let mut signals: Vec<&str> = Vec::new();

    let lines: Vec<&str> = content.lines().collect();
    let paragraph_count = content
        .split("\n\n")
        .filter(|p| p.trim().len() > 30)
        .count();
    if paragraph_count >= 3 {
        signals.push("multiple paragraphs covering different topics");
    }

    let speaker_pattern_count = lines
        .iter()
        .filter(|l| {
            let trimmed = l.trim();
            // "Speaker: text" or "Speaker Name: text"
            if let Some(colon_pos) = trimmed.find(':') {
                let before_colon = &trimmed[..colon_pos];
                colon_pos < 40
                    && !before_colon.is_empty()
                    && before_colon
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
                    && trimmed.len() > colon_pos + 5
            } else {
                false
            }
        })
        .count();
    if speaker_pattern_count >= 3 {
        signals.push("conversation transcript (multiple 'Speaker: text' lines)");
    }

    let bullet_count = lines
        .iter()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("- ")
                || t.starts_with("* ")
                || t.starts_with("• ")
                || (t.len() > 3
                    && t.chars().next().is_some_and(|c| c.is_ascii_digit())
                    && (t.contains(". ") || t.contains(") ")))
        })
        .count();
    if bullet_count >= 4 {
        signals.push("bulleted list with multiple distinct items");
    }

    let topic_shift_indicators = [
        "also,",
        "additionally,",
        "on another note",
        "separately,",
        "moving on",
        "another thing",
        "by the way",
        "btw,",
        "oh and",
        "also worth noting",
        "furthermore,",
        "in other news",
        "on a different topic",
    ];
    let topic_shifts = lines
        .iter()
        .filter(|l| {
            let lower = l.to_lowercase();
            topic_shift_indicators.iter().any(|ind| lower.contains(ind))
        })
        .count();
    if topic_shifts >= 2 {
        signals.push("topic-shift phrases detected");
    }

    if signals.is_empty() {
        return None;
    }

    let reason = signals.join("; ");
    Some(format!(
        "⚠️ COMPOUND CONTENT DETECTED: This memory contains {}. \
         For better search recall, split into separate atomic memories using batch mode. \
         Each memory should contain ONE fact, decision, or event. \
         Example: instead of one memory with 5 bullet points, use `items` array with 5 separate entries.",
        reason
    ))
}
