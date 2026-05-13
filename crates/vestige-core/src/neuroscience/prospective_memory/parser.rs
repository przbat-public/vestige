//! Heuristic natural-language parser that turns "remind me to X when Y"
//! into structured [`Intention`] values.

use std::collections::HashMap;

use chrono::Duration;

use super::error::{ProspectiveMemoryError, Result};
use super::intention::{Intention, IntentionSource};
use super::triggers::{IntentionTrigger, Priority, TriggerPattern};

// ============================================================================
// NATURAL LANGUAGE PARSING
// ============================================================================

/// Parser for natural language intentions
pub struct IntentionParser {
    /// Time-related keywords
    time_keywords: HashMap<String, Duration>,
}

impl IntentionParser {
    /// Create a new intention parser
    pub fn new() -> Self {
        let mut time_keywords = HashMap::new();

        // Duration keywords
        time_keywords.insert("in a minute".to_string(), Duration::minutes(1));
        time_keywords.insert("in 5 minutes".to_string(), Duration::minutes(5));
        time_keywords.insert("in 10 minutes".to_string(), Duration::minutes(10));
        time_keywords.insert("in 15 minutes".to_string(), Duration::minutes(15));
        time_keywords.insert("in 30 minutes".to_string(), Duration::minutes(30));
        time_keywords.insert("in an hour".to_string(), Duration::hours(1));
        time_keywords.insert("in 2 hours".to_string(), Duration::hours(2));
        time_keywords.insert("tomorrow".to_string(), Duration::hours(24));
        time_keywords.insert("next week".to_string(), Duration::days(7));

        Self { time_keywords }
    }

    /// Parse a natural language intention
    pub fn parse(&self, text: &str) -> Result<Intention> {
        let text_lower = text.to_lowercase();

        // Detect trigger type and extract content
        let (trigger, content) = self.extract_trigger_and_content(&text_lower, text)?;

        let mut intention = Intention::new(content, trigger);
        intention.source = IntentionSource::NaturalLanguage {
            original_text: text.to_string(),
            confidence: 0.7, // Base confidence for pattern matching
        };

        // Detect priority from keywords
        if text_lower.contains("urgent")
            || text_lower.contains("important")
            || text_lower.contains("critical")
            || text_lower.contains("asap")
        {
            intention.priority = Priority::High;
        }

        Ok(intention)
    }

    /// Extract trigger and content from text
    fn extract_trigger_and_content(
        &self,
        text_lower: &str,
        original: &str,
    ) -> Result<(IntentionTrigger, String)> {
        // Check for "remind me to X when Y" pattern
        if let Some(when_byte_idx) = text_lower.find(" when ") {
            // Convert byte index to char index for safe slicing
            let when_char_idx = text_lower[..when_byte_idx].chars().count();

            let content_part: String = if text_lower.starts_with("remind me to ") {
                original
                    .chars()
                    .skip(13)
                    .take(when_char_idx.saturating_sub(13))
                    .collect()
            } else if text_lower.starts_with("remind me ") {
                original
                    .chars()
                    .skip(10)
                    .take(when_char_idx.saturating_sub(10))
                    .collect()
            } else {
                original.chars().take(when_char_idx).collect()
            };

            let condition_part: String = original.chars().skip(when_char_idx + 6).collect();

            return Ok((
                IntentionTrigger::EventBased {
                    condition: condition_part.clone(),
                    pattern: TriggerPattern::contains(&condition_part),
                },
                content_part,
            ));
        }

        // Check for time-based patterns
        for (keyword, duration) in &self.time_keywords {
            if text_lower.contains(keyword) {
                let content = self.extract_content(text_lower, original, keyword);
                return Ok((IntentionTrigger::after_duration(*duration), content));
            }
        }

        // Check for "at X" time pattern
        if text_lower.contains(" at ") {
            // For now, treat as a simple event trigger
            let parts: Vec<&str> = original.splitn(2, " at ").collect();
            if parts.len() == 2 {
                let part0_lower = parts[0].to_lowercase();
                let content: String = if part0_lower.starts_with("remind me to ") {
                    parts[0].chars().skip(13).collect()
                } else if part0_lower.starts_with("remind me ") {
                    parts[0].chars().skip(10).collect()
                } else {
                    parts[0].to_string()
                };

                // Try to parse time (simplified - just use duration for now)
                return Ok((
                    IntentionTrigger::after_duration(Duration::hours(1)),
                    content,
                ));
            }
        }

        // Check for implicit intentions ("I should tell Sarah about this")
        if text_lower.starts_with("i should ")
            || text_lower.starts_with("i need to ")
            || text_lower.starts_with("don't forget to ")
            || text_lower.starts_with("remember to ")
        {
            // Use char-aware slicing to avoid UTF-8 boundary issues
            let content: String = if text_lower.starts_with("i should ") {
                original.chars().skip(9).collect()
            } else if text_lower.starts_with("i need to ") {
                original.chars().skip(10).collect()
            } else if text_lower.starts_with("don't forget to ") {
                original.chars().skip(16).collect()
            } else {
                original.chars().skip(12).collect()
            };

            // Extract entity if mentioned
            if let Some(entity) = self.extract_entity(text_lower) {
                return Ok((
                    IntentionTrigger::EventBased {
                        condition: format!("Meeting or conversation with {}", entity),
                        pattern: TriggerPattern::contains(&entity),
                    },
                    content,
                ));
            }

            // Default to time-based
            return Ok((
                IntentionTrigger::after_duration(Duration::hours(1)),
                content,
            ));
        }

        // Default fallback
        Err(ProspectiveMemoryError::ParseError(
            "Could not parse intention from text".to_string(),
        ))
    }

    /// Extract content from text, removing trigger keywords
    fn extract_content(&self, _text_lower: &str, original: &str, keyword: &str) -> String {
        original
            .replace(keyword, "")
            .replace(&keyword.to_uppercase(), "")
            .replace("remind me to ", "")
            .replace("Remind me to ", "")
            .replace("remind me ", "")
            .replace("Remind me ", "")
            .trim()
            .to_string()
    }

    /// Extract entity names from text
    fn extract_entity(&self, text_lower: &str) -> Option<String> {
        // Simple pattern: look for "tell X about" or "ask X about" or "with X"
        let patterns = ["tell ", "ask ", "with ", "to "];

        for pattern in patterns {
            if let Some(idx) = text_lower.find(pattern) {
                let after = &text_lower[idx + pattern.len()..];
                // Get first word as entity
                if let Some(space_idx) = after.find(' ') {
                    let entity = &after[..space_idx];
                    if !["the", "a", "an", "about", "to", "for"].contains(&entity) {
                        return Some(entity.to_string());
                    }
                }
            }
        }

        None
    }
}

impl Default for IntentionParser {
    fn default() -> Self {
        Self::new()
    }
}
