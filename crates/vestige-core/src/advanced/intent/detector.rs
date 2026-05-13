//! `IntentDetector` — scores user-action streams against intent patterns.

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

use chrono::{Duration, Utc};

use super::actions::{ActionType, UserAction};
use super::constants::{INTENT_WINDOW_MINUTES, MAX_ACTION_HISTORY, MIN_INTENT_CONFIDENCE};
use super::intent_kinds::{DetectedIntent, LearningLevel, MaintenanceType};
use super::result::{IntentDetectionResult, IntentMemoryQuery};

/// Intent detector that analyzes user actions
pub struct IntentDetector {
    /// Action history
    actions: Arc<RwLock<VecDeque<UserAction>>>,
    /// Intent patterns
    patterns: Vec<IntentPattern>,
}

/// A pattern that suggests a specific intent
#[allow(clippy::type_complexity)]
struct IntentPattern {
    /// Name of the pattern
    name: String,
    /// Function to score actions against this pattern
    scorer: Box<dyn Fn(&[&UserAction]) -> (DetectedIntent, f64) + Send + Sync>,
}

impl IntentDetector {
    /// Create a new intent detector
    pub fn new() -> Self {
        Self {
            actions: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_ACTION_HISTORY))),
            patterns: Self::build_patterns(),
        }
    }

    /// Record a user action
    pub fn record_action(&self, action: UserAction) {
        if let Ok(mut actions) = self.actions.write() {
            actions.push_back(action);

            // Trim old actions
            while actions.len() > MAX_ACTION_HISTORY {
                actions.pop_front();
            }
        }
    }

    /// Detect intent from recorded actions
    pub fn detect_intent(&self) -> IntentDetectionResult {
        let actions = self.get_recent_actions();

        if actions.is_empty() {
            return IntentDetectionResult {
                primary_intent: DetectedIntent::Unknown,
                confidence: 0.0,
                alternatives: vec![],
                evidence: vec![],
                detected_at: Utc::now(),
            };
        }

        // Score each pattern
        let mut scores: Vec<(DetectedIntent, f64, String)> = Vec::new();

        for pattern in &self.patterns {
            let action_refs: Vec<_> = actions.iter().collect();
            let (intent, score) = (pattern.scorer)(&action_refs);
            if score >= MIN_INTENT_CONFIDENCE {
                scores.push((intent, score, pattern.name.clone()));
            }
        }

        // Sort by score
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if scores.is_empty() {
            return IntentDetectionResult {
                primary_intent: DetectedIntent::Unknown,
                confidence: 0.0,
                alternatives: vec![],
                evidence: self.collect_evidence(&actions),
                detected_at: Utc::now(),
            };
        }

        let (primary_intent, confidence, _) = scores.remove(0);
        let alternatives: Vec<_> = scores
            .into_iter()
            .map(|(intent, score, _)| (intent, score))
            .take(3)
            .collect();

        IntentDetectionResult {
            primary_intent,
            confidence,
            alternatives,
            evidence: self.collect_evidence(&actions),
            detected_at: Utc::now(),
        }
    }

    /// Get memories relevant to detected intent
    pub fn memories_for_intent(&self, intent: &DetectedIntent) -> IntentMemoryQuery {
        let tags = intent.relevant_tags();

        IntentMemoryQuery {
            tags,
            keywords: self.extract_intent_keywords(intent),
            recency_boost: matches!(intent, DetectedIntent::Debugging { .. }),
        }
    }

    /// Clear action history
    pub fn clear_actions(&self) {
        if let Ok(mut actions) = self.actions.write() {
            actions.clear();
        }
    }

    /// Get action count
    pub fn action_count(&self) -> usize {
        self.actions.read().map(|a| a.len()).unwrap_or(0)
    }

    // ========================================================================
    // Private implementation
    // ========================================================================

    fn get_recent_actions(&self) -> Vec<UserAction> {
        let cutoff = Utc::now() - Duration::minutes(INTENT_WINDOW_MINUTES);

        self.actions
            .read()
            .map(|actions| {
                actions
                    .iter()
                    .filter(|a| a.timestamp > cutoff)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn build_patterns() -> Vec<IntentPattern> {
        vec![
            // Debugging pattern
            IntentPattern {
                name: "Debugging".to_string(),
                scorer: Box::new(|actions| {
                    let mut score: f64 = 0.0;
                    let mut symptoms = Vec::new();
                    let mut suspected_area = String::new();

                    for action in actions {
                        match &action.action_type {
                            ActionType::ErrorEncountered => {
                                score += 0.3;
                                if let Some(content) = &action.content {
                                    symptoms.push(content.clone());
                                }
                            }
                            ActionType::DebugStarted => score += 0.4,
                            ActionType::Search
                                if action
                                    .content
                                    .as_ref()
                                    .map(|c| c.to_lowercase())
                                    .map(|c| {
                                        c.contains("error")
                                            || c.contains("bug")
                                            || c.contains("fix")
                                    })
                                    .unwrap_or(false) =>
                            {
                                score += 0.2;
                            }
                            ActionType::FileOpened | ActionType::FileEdited => {
                                if let Some(file) = &action.file
                                    && let Some(name) = file.file_name()
                                {
                                    suspected_area = name.to_string_lossy().to_string();
                                }
                            }
                            _ => {}
                        }
                    }

                    let intent = DetectedIntent::Debugging {
                        suspected_area: if suspected_area.is_empty() {
                            "unknown".to_string()
                        } else {
                            suspected_area
                        },
                        symptoms,
                    };

                    (intent, score.min(1.0))
                }),
            },
            // Refactoring pattern
            IntentPattern {
                name: "Refactoring".to_string(),
                scorer: Box::new(|actions| {
                    let mut score: f64 = 0.0;
                    let mut target = String::new();

                    let edit_count = actions
                        .iter()
                        .filter(|a| a.action_type == ActionType::FileEdited)
                        .count();

                    // Multiple edits to related files suggests refactoring
                    if edit_count >= 3 {
                        score += 0.3;
                    }

                    for action in actions {
                        match &action.action_type {
                            ActionType::Search
                                if action
                                    .content
                                    .as_ref()
                                    .map(|c| c.to_lowercase())
                                    .map(|c| {
                                        c.contains("refactor")
                                            || c.contains("rename")
                                            || c.contains("extract")
                                    })
                                    .unwrap_or(false) =>
                            {
                                score += 0.3;
                            }
                            ActionType::FileEdited => {
                                if let Some(file) = &action.file {
                                    target = file.to_string_lossy().to_string();
                                }
                            }
                            _ => {}
                        }
                    }

                    let intent = DetectedIntent::Refactoring {
                        target: if target.is_empty() {
                            "code".to_string()
                        } else {
                            target
                        },
                        goal: "improve structure".to_string(),
                    };

                    (intent, score.min(1.0))
                }),
            },
            // Learning pattern
            IntentPattern {
                name: "Learning".to_string(),
                scorer: Box::new(|actions| {
                    let mut score: f64 = 0.0;
                    let mut topic = String::new();

                    for action in actions {
                        match &action.action_type {
                            ActionType::DocumentationViewed => {
                                score += 0.3;
                                if let Some(content) = &action.content {
                                    topic = content.clone();
                                }
                            }
                            ActionType::Search => {
                                if let Some(query) = &action.content {
                                    let lower = query.to_lowercase();
                                    if lower.contains("how to")
                                        || lower.contains("what is")
                                        || lower.contains("tutorial")
                                        || lower.contains("guide")
                                        || lower.contains("example")
                                    {
                                        score += 0.25;
                                        topic = query.clone();
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    let intent = DetectedIntent::Learning {
                        topic: if topic.is_empty() {
                            "unknown".to_string()
                        } else {
                            topic
                        },
                        level: LearningLevel::Intermediate,
                    };

                    (intent, score.min(1.0))
                }),
            },
            // New feature pattern
            IntentPattern {
                name: "NewFeature".to_string(),
                scorer: Box::new(|actions| {
                    let mut score: f64 = 0.0;
                    let mut description = String::new();
                    let mut components = Vec::new();

                    let created_count = actions
                        .iter()
                        .filter(|a| a.action_type == ActionType::FileCreated)
                        .count();

                    if created_count >= 1 {
                        score += 0.4;
                    }

                    for action in actions {
                        match &action.action_type {
                            ActionType::FileCreated => {
                                if let Some(file) = &action.file {
                                    description = file
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_default();
                                }
                            }
                            ActionType::FileOpened | ActionType::FileEdited => {
                                if let Some(file) = &action.file {
                                    components.push(file.to_string_lossy().to_string());
                                }
                            }
                            _ => {}
                        }
                    }

                    let intent = DetectedIntent::NewFeature {
                        feature_description: if description.is_empty() {
                            "new feature".to_string()
                        } else {
                            description
                        },
                        related_components: components,
                    };

                    (intent, score.min(1.0))
                }),
            },
            // Maintenance pattern
            IntentPattern {
                name: "Maintenance".to_string(),
                scorer: Box::new(|actions| {
                    let mut score: f64 = 0.0;
                    let mut maint_type = MaintenanceType::Cleanup;
                    let mut target = None;

                    for action in actions {
                        match &action.action_type {
                            ActionType::CommandExecuted => {
                                if let Some(cmd) = &action.content {
                                    let lower = cmd.to_lowercase();
                                    if lower.contains("upgrade")
                                        || lower.contains("update")
                                        || lower.contains("npm")
                                        || lower.contains("cargo update")
                                    {
                                        score += 0.4;
                                        maint_type = MaintenanceType::DependencyUpdate;
                                    }
                                }
                            }
                            ActionType::FileEdited => {
                                if let Some(file) = &action.file {
                                    let name = file
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_lowercase())
                                        .unwrap_or_default();

                                    if name.contains("config")
                                        || name == "cargo.toml"
                                        || name == "package.json"
                                    {
                                        score += 0.2;
                                        maint_type = MaintenanceType::Configuration;
                                        target = Some(name);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    let intent = DetectedIntent::Maintenance {
                        maintenance_type: maint_type,
                        target,
                    };

                    (intent, score.min(1.0))
                }),
            },
        ]
    }

    fn collect_evidence(&self, actions: &[UserAction]) -> Vec<String> {
        actions
            .iter()
            .take(5)
            .map(|a| match &a.action_type {
                ActionType::FileOpened | ActionType::FileEdited => {
                    format!(
                        "{:?}: {}",
                        a.action_type,
                        a.file
                            .as_ref()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default()
                    )
                }
                ActionType::Search => {
                    format!("Searched: {}", a.content.as_ref().unwrap_or(&String::new()))
                }
                ActionType::ErrorEncountered => {
                    format!("Error: {}", a.content.as_ref().unwrap_or(&String::new()))
                }
                _ => format!("{:?}", a.action_type),
            })
            .collect()
    }

    fn extract_intent_keywords(&self, intent: &DetectedIntent) -> Vec<String> {
        match intent {
            DetectedIntent::Debugging {
                suspected_area,
                symptoms,
            } => {
                let mut keywords = vec![suspected_area.clone()];
                keywords.extend(symptoms.iter().take(3).cloned());
                keywords
            }
            DetectedIntent::Refactoring { target, goal } => {
                vec![target.clone(), goal.clone()]
            }
            DetectedIntent::NewFeature {
                feature_description,
                related_components,
            } => {
                let mut keywords = vec![feature_description.clone()];
                keywords.extend(related_components.iter().take(3).cloned());
                keywords
            }
            DetectedIntent::Learning { topic, .. } => vec![topic.clone()],
            DetectedIntent::Integration { system } => vec![system.clone()],
            _ => vec![],
        }
    }
}

impl Default for IntentDetector {
    fn default() -> Self {
        Self::new()
    }
}
