//! Core data types — predicted memories, prediction reasons, query and
//! temporal patterns, and the session/project context structs.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};

use super::{INTEREST_LEARNING_RATE, RECENT_QUERY_WINDOW, SESSION_WINDOW_MINUTES};

// ============================================================================
// CORE TYPES
// ============================================================================

/// A predicted memory that the user is likely to need
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedMemory {
    /// The memory ID predicted to be needed
    pub memory_id: String,
    /// Content preview for quick reference
    pub content_preview: String,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// Human-readable reasoning for this prediction
    pub reasoning: PredictionReason,
    /// When this prediction was made
    pub predicted_at: DateTime<Utc>,
    /// Tags associated with this memory
    pub tags: Vec<String>,
}

/// Reasons why a memory was predicted to be needed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredictionReason {
    /// Based on learned user interests
    InterestBased {
        /// Topic that matched
        topic: String,
        /// Interest weight for this topic
        weight: f64,
    },
    /// Based on recent query patterns
    QueryPattern {
        /// Related query that triggered prediction
        related_query: String,
        /// How often this pattern occurred
        frequency: u32,
    },
    /// Based on temporal patterns (time of day, day of week)
    TemporalPattern {
        /// Description of the temporal pattern
        pattern_description: String,
        /// Historical accuracy of this pattern
        historical_accuracy: f64,
    },
    /// Based on current session context
    SessionContext {
        /// What in the session triggered this
        trigger: String,
        /// Semantic similarity to session content
        similarity: f64,
    },
    /// Based on co-access patterns (memories accessed together)
    CoAccess {
        /// The memory that triggered this prediction
        trigger_memory: String,
        /// How often these are accessed together
        co_occurrence_rate: f64,
    },
    /// Prediction based on semantic similarity
    SemanticSimilarity {
        /// Query or content that was semantically similar
        similar_to: String,
        /// Similarity score
        similarity: f64,
    },
}

impl PredictionReason {
    /// Get a human-readable description of the prediction reason
    pub fn description(&self) -> String {
        match self {
            Self::InterestBased { topic, weight } => {
                format!(
                    "Based on your interest in {} ({}% interest weight)",
                    topic,
                    (weight * 100.0) as u32
                )
            }
            Self::QueryPattern {
                related_query,
                frequency,
            } => {
                format!(
                    "You've searched for similar topics {} times (related: \"{}\")",
                    frequency, related_query
                )
            }
            Self::TemporalPattern {
                pattern_description,
                historical_accuracy,
            } => {
                format!(
                    "{} ({}% historical accuracy)",
                    pattern_description,
                    (historical_accuracy * 100.0) as u32
                )
            }
            Self::SessionContext {
                trigger,
                similarity,
            } => {
                format!(
                    "Relevant to your current session: {} ({}% match)",
                    trigger,
                    (similarity * 100.0) as u32
                )
            }
            Self::CoAccess {
                trigger_memory,
                co_occurrence_rate,
            } => {
                format!(
                    "Often accessed with {} ({}% of the time)",
                    trigger_memory,
                    (co_occurrence_rate * 100.0) as u32
                )
            }
            Self::SemanticSimilarity {
                similar_to,
                similarity,
            } => {
                format!(
                    "Semantically similar to \"{}\" ({}% similarity)",
                    similar_to,
                    (similarity * 100.0) as u32
                )
            }
        }
    }
}

/// Outcome of a prediction (for learning)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionOutcome {
    /// The predicted memory ID
    pub memory_id: String,
    /// The prediction confidence
    pub confidence: f64,
    /// Whether the prediction was used/helpful
    pub was_useful: bool,
    /// Time between prediction and actual use (if used)
    pub time_to_use: Option<Duration>,
    /// When this outcome was recorded
    pub recorded_at: DateTime<Utc>,
}

/// A pattern detected in user queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPattern {
    /// The query content
    pub query: String,
    /// Tags associated with this query
    pub tags: Vec<String>,
    /// When this query was made
    pub timestamp: DateTime<Utc>,
    /// Results that were accessed after this query
    pub accessed_results: Vec<String>,
    /// Whether the user found what they were looking for
    pub was_satisfied: Option<bool>,
}

/// Temporal patterns in user behavior
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemporalPatterns {
    /// Hour of day preferences (0-23) -> topic -> weight
    pub hourly_patterns: HashMap<u32, HashMap<String, f64>>,
    /// Day of week preferences -> topic -> weight
    pub daily_patterns: HashMap<String, HashMap<String, f64>>,
    /// Monthly patterns (for seasonal interests)
    pub monthly_patterns: HashMap<u32, HashMap<String, f64>>,
    /// Activity level by hour (for determining engagement periods)
    pub activity_by_hour: [f64; 24],
}

impl TemporalPatterns {
    /// Create new empty temporal patterns
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the most active hour of the day
    pub fn peak_activity_hour(&self) -> u32 {
        self.activity_by_hour
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i as u32)
            .unwrap_or(10) // Default to 10 AM
    }

    /// Get topics relevant for the current time
    pub fn topics_for_time(&self, time: DateTime<Utc>) -> Vec<(String, f64)> {
        let hour = time.hour();
        let mut topics = Vec::new();

        if let Some(hour_topics) = self.hourly_patterns.get(&hour) {
            for (topic, weight) in hour_topics {
                topics.push((topic.clone(), *weight));
            }
        }

        let weekday = match time.weekday() {
            Weekday::Mon => "monday",
            Weekday::Tue => "tuesday",
            Weekday::Wed => "wednesday",
            Weekday::Thu => "thursday",
            Weekday::Fri => "friday",
            Weekday::Sat => "saturday",
            Weekday::Sun => "sunday",
        };

        if let Some(day_topics) = self.daily_patterns.get(weekday) {
            for (topic, weight) in day_topics {
                // Combine if already exists
                if let Some(existing) = topics.iter_mut().find(|(t, _)| t == topic) {
                    existing.1 = (existing.1 + weight) / 2.0;
                } else {
                    topics.push((topic.clone(), *weight));
                }
            }
        }

        topics.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        topics
    }

    /// Record activity at a specific time
    pub fn record_activity(&mut self, time: DateTime<Utc>, topic: &str, weight: f64) {
        let hour = time.hour();

        // Update hourly activity
        self.activity_by_hour[hour as usize] = self.activity_by_hour[hour as usize] * 0.9 + 0.1;

        // Update hourly topic patterns
        let hour_topics = self.hourly_patterns.entry(hour).or_default();
        let current = hour_topics.entry(topic.to_string()).or_insert(0.0);
        *current = *current * (1.0 - INTEREST_LEARNING_RATE) + weight * INTEREST_LEARNING_RATE;

        // Update daily patterns
        let weekday = match time.weekday() {
            Weekday::Mon => "monday",
            Weekday::Tue => "tuesday",
            Weekday::Wed => "wednesday",
            Weekday::Thu => "thursday",
            Weekday::Fri => "friday",
            Weekday::Sat => "saturday",
            Weekday::Sun => "sunday",
        };

        let day_topics = self.daily_patterns.entry(weekday.to_string()).or_default();
        let current = day_topics.entry(topic.to_string()).or_insert(0.0);
        *current = *current * (1.0 - INTEREST_LEARNING_RATE) + weight * INTEREST_LEARNING_RATE;

        // Update monthly patterns
        let month = time.month();
        let month_topics = self.monthly_patterns.entry(month).or_default();
        let current = month_topics.entry(topic.to_string()).or_insert(0.0);
        *current = *current * (1.0 - INTEREST_LEARNING_RATE) + weight * INTEREST_LEARNING_RATE;
    }
}

/// Current session context for predictions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionContext {
    /// When the session started
    pub started_at: DateTime<Utc>,
    /// Current working topic/focus
    pub current_focus: Option<String>,
    /// Files currently being worked on
    pub active_files: Vec<String>,
    /// Recent memory accesses in this session
    pub accessed_memories: Vec<String>,
    /// Recent queries in this session
    pub recent_queries: Vec<String>,
    /// Detected intent (if any)
    pub detected_intent: Option<String>,
    /// Project context (if any)
    pub project_context: Option<ProjectContext>,
}

impl SessionContext {
    /// Create a new session context
    pub fn new() -> Self {
        Self {
            started_at: Utc::now(),
            ..Default::default()
        }
    }

    /// Get session duration
    pub fn duration(&self) -> Duration {
        Utc::now() - self.started_at
    }

    /// Check if session is still active (within window)
    pub fn is_active(&self) -> bool {
        self.duration() < Duration::minutes(SESSION_WINDOW_MINUTES)
    }

    /// Add a file to active files
    pub fn add_active_file(&mut self, file: String) {
        if !self.active_files.contains(&file) {
            self.active_files.push(file);
        }
    }

    /// Add an accessed memory
    pub fn add_accessed_memory(&mut self, memory_id: String) {
        if !self.accessed_memories.contains(&memory_id) {
            self.accessed_memories.push(memory_id);
        }
    }

    /// Add a recent query
    pub fn add_query(&mut self, query: String) {
        self.recent_queries.push(query);
        // Keep only recent queries
        if self.recent_queries.len() > RECENT_QUERY_WINDOW {
            self.recent_queries.remove(0);
        }
    }
}

/// Project context for predictions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Project name
    pub name: String,
    /// Project path
    pub path: String,
    /// Detected frameworks/technologies
    pub technologies: Vec<String>,
    /// Primary programming language
    pub primary_language: Option<String>,
}
