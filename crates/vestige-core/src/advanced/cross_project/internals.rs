//! Private learner internals — pattern extraction and applicability scoring.
//!
//! Lives in its own file because these helpers grew large enough (>200 LOC)
//! to overshadow the public learner API in the parent module. Keeping the
//! split avoids a 778-line god-file while letting the helpers borrow the
//! same `Arc<RwLock<...>>` state via `&self`.

use std::collections::{HashMap, HashSet};

use chrono::Utc;

use super::CrossProjectLearner;
use super::types::{
    ApplicableKnowledge, CodePattern, MIN_PROJECTS_FOR_UNIVERSAL, MemoryForLearning,
    PatternCategory, PatternTrigger, ProjectContext, Suggestion, TriggerType, UniversalPattern,
    category_to_string,
};

impl CrossProjectLearner {
    pub(super) fn generate_suggestions(&self, context: &ProjectContext) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        let patterns = self
            .patterns
            .read()
            .map(|p| p.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        for pattern in patterns {
            if let Some(applicable) = self.check_pattern_applicability(&pattern, context) {
                for (i, suggestion_text) in applicable.suggestions.iter().enumerate() {
                    suggestions.push(Suggestion {
                        suggestion: suggestion_text.clone(),
                        based_on: pattern.pattern.name.clone(),
                        confidence: applicable.applicability_confidence,
                        evidence: applicable.supporting_memories.clone(),
                        priority: (10.0 * applicable.applicability_confidence) as u32 - i as u32,
                    });
                }
            }
        }

        suggestions.sort_by(|a, b| b.priority.cmp(&a.priority));
        suggestions
    }

    pub(super) fn check_pattern_applicability(
        &self,
        pattern: &UniversalPattern,
        context: &ProjectContext,
    ) -> Option<ApplicableKnowledge> {
        let mut match_scores: Vec<f64> = Vec::new();
        let mut match_reasons: Vec<String> = Vec::new();

        for trigger in &pattern.pattern.triggers {
            if let Some((matches, reason)) = self.check_trigger(trigger, context)
                && matches
            {
                match_scores.push(trigger.confidence);
                match_reasons.push(reason);
            }
        }

        if match_scores.is_empty() {
            return None;
        }

        let avg_confidence = match_scores.iter().sum::<f64>() / match_scores.len() as f64;
        // Boost confidence based on the pattern's track record; both
        // factors are in [0, 1] so the product collapses fast for
        // unproven patterns.
        let adjusted_confidence = avg_confidence * pattern.success_rate * pattern.confidence;

        if adjusted_confidence < 0.3 {
            return None;
        }

        let suggestions = self.generate_pattern_suggestions(pattern, context);

        Some(ApplicableKnowledge {
            pattern: pattern.clone(),
            match_reason: match_reasons.join("; "),
            applicability_confidence: adjusted_confidence,
            suggestions,
            // Filled from storage by the caller when needed.
            supporting_memories: Vec::new(),
        })
    }

    fn check_trigger(
        &self,
        trigger: &PatternTrigger,
        context: &ProjectContext,
    ) -> Option<(bool, String)> {
        match &trigger.trigger_type {
            TriggerType::FileName => {
                let matches = context
                    .file_types
                    .iter()
                    .any(|ft| ft.contains(&trigger.value));
                Some((matches, format!("Found {} files", trigger.value)))
            }
            TriggerType::Dependency => {
                let matches = context
                    .dependencies
                    .iter()
                    .any(|d| d.to_lowercase().contains(&trigger.value.to_lowercase()));
                Some((matches, format!("Uses {}", trigger.value)))
            }
            TriggerType::CodeConstruct => {
                // TODO: integrate with codebase scan once available.
                Some((false, String::new()))
            }
            TriggerType::DirectoryStructure => {
                let matches = context.structure.iter().any(|d| d.contains(&trigger.value));
                Some((matches, format!("Has {} directory", trigger.value)))
            }
            TriggerType::Topic | TriggerType::Intent | TriggerType::ErrorMessage => {
                // Checked against the live conversation; no pure-data signal here.
                Some((false, String::new()))
            }
        }
    }

    fn generate_pattern_suggestions(
        &self,
        pattern: &UniversalPattern,
        _context: &ProjectContext,
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        suggestions.push(format!(
            "Consider using: {} - {}",
            pattern.pattern.name, pattern.pattern.description
        ));

        for benefit in &pattern.pattern.benefits {
            suggestions.push(format!("This can help with: {}", benefit));
        }

        if let Some(example) = &pattern.pattern.example {
            suggestions.push(format!("Example: {}", example));
        }

        suggestions
    }

    pub(super) fn update_pattern_success_rate(&self, pattern_id: &str) {
        let (success_count, total_count) = {
            let Ok(outcomes) = self.outcomes.read() else {
                return;
            };

            let relevant: Vec<_> = outcomes
                .iter()
                .filter(|o| o.pattern_id == pattern_id)
                .collect();

            let success = relevant.iter().filter(|o| o.was_successful).count();
            (success, relevant.len())
        };

        if total_count == 0 {
            return;
        }

        let success_rate = success_count as f64 / total_count as f64;

        if let Ok(mut patterns) = self.patterns.write()
            && let Some(pattern) = patterns.get_mut(pattern_id)
        {
            pattern.success_rate = success_rate;
            pattern.application_count = total_count as u32;
        }
    }

    pub(super) fn extract_patterns_from_category(
        &self,
        category: PatternCategory,
        memories: &[&MemoryForLearning],
    ) {
        // Group memories by project so we can detect terms that recur
        // across project boundaries (the hallmark of a universal pattern).
        let mut by_project: HashMap<&str, Vec<&MemoryForLearning>> = HashMap::new();
        for memory in memories {
            by_project
                .entry(&memory.project_name)
                .or_default()
                .push(memory);
        }

        if by_project.len() < MIN_PROJECTS_FOR_UNIVERSAL {
            return;
        }

        // Crude bag-of-words approach: any token >5 chars seen in
        // multiple projects is a candidate. Good enough as a seed for
        // the human/agent reviewer; the success-rate loop will prune.
        let mut keyword_projects: HashMap<String, HashSet<&str>> = HashMap::new();

        for (project, project_memories) in &by_project {
            for memory in project_memories {
                for word in memory.content.split_whitespace() {
                    let clean = word
                        .trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase();
                    if clean.len() > 5 {
                        keyword_projects.entry(clean).or_default().insert(project);
                    }
                }
            }
        }

        for (keyword, projects) in keyword_projects {
            if projects.len() < MIN_PROJECTS_FOR_UNIVERSAL {
                continue;
            }
            let pattern_id = format!("auto-{}-{}", category_to_string(&category), keyword);

            if let Ok(mut patterns) = self.patterns.write()
                && !patterns.contains_key(&pattern_id)
            {
                patterns.insert(
                    pattern_id.clone(),
                    UniversalPattern {
                        id: pattern_id,
                        pattern: CodePattern {
                            name: format!("{} pattern", keyword),
                            category: category.clone(),
                            description: format!(
                                "Pattern involving '{}' observed in {} projects",
                                keyword,
                                projects.len()
                            ),
                            example: None,
                            triggers: vec![PatternTrigger {
                                trigger_type: TriggerType::Topic,
                                value: keyword.clone(),
                                confidence: 0.5,
                            }],
                            benefits: vec![],
                            considerations: vec![],
                        },
                        projects_seen_in: projects.iter().map(|s| s.to_string()).collect(),
                        // Default until validated by `record_pattern_outcome`.
                        success_rate: 0.5,
                        applicability: format!("When working with {}", keyword),
                        confidence: 0.5,
                        first_seen: Utc::now(),
                        last_seen: Utc::now(),
                        application_count: 0,
                    },
                );
            }
        }
    }
}
