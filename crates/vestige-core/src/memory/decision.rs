//! Structured decision payload (Proposal C — Decision Matrix).
//!
//! When an agent records a non-trivial decision through
//! `codebase.remember_decision_v2`, we don't want to lose the structure
//! by flattening it into a Markdown blob. The agent provides:
//!
//! - A **question** the decision answered ("Which logging library?")
//! - A list of **choices** (with chosen one marked)
//! - A list of **criteria** the decision was scored against
//! - A score matrix (criterion → choice → 1–5)
//! - Optional **valid_until** so we can flag the decision as stale
//!   automatically when the date passes
//!
//! This struct lives on `KnowledgeNode.extra_json` keyed under
//! `"decision"` so the rest of the storage layer doesn't need to know
//! about it. The reflect engine and the `/api/decisions` endpoint read
//! it back; everything else treats the node like any other Decision.
//!
//! Backward compatibility: the legacy `remember_decision` tool still
//! exists and stores plain Markdown — those memories simply have no
//! `extra_json.decision` and the new endpoints skip them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One alternative the agent considered.
///
/// `chosen` is `true` for exactly one entry — the validator below
/// enforces that. The dashboard renders the chosen entry highlighted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    /// Stable identifier within the payload. Free-form (`"postgres"`,
    /// `"option-a"`) — used as a join key in the score matrix and as
    /// the React `key` prop.
    pub id: String,
    /// Human-readable label rendered in the UI.
    pub label: String,
    /// One short sentence describing what makes this choice distinctive.
    /// Optional; some payloads have only a label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Marks the choice that was selected. Exactly one `Choice` in a
    /// `DecisionPayload.choices` must have `chosen = true`.
    #[serde(default)]
    pub chosen: bool,
}

/// One axis the decision was evaluated against.
///
/// `weight` defaults to `1.0` — the dashboard uses it to bias the
/// radar/bar visualisation toward what the agent cared about most.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Criterion {
    /// Stable identifier — used as a join key in the score matrix.
    pub id: String,
    /// Human-readable label ("Operational complexity", "Cost").
    pub label: String,
    /// Optional weight (≥ 0). Defaults to `1.0`. Negative weights are
    /// rejected by `validate()` because they would invert the radar
    /// chart silently.
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_weight() -> f32 {
    1.0
}

/// Structured decision payload stored as `extra_json.decision`.
///
/// **Validation invariants** (enforced by [`Self::validate`]):
/// - `choices.len() >= 2` — a one-option "decision" is just a fact
/// - Exactly one `chosen == true`
/// - Every `score_matrix` key must reference an existing `criterion.id`
/// - Every score row must only mention existing `choice.id`s
/// - Score values must be in `1..=5`
/// - `criterion.weight` must be `>= 0`
///
/// The validator runs in the MCP tool before any storage call, so by
/// the time a payload lands in `extra_json` it is structurally sound.
/// The dashboard re-parses the column on read and silently drops
/// payloads that fail to deserialize (defensive against schema drift).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DecisionPayload {
    /// What was being decided. Becomes the card title in the dashboard.
    pub question: String,
    /// Why the chosen option won — free-form prose, rendered as the
    /// expanded view body.
    pub rationale: String,
    pub choices: Vec<Choice>,
    pub criteria: Vec<Criterion>,
    /// Sparse 1–5 score matrix: `score_matrix[criterion_id][choice_id]`.
    /// Missing cells render as "—" in the UI rather than as zero — an
    /// empty cell means "not scored", not "scored zero".
    #[serde(default)]
    pub score_matrix: std::collections::HashMap<String, std::collections::HashMap<String, u8>>,
    /// Stale-detection helper. When `valid_until < now`, the reflect
    /// engine flags this decision under `staleDecisions` with reason
    /// `"expired_explicit"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    /// IDs of decisions this one supersedes. Used by the dashboard to
    /// build a "previously: X" link chain. Optional.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supersedes: Vec<String>,
}

impl DecisionPayload {
    /// Returns `Ok(())` only when the payload satisfies every
    /// structural invariant. Called by the MCP tool *before* we
    /// serialize to `extra_json` so malformed payloads never reach
    /// storage.
    pub fn validate(&self) -> Result<(), String> {
        if self.question.trim().is_empty() {
            return Err("question must be non-empty".into());
        }
        if self.choices.len() < 2 {
            return Err(format!(
                "need at least 2 choices, got {}",
                self.choices.len()
            ));
        }
        let chosen_count = self.choices.iter().filter(|c| c.chosen).count();
        if chosen_count != 1 {
            return Err(format!(
                "exactly one choice must be marked chosen, found {}",
                chosen_count
            ));
        }

        let mut seen_choice_ids = std::collections::HashSet::new();
        for choice in &self.choices {
            if choice.id.trim().is_empty() {
                return Err("choice.id must be non-empty".into());
            }
            if !seen_choice_ids.insert(&choice.id) {
                return Err(format!("duplicate choice.id {}", choice.id));
            }
        }

        let mut seen_criterion_ids = std::collections::HashSet::new();
        for criterion in &self.criteria {
            if criterion.id.trim().is_empty() {
                return Err("criterion.id must be non-empty".into());
            }
            if !seen_criterion_ids.insert(&criterion.id) {
                return Err(format!("duplicate criterion.id {}", criterion.id));
            }
            if criterion.weight < 0.0 || !criterion.weight.is_finite() {
                return Err(format!(
                    "criterion {} has invalid weight {}",
                    criterion.id, criterion.weight
                ));
            }
        }

        let choice_ids: std::collections::HashSet<&String> =
            self.choices.iter().map(|c| &c.id).collect();
        let criterion_ids: std::collections::HashSet<&String> =
            self.criteria.iter().map(|c| &c.id).collect();

        for (crit_id, row) in &self.score_matrix {
            if !criterion_ids.contains(crit_id) {
                return Err(format!(
                    "score_matrix references unknown criterion {}",
                    crit_id
                ));
            }
            for (ch_id, score) in row {
                if !choice_ids.contains(ch_id) {
                    return Err(format!(
                        "score_matrix.{} references unknown choice {}",
                        crit_id, ch_id
                    ));
                }
                if !(1..=5).contains(score) {
                    return Err(format!(
                        "score_matrix.{}.{} = {} (must be 1..=5)",
                        crit_id, ch_id, score
                    ));
                }
            }
        }

        Ok(())
    }

    /// True when an explicit `valid_until` has elapsed. Used by the
    /// reflect engine to flag decisions whose author told us upfront
    /// they have a shelf life.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.valid_until.is_some_and(|until| until < now)
    }

    /// Pull the `chosen` choice. None if the payload is malformed
    /// (validator should have caught that on ingest).
    pub fn chosen_choice(&self) -> Option<&Choice> {
        self.choices.iter().find(|c| c.chosen)
    }
}

/// Convenience: pull a `DecisionPayload` out of a node's
/// `extra_json.decision` slot. Returns `None` when the column is
/// missing, when it doesn't contain a `decision` key, or when the
/// payload fails to deserialize. Drift-tolerant by design — a single
/// corrupted row mustn't take down `/api/decisions`.
pub fn extract_decision(extra_json: Option<&serde_json::Value>) -> Option<DecisionPayload> {
    let extra = extra_json?;
    let raw = extra.get("decision")?.clone();
    serde_json::from_value::<DecisionPayload>(raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn fixture_valid() -> DecisionPayload {
        let mut score_matrix = std::collections::HashMap::new();
        let mut perf_row = std::collections::HashMap::new();
        perf_row.insert("postgres".to_string(), 4);
        perf_row.insert("mysql".to_string(), 3);
        score_matrix.insert("performance".to_string(), perf_row);

        DecisionPayload {
            question: "Which database?".into(),
            rationale: "Postgres wins on JSONB support.".into(),
            choices: vec![
                Choice {
                    id: "postgres".into(),
                    label: "PostgreSQL".into(),
                    summary: Some("Native JSONB, mature".into()),
                    chosen: true,
                },
                Choice {
                    id: "mysql".into(),
                    label: "MySQL".into(),
                    summary: None,
                    chosen: false,
                },
            ],
            criteria: vec![Criterion {
                id: "performance".into(),
                label: "Read performance".into(),
                weight: 1.5,
            }],
            score_matrix,
            valid_until: None,
            supersedes: vec![],
        }
    }

    #[test]
    fn validate_accepts_well_formed_payload() {
        let payload = fixture_valid();
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_or_one_choice() {
        let mut p = fixture_valid();
        p.choices.pop();
        let err = p.validate().unwrap_err();
        assert!(err.contains("at least 2"));
    }

    #[test]
    fn validate_rejects_no_chosen() {
        let mut p = fixture_valid();
        for c in &mut p.choices {
            c.chosen = false;
        }
        let err = p.validate().unwrap_err();
        assert!(err.contains("exactly one choice"));
    }

    #[test]
    fn validate_rejects_two_chosen() {
        let mut p = fixture_valid();
        for c in &mut p.choices {
            c.chosen = true;
        }
        let err = p.validate().unwrap_err();
        assert!(err.contains("exactly one choice"));
    }

    #[test]
    fn validate_rejects_score_out_of_range() {
        let mut p = fixture_valid();
        p.score_matrix
            .get_mut("performance")
            .unwrap()
            .insert("postgres".into(), 7);
        let err = p.validate().unwrap_err();
        assert!(err.contains("1..=5"));
    }

    #[test]
    fn validate_rejects_score_for_unknown_choice() {
        let mut p = fixture_valid();
        p.score_matrix
            .get_mut("performance")
            .unwrap()
            .insert("redis".into(), 3);
        let err = p.validate().unwrap_err();
        assert!(err.contains("unknown choice"));
    }

    #[test]
    fn validate_rejects_unknown_criterion() {
        let mut p = fixture_valid();
        p.score_matrix
            .insert("ghost".into(), std::collections::HashMap::new());
        let err = p.validate().unwrap_err();
        assert!(err.contains("unknown criterion"));
    }

    #[test]
    fn validate_rejects_negative_weight() {
        let mut p = fixture_valid();
        p.criteria[0].weight = -1.0;
        let err = p.validate().unwrap_err();
        assert!(err.contains("weight"));
    }

    #[test]
    fn is_expired_respects_valid_until() {
        let now = Utc::now();
        let mut p = fixture_valid();
        assert!(!p.is_expired(now));
        p.valid_until = Some(now - Duration::days(1));
        assert!(p.is_expired(now));
        p.valid_until = Some(now + Duration::days(1));
        assert!(!p.is_expired(now));
    }

    #[test]
    fn extract_decision_handles_missing_and_corrupt() {
        assert!(extract_decision(None).is_none());
        assert!(extract_decision(Some(&serde_json::json!({}))).is_none());
        assert!(extract_decision(Some(&serde_json::json!({"decision": "garbage"}))).is_none());

        let payload = fixture_valid();
        let wrapped = serde_json::json!({ "decision": payload });
        let parsed = extract_decision(Some(&wrapped)).unwrap();
        assert_eq!(parsed.question, payload.question);
        assert_eq!(parsed.choices.len(), payload.choices.len());
    }

    #[test]
    fn chosen_choice_returns_the_marked_one() {
        let p = fixture_valid();
        assert_eq!(p.chosen_choice().unwrap().id, "postgres");
    }
}
