//! Wire DTOs for memory-related endpoints.
//!
//! Mirrors the legacy `serde_json::json!({...})` shape that the dashboard
//! has been consuming, but as a typed Rust struct so ts-rs can emit a
//! TypeScript declaration in lockstep.
//!
//! `MemoryDto` is the projection of `KnowledgeNode` exposed to the
//! dashboard — it intentionally drops FSRS state (stability, difficulty,
//! reps, lapses) and storage-internal fields (memory_kind, subject,
//! provenance) because the dashboard never read them. Adding a field
//! here is a wire-contract change: bump the dashboard at the same time
//! and let the ts-rs CI gate enforce it.

use serde::Serialize;
use ts_rs::TS;
use vestige_core::memory::{KnowledgeNode, extract_insight};

use super::insights::InsightDto;

/// One memory as the dashboard sees it.
///
/// All `Option` fields are emitted as `null`/missing on the wire when
/// absent. The corresponding TypeScript declaration uses `T | null` for
/// fields without `#[ts(optional)]`; we use `#[ts(optional)]` for fields
/// that the dashboard treats as truly absent (e.g. `source` is sometimes
/// just not set).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "MemoryDto.ts", rename_all = "camelCase")]
pub struct MemoryDto {
    pub id: String,
    pub content: String,
    pub node_type: String,
    pub tags: Vec<String>,
    pub retention_strength: f64,
    pub storage_strength: f64,
    pub retrieval_strength: f64,
    /// ISO-8601 RFC 3339 — frontend parses with `new Date(...)`.
    pub created_at: String,
    /// ISO-8601 RFC 3339 — when we *recorded* the memory (V17
    /// `knowledge_nodes.recorded_at`).
    ///
    /// Distinct from `created_at` (row birth) and from `last_accessed_at`
    /// (refreshed by every search hit, so it carries no claim about age).
    /// `created_at` is the only other anchor the dashboard has, and it is
    /// not the same claim: "when did we learn this" is what a reader uses
    /// to date the picture, and until this field existed the dashboard
    /// could not answer it at all.
    pub recorded_at: String,
    /// ISO-8601 RFC 3339.
    pub updated_at: String,
    /// `Option` because partial responses (search hits, list view) skip
    /// it; the full GET handler always populates it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_accessed_at: Option<String>,
    /// FSRS-6 next-review timestamp. `None` when the memory has never
    /// been reviewed (initial state) or has been suppressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_review_at: Option<String>,
    /// Hybrid search score; only populated by `list_memories` when a
    /// `?q=` query is supplied. Ad-hoc field on a domain model is a
    /// smell, but matches what the dashboard already expects.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub combined_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub review_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sentiment_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sentiment_magnitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub valid_until: Option<String>,
    /// Inferred from `node_type` + tags + content. Stable enum surface so
    /// the dashboard can color-code memories without re-implementing the
    /// inference. See `KnowledgeNode::epistemic_status` for the rules.
    pub epistemic_status: EpistemicStatusDto,
    /// Coarse memory-system bucket (episodic / semantic / procedural).
    pub memory_system: MemorySystemDto,
    /// What the write-time self-containedness gate said (V18):
    /// `Some(true)` it ran and passed, `Some(false)` it ran and flagged the
    /// memory as leaning on the conversation it came from, `None` no gate ran.
    ///
    /// `None` is deliberately *not* rendered as "clean" by the dashboard: the
    /// column is NULL for every row written before V18 or by a writer that is
    /// not `smart_ingest`, and a store that reports "no flags" must not be
    /// reporting on rows it never checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub self_contained: Option<bool>,
    /// The findings behind a `self_contained: Some(false)`, so a reader learns
    /// what to fix and not only that something is wrong. Empty/absent when the
    /// gate passed, never ran, or the persisted JSON is unreadable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub self_contained_findings: Option<Vec<SelfContainedFindingDto>>,
    /// Structured Insight Tier metadata (Proposal B) when the host
    /// node is `node_type="insight"` and `extra_json.insight` parses
    /// cleanly. Present on `GET /api/memories/:id`, dropped on the
    /// list view by `into_list_view()` to keep payloads small.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub insight: Option<InsightDto>,
}

/// One rule the self-containedness gate fired on a memory.
///
/// Mirrors the `{kind, span, hint}` triple the gate writes into
/// `knowledge_nodes.self_contained_findings` and returns on the write
/// response. `kind` names the rule, `span` is the exact text that tripped it,
/// `hint` says what to write instead — the three things a reader needs to
/// repair the entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "SelfContainedFindingDto.ts",
    rename_all = "camelCase"
)]
pub struct SelfContainedFindingDto {
    pub kind: String,
    pub span: String,
    pub hint: String,
}

impl SelfContainedFindingDto {
    /// Parse the persisted `self_contained_findings` JSON array.
    ///
    /// Tolerant by construction: the column is free-form JSON written by the
    /// gate, and a row whose payload was truncated (or written by a future
    /// shape) must not turn the whole memory response into a 500. On a parse
    /// failure this returns `None` — the `self_contained: false` marker still
    /// tells the reader the entry was flagged, which is the claim that matters.
    pub fn parse_all(value: Option<&serde_json::Value>) -> Option<Vec<Self>> {
        let value = value?;
        if value.is_null() {
            return None;
        }
        serde_json::from_value(value.clone())
            .ok()
            .filter(|v: &Vec<Self>| !v.is_empty())
    }
}

/// Mirrors `vestige_core::memory::EpistemicStatus`. Kept as a wire-side
/// enum so adding a new variant in core without touching the DTO becomes
/// a compile error rather than a silent UI regression.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "EpistemicStatusDto.ts", rename_all = "snake_case")]
pub enum EpistemicStatusDto {
    World,
    Experience,
    Observation,
    Opinion,
}

impl From<vestige_core::memory::EpistemicStatus> for EpistemicStatusDto {
    fn from(s: vestige_core::memory::EpistemicStatus) -> Self {
        use vestige_core::memory::EpistemicStatus as E;
        match s {
            E::World => Self::World,
            E::Experience => Self::Experience,
            E::Observation => Self::Observation,
            E::Opinion => Self::Opinion,
        }
    }
}

/// Mirrors `vestige_core::memory::MemorySystem`.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "MemorySystemDto.ts", rename_all = "snake_case")]
pub enum MemorySystemDto {
    Episodic,
    Semantic,
    Procedural,
}

impl From<vestige_core::memory::MemorySystem> for MemorySystemDto {
    fn from(s: vestige_core::memory::MemorySystem) -> Self {
        use vestige_core::memory::MemorySystem as M;
        match s {
            M::Episodic => Self::Episodic,
            M::Semantic => Self::Semantic,
            M::Procedural => Self::Procedural,
        }
    }
}

impl From<&KnowledgeNode> for MemoryDto {
    fn from(n: &KnowledgeNode) -> Self {
        // Insight payload is best-effort — corrupted extra_json must
        // not poison the whole memory response.
        let insight = if n.node_type == "insight" {
            extract_insight(n.extra_json.as_ref())
                .map(|metadata| InsightDto::from_metadata(n, &metadata))
        } else {
            None
        };
        Self {
            id: n.id.clone(),
            content: n.content.clone(),
            node_type: n.node_type.clone(),
            tags: n.tags.clone(),
            retention_strength: n.retention_strength,
            storage_strength: n.storage_strength,
            retrieval_strength: n.retrieval_strength,
            created_at: n.created_at.to_rfc3339(),
            recorded_at: n.recorded_at.to_rfc3339(),
            updated_at: n.updated_at.to_rfc3339(),
            last_accessed_at: Some(n.last_accessed.to_rfc3339()),
            next_review_at: n.next_review.map(|dt| dt.to_rfc3339()),
            combined_score: None,
            source: n.source.clone(),
            review_count: Some(n.reps),
            sentiment_score: Some(n.sentiment_score),
            sentiment_magnitude: Some(n.sentiment_magnitude),
            valid_from: n.valid_from.map(|dt| dt.to_rfc3339()),
            valid_until: n.valid_until.map(|dt| dt.to_rfc3339()),
            epistemic_status: n.epistemic_status().into(),
            memory_system: n.memory_system().into(),
            self_contained: n.self_contained,
            self_contained_findings: SelfContainedFindingDto::parse_all(
                n.self_contained_findings.as_ref(),
            ),
            insight,
        }
    }
}

impl MemoryDto {
    /// Strip optional metadata that the list view doesn't surface.
    /// Keeps responses small over the wire and matches the historical
    /// `list_memories` shape (no sentiment, no valid_from/valid_until,
    /// no last_accessed, no next_review).
    ///
    /// `recorded_at` and the gate verdict are deliberately *kept*: the list
    /// renders the record time next to each row and marks a flagged memory,
    /// and `MemoryDetail` is seeded from the list row it was opened from
    /// (it does not re-fetch), so dropping them here would make the detail
    /// panel unable to show either one on the dashboard's main path.
    pub fn into_list_view(mut self) -> Self {
        self.last_accessed_at = None;
        self.next_review_at = None;
        self.sentiment_score = None;
        self.sentiment_magnitude = None;
        self.valid_from = None;
        self.valid_until = None;
        // Insight metadata is heavy (carries source IDs, content
        // snippets) and the list view doesn't render it — drop it to
        // keep the wire payload small. Per-row detail view still has it.
        self.insight = None;
        self
    }

    /// Attach a hybrid-search combined score (`bm25 * w1 + cosine * w2`).
    /// Builder-style so list/search handlers can compose with `From`.
    pub fn with_combined_score(mut self, score: f64) -> Self {
        self.combined_score = Some(score);
        self
    }
}

/// Wrapper for `GET /api/memories` and similar list endpoints.
///
/// `total` is the count *after* server-side filtering (node_type / tag /
/// min_retention) — same definition the dashboard already relied on.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "MemoryListResponseDto.ts",
    rename_all = "camelCase"
)]
pub struct MemoryListResponseDto {
    pub total: usize,
    pub memories: Vec<MemoryDto>,
}

/// Response for `POST /api/memories/{id}/promote` and `…/demote`.
///
/// Carries just the changed retention plus a flag so the dashboard's
/// optimistic UI can confirm the operation without fetching the full
/// memory back.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "MemoryStatusDto.ts", rename_all = "camelCase")]
pub struct MemoryStatusDto {
    /// `true` when the action succeeded. Always `true` in practice
    /// (storage errors return non-2xx) but kept on the wire so callers
    /// can pattern-match on a single shape.
    pub ok: bool,
    pub id: String,
    pub retention_strength: f64,
    /// Which transition happened, for UI feedback ("Promoted", "Demoted",
    /// "Deleted"). The dashboard had three different status booleans
    /// before — this collapses them.
    pub action: MemoryStatusAction,
}

/// Discriminator for `MemoryStatusDto`.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "MemoryStatusAction.ts", rename_all = "snake_case")]
pub enum MemoryStatusAction {
    Promoted,
    Demoted,
    Deleted,
}

/// Response for `PATCH /api/memories/{id}` — returns the post-update DTO
/// plus a `field` discriminator so the dashboard can label the toast
/// correctly ("Content updated" vs "Tags updated").
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "MemoryUpdateResultDto.ts",
    rename_all = "camelCase"
)]
pub struct MemoryUpdateResultDto {
    pub memory: MemoryDto,
    /// Free-form discriminator: "content", "tags", "content+tags".
    pub field: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use serde_json::json;

    fn node() -> KnowledgeNode {
        KnowledgeNode {
            id: "11111111-1111-4111-9111-111111111111".to_string(),
            content: "Vestige stores memories in SQLite".to_string(),
            ..Default::default()
        }
    }

    /// `recorded_at` is the one anchor the dashboard could not show: the
    /// DTO copied created/updated/accessed and dropped it. It must reach the
    /// wire, and it must survive `into_list_view()` because `MemoryDetail` is
    /// seeded from the list row rather than re-fetched.
    #[test]
    fn recorded_at_reaches_both_views_and_is_not_created_at() {
        let created = Utc::now() - Duration::days(30);
        let recorded = Utc::now() - Duration::days(1);
        let n = KnowledgeNode {
            created_at: created,
            recorded_at: recorded,
            ..node()
        };

        let full = MemoryDto::from(&n);
        assert_eq!(full.recorded_at, recorded.to_rfc3339());
        assert_ne!(
            full.recorded_at, full.created_at,
            "the fixture must keep the two timestamps distinguishable, or the \
             assertion above proves nothing"
        );

        let listed = MemoryDto::from(&n).into_list_view();
        assert_eq!(listed.recorded_at, recorded.to_rfc3339());
    }

    /// A flagged write must stay flagged everywhere it is read: the list view
    /// drops heavy optional fields, and dropping this one would make the
    /// dashboard silently present a memory that needs its conversation as a
    /// clean entry.
    #[test]
    fn flagged_gate_verdict_and_findings_survive_the_list_view() {
        let n = KnowledgeNode {
            self_contained: Some(false),
            self_contained_findings: Some(json!([
                { "kind": "discourse_deixis", "span": "as we discussed", "hint": "name what was discussed" }
            ])),
            ..node()
        };

        let listed = MemoryDto::from(&n).into_list_view();
        assert_eq!(listed.self_contained, Some(false));
        let findings = listed
            .self_contained_findings
            .expect("findings must survive the list view");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "discourse_deixis");
        assert_eq!(findings[0].span, "as we discussed");
        assert_eq!(findings[0].hint, "name what was discussed");
    }

    /// `None` means the gate never ran, which is not the same claim as
    /// `Some(true)`. It must stay `None` on the wire (the field is
    /// `skip_serializing_if`), never collapse to "clean".
    #[test]
    fn a_row_the_gate_never_checked_has_no_verdict() {
        let dto = MemoryDto::from(&node());
        assert_eq!(dto.self_contained, None);
        assert_eq!(dto.self_contained_findings, None);
        let wire = serde_json::to_value(&dto).unwrap();
        assert!(
            wire.get("selfContained").is_none(),
            "an unchecked row must not carry a verdict key: {wire}"
        );
    }

    /// The findings column is free-form JSON. A payload the DTO cannot read
    /// must still leave the flag visible — losing the flag would be the
    /// lie, losing the prose is only an inconvenience.
    #[test]
    fn unreadable_findings_do_not_hide_the_flag() {
        let n = KnowledgeNode {
            self_contained: Some(false),
            self_contained_findings: Some(json!({ "unexpected": "shape" })),
            ..node()
        };
        let dto = MemoryDto::from(&n);
        assert_eq!(dto.self_contained, Some(false));
        assert_eq!(dto.self_contained_findings, None);
    }
}
