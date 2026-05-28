//! Wire DTOs for the Decision Matrix surface (Proposal C).
//!
//! Backed by `vestige_core::memory::DecisionPayload` — these DTOs are the
//! projection the dashboard consumes. They are intentionally a flatter
//! shape (`Vec` over `HashMap`) so the React table can iterate without
//! re-sorting keys per render. Rust → wire mapping:
//!
//! * `DecisionPayload.score_matrix: HashMap<criterion_id, HashMap<choice_id, score>>`
//!   becomes `Vec<DecisionScoreCellDto { criterionId, choiceId, score }>`.
//!   The dashboard joins the cells against `criteria` / `choices` by id.
//! * Optional fields stay optional with `#[ts(optional)]` so the TS
//!   declaration uses `field?: T` (not `field: T | null`).
//!
//! Two top-level DTOs are exposed:
//!
//! * `DecisionDto` — one decision card, returned by both the list and
//!   detail endpoints.
//! * `DecisionListResponseDto` — wrapper for `GET /api/decisions` with
//!   the post-filter total.

use serde::Serialize;
use ts_rs::TS;
use vestige_core::memory::{Choice, Criterion, DecisionPayload};

/// One row in the score matrix. Sparse: cells the agent didn't score
/// are simply absent from the vector. Dashboard renders missing cells
/// as `—` rather than as 0.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionScoreCellDto.ts", rename_all = "camelCase")]
pub struct DecisionScoreCellDto {
    pub criterion_id: String,
    pub choice_id: String,
    pub score: u8,
}

/// One alternative the agent considered.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionChoiceDto.ts", rename_all = "camelCase")]
pub struct DecisionChoiceDto {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub summary: Option<String>,
    pub chosen: bool,
}

impl From<&Choice> for DecisionChoiceDto {
    fn from(c: &Choice) -> Self {
        Self {
            id: c.id.clone(),
            label: c.label.clone(),
            summary: c.summary.clone(),
            chosen: c.chosen,
        }
    }
}

/// One evaluation axis. `weight` carries through verbatim; the dashboard
/// uses it to bias the radar visualisation.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionCriterionDto.ts", rename_all = "camelCase")]
pub struct DecisionCriterionDto {
    pub id: String,
    pub label: String,
    pub weight: f32,
}

impl From<&Criterion> for DecisionCriterionDto {
    fn from(c: &Criterion) -> Self {
        Self {
            id: c.id.clone(),
            label: c.label.clone(),
            weight: c.weight,
        }
    }
}

/// A single decision card.
///
/// `id` is the host `KnowledgeNode.id` — the same id the dashboard
/// already uses everywhere else for promote/demote/edit.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionDto.ts", rename_all = "camelCase")]
pub struct DecisionDto {
    pub id: String,
    pub question: String,
    pub rationale: String,
    pub choices: Vec<DecisionChoiceDto>,
    pub criteria: Vec<DecisionCriterionDto>,
    pub score_matrix: Vec<DecisionScoreCellDto>,
    /// RFC 3339. None when the agent didn't set an expiry. Dashboard uses
    /// it to render the stale/expired badge.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub valid_until: Option<String>,
    pub supersedes: Vec<String>,
    /// RFC 3339 — when the host node was created. Used as the card
    /// subtitle ("decided 2026-05-12").
    pub created_at: String,
    /// Convenience: `true` iff `valid_until.is_some_and(|t| t < now)`,
    /// computed server-side so every client renders the same badge
    /// without re-implementing the time math.
    pub expired: bool,
    pub tags: Vec<String>,
}

impl DecisionDto {
    /// Build a wire DTO from a host node + its parsed payload.
    pub fn from_payload(
        node: &vestige_core::memory::KnowledgeNode,
        payload: &DecisionPayload,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let mut cells: Vec<DecisionScoreCellDto> = payload
            .score_matrix
            .iter()
            .flat_map(|(crit_id, row)| {
                row.iter().map(move |(ch_id, score)| DecisionScoreCellDto {
                    criterion_id: crit_id.clone(),
                    choice_id: ch_id.clone(),
                    score: *score,
                })
            })
            .collect();
        // Deterministic ordering — keeps snapshot tests stable and avoids
        // pointless re-renders when HashMap iteration order shifts.
        cells.sort_by(|a, b| {
            a.criterion_id
                .cmp(&b.criterion_id)
                .then(a.choice_id.cmp(&b.choice_id))
        });

        Self {
            id: node.id.clone(),
            question: payload.question.clone(),
            rationale: payload.rationale.clone(),
            choices: payload.choices.iter().map(Into::into).collect(),
            criteria: payload.criteria.iter().map(Into::into).collect(),
            score_matrix: cells,
            valid_until: payload.valid_until.map(|t| t.to_rfc3339()),
            supersedes: payload.supersedes.clone(),
            created_at: node.created_at.to_rfc3339(),
            expired: payload.is_expired(now),
            tags: node.tags.clone(),
        }
    }
}

/// Wrapper for `GET /api/decisions`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "DecisionListResponseDto.ts",
    rename_all = "camelCase"
)]
pub struct DecisionListResponseDto {
    pub total: usize,
    pub decisions: Vec<DecisionDto>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vestige_core::memory::{Choice as CoreChoice, Criterion as CoreCriterion, KnowledgeNode};

    fn fixture_payload() -> DecisionPayload {
        let mut score_matrix = std::collections::HashMap::new();
        let mut perf = std::collections::HashMap::new();
        perf.insert("postgres".to_string(), 5);
        perf.insert("mysql".to_string(), 3);
        score_matrix.insert("performance".to_string(), perf);

        DecisionPayload {
            question: "Which DB?".into(),
            rationale: "JSONB.".into(),
            choices: vec![
                CoreChoice {
                    id: "postgres".into(),
                    label: "PostgreSQL".into(),
                    summary: None,
                    chosen: true,
                },
                CoreChoice {
                    id: "mysql".into(),
                    label: "MySQL".into(),
                    summary: None,
                    chosen: false,
                },
            ],
            criteria: vec![CoreCriterion {
                id: "performance".into(),
                label: "Read performance".into(),
                weight: 1.0,
            }],
            score_matrix,
            valid_until: None,
            supersedes: vec![],
        }
    }

    #[test]
    fn from_payload_orders_cells_deterministically() {
        let node = KnowledgeNode {
            id: "n1".into(),
            ..KnowledgeNode::default()
        };
        let payload = fixture_payload();

        let dto = DecisionDto::from_payload(&node, &payload, Utc::now());
        assert_eq!(dto.score_matrix.len(), 2);
        // Sorted by (criterion_id, choice_id) — both share criterion so
        // the choices land alphabetically.
        assert_eq!(dto.score_matrix[0].choice_id, "mysql");
        assert_eq!(dto.score_matrix[1].choice_id, "postgres");
    }

    #[test]
    fn expired_flag_respects_valid_until() {
        let node = KnowledgeNode {
            id: "n2".into(),
            ..KnowledgeNode::default()
        };
        let mut payload = fixture_payload();
        let now = Utc::now();
        payload.valid_until = Some(now - chrono::Duration::days(1));

        let dto = DecisionDto::from_payload(&node, &payload, now);
        assert!(dto.expired);
    }
}
