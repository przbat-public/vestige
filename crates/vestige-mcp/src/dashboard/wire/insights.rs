//! Wire DTOs for the Insight Tier surface (Proposal B).
//!
//! Backed by `vestige_core::memory::InsightMetadata`. The wire shape
//! adds two server-computed conveniences over the raw payload:
//!
//! * `created_at` from the host `KnowledgeNode`, so the dashboard can
//!   sort/timeline insights without joining against `/api/memories`.
//! * `validated` mirrors `validated_by_agent` under the camelCase
//!   field the rest of the API uses, so React components don't need
//!   to remember the underscore variant exists.
//!
//! Origin is widened to plain `String` on the wire — adding a new
//! origin server-side then must not break old dashboards.

use serde::Serialize;
use ts_rs::TS;
use vestige_core::memory::{InsightMetadata, KnowledgeNode};

/// One insight card.
///
/// `id` is the host `KnowledgeNode.id` — same id the dashboard already
/// uses everywhere else for promote/demote/edit.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "InsightDto.ts", rename_all = "camelCase")]
pub struct InsightDto {
    pub id: String,
    /// The synthesised observation rendered as Markdown in the card body.
    pub content: String,
    /// Free-form category — `"pattern"`, `"contradiction"`, … — used
    /// as the chip label. Stable per origin engine; the dashboard
    /// MUST treat unknown values as fall-through ("Other") rather than
    /// throwing.
    pub insight_type: String,
    /// `"dream" | "synthesized" | "reflect" | "manual"`. String on the
    /// wire so we can add origins without breaking the client.
    pub origin: String,
    pub source_memory_ids: Vec<String>,
    pub confidence: f32,
    pub novelty: f32,
    pub validated: bool,
    /// RFC 3339. `None` while the insight is unvalidated.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub validated_at: Option<String>,
    /// Optional pointer to the legacy `InsightRecord` row this node
    /// mirrors. Lets the dashboard cross-link to the old insights
    /// surface during the transition.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub insight_record_id: Option<String>,
    /// RFC 3339 — when the host node was created.
    pub created_at: String,
    pub tags: Vec<String>,
}

impl InsightDto {
    /// Build a wire DTO from a host node + its parsed metadata.
    pub fn from_metadata(node: &KnowledgeNode, metadata: &InsightMetadata) -> Self {
        Self {
            id: node.id.clone(),
            content: node.content.clone(),
            insight_type: metadata.insight_type.clone(),
            origin: metadata.origin.as_str().to_string(),
            source_memory_ids: metadata.source_memory_ids.clone(),
            confidence: metadata.confidence,
            novelty: metadata.novelty,
            validated: metadata.validated_by_agent,
            validated_at: metadata.validated_at.map(|t| t.to_rfc3339()),
            insight_record_id: metadata.insight_record_id.clone(),
            created_at: node.created_at.to_rfc3339(),
            tags: node.tags.clone(),
        }
    }
}

/// Wrapper for `GET /api/insights`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "InsightListResponseDto.ts",
    rename_all = "camelCase"
)]
pub struct InsightListResponseDto {
    pub total: usize,
    /// Count of insights with `validated == true` in the returned page.
    /// The dashboard uses this for the "X / Y validated" header pill
    /// without re-iterating.
    pub validated_count: usize,
    pub insights: Vec<InsightDto>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vestige_core::memory::InsightOrigin;

    fn fixture_metadata() -> InsightMetadata {
        InsightMetadata {
            insight_type: "pattern".into(),
            origin: InsightOrigin::Dream,
            source_memory_ids: vec!["a".into(), "b".into()],
            confidence: 0.7,
            novelty: 0.4,
            validated_by_agent: false,
            validated_at: None,
            insight_record_id: Some("rec-1".into()),
        }
    }

    #[test]
    fn from_metadata_copies_all_fields() {
        let mut node = KnowledgeNode::default();
        node.id = "n1".into();
        node.content = "Three OAuth bugs in a row".into();
        node.tags = vec!["dream".into(), "insight".into(), "unvalidated".into()];
        let metadata = fixture_metadata();

        let dto = InsightDto::from_metadata(&node, &metadata);
        assert_eq!(dto.id, "n1");
        assert_eq!(dto.content, "Three OAuth bugs in a row");
        assert_eq!(dto.insight_type, "pattern");
        assert_eq!(dto.origin, "dream");
        assert_eq!(dto.source_memory_ids, vec!["a", "b"]);
        assert_eq!(dto.confidence, 0.7);
        assert_eq!(dto.novelty, 0.4);
        assert!(!dto.validated);
        assert!(dto.validated_at.is_none());
        assert_eq!(dto.insight_record_id.as_deref(), Some("rec-1"));
    }

    #[test]
    fn validated_metadata_serialises_timestamp() {
        let mut node = KnowledgeNode::default();
        node.id = "n2".into();
        let mut metadata = fixture_metadata();
        let t = Utc::now();
        metadata.mark_validated(t);

        let dto = InsightDto::from_metadata(&node, &metadata);
        assert!(dto.validated);
        assert_eq!(dto.validated_at, Some(t.to_rfc3339()));
    }
}
