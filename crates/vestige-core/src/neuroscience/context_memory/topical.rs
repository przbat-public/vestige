//! Topical context: active topics, recent queries, and shared lexical signal.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

// ============================================================================
// TOPICAL CONTEXT
// ============================================================================

/// Topical context captures WHAT topics were active during encoding
///
/// This is the cognitive context - what the user was thinking about,
/// what topics were being discussed, and what the recent query history was.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicalContext {
    /// Currently active topics (extracted from recent interactions)
    pub active_topics: Vec<String>,
    /// Recent queries (for query-based context matching)
    pub recent_queries: Vec<String>,
    /// Current conversation thread ID (if applicable)
    pub conversation_thread: Option<String>,
    /// Keywords extracted from the current context
    pub keywords: Vec<String>,
    /// Tags that were active at encoding time
    pub active_tags: Vec<String>,
}

impl TopicalContext {
    /// Create a new topical context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with active topics
    pub fn with_topics(topics: Vec<String>) -> Self {
        Self {
            active_topics: topics,
            ..Default::default()
        }
    }

    /// Add a topic to the context
    pub fn add_topic(&mut self, topic: impl Into<String>) {
        let topic = topic.into();
        if !self.active_topics.contains(&topic) {
            self.active_topics.push(topic);
        }
    }

    /// Add a recent query
    pub fn add_query(&mut self, query: impl Into<String>) {
        self.recent_queries.push(query.into());
        // Keep only the last 10 queries
        if self.recent_queries.len() > 10 {
            self.recent_queries.remove(0);
        }
    }

    /// Set the conversation thread
    pub fn set_thread(&mut self, thread_id: impl Into<String>) {
        self.conversation_thread = Some(thread_id.into());
    }

    /// Extract keywords from text and add them
    pub fn extract_keywords_from(&mut self, text: &str) {
        // Simple keyword extraction (in production, use NLP)
        let words: Vec<String> = text
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(|w| w.to_lowercase())
            .filter(|w| !is_stop_word(w))
            .collect();

        for word in words {
            if !self.keywords.contains(&word) {
                self.keywords.push(word);
            }
        }

        // Keep only top 20 keywords
        self.keywords.truncate(20);
    }

    /// Get all context terms (topics + keywords + tags)
    pub fn all_terms(&self) -> HashSet<String> {
        let mut terms = HashSet::new();
        terms.extend(self.active_topics.iter().cloned());
        terms.extend(self.keywords.iter().cloned());
        terms.extend(self.active_tags.iter().cloned());
        terms
    }
}

/// Simple stop word check (expand for production)
fn is_stop_word(word: &str) -> bool {
    const STOP_WORDS: &[&str] = &[
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
        "from", "as", "is", "was", "are", "were", "been", "be", "have", "has", "had", "do", "does",
        "did", "will", "would", "could", "should", "may", "might", "must", "shall", "can", "this",
        "that", "these", "those", "it", "its", "they", "them", "their", "we", "our", "you", "your",
        "he", "she", "his", "her", "what", "which", "who", "whom", "when", "where", "why", "how",
    ];
    STOP_WORDS.contains(&word)
}
