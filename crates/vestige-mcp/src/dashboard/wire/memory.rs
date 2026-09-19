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
    /// Structured Insight Tier metadata (Proposal B) when the host
    /// node is `node_type="insight"` and `extra_json.insight` parses
    /// cleanly. Present on `GET /api/memories/:id`, dropped on the
    /// list view by `into_list_view()` to keep payloads small.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub insight: Option<InsightDto>,
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
            insight,
        }
    }
}

impl MemoryDto {
    /// Strip optional metadata that the list view doesn't surface.
    /// Keeps responses small over the wire and matches the historical
    /// `list_memories` shape (no sentiment, no valid_from/valid_until,
    /// no last_accessed, no next_review).
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
