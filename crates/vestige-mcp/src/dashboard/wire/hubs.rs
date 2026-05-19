//! Wire DTOs for the Topic Hubs surface (Proposal A).
//!
//! Backed by `vestige_core::memory::HubMetadata`. Same shape philosophy
//! as `insights`: rename to camelCase, pull host-node fields (`id`,
//! `content`, `created_at`, `tags`) up to the top level so the
//! dashboard can render a card without a follow-up `/api/memories/:id`
//! call.

use serde::Serialize;
use ts_rs::TS;
use vestige_core::memory::{HubMetadata, KnowledgeNode};

/// One Topic Hub card.
///
/// `clusterSignature` is the deterministic FNV-1a id of the sorted
/// child set. The dashboard treats it as an opaque token (useful for
/// "is this the same cluster?" deep links).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "HubDto.ts", rename_all = "camelCase")]
pub struct HubDto {
    pub id: String,
    /// Rendered hub body — Markdown bullet summary today, future LLM
    /// variants may emit prose.
    pub content: String,
    pub cluster_signature: String,
    pub child_ids: Vec<String>,
    pub dominant_tags: Vec<String>,
    /// `[earliest, latest]` RFC 3339 timestamps across cluster members.
    pub date_range: (String, String),
    /// `"template_v1" | "llm_…"` — opaque string the dashboard can
    /// chip-render so users see which generator wrote the body.
    pub generation_method: String,
    /// Number of times the dream cycle has overwritten this hub. Lets
    /// the dashboard show a "regenerated N times" badge on dense
    /// clusters.
    pub regeneration_count: u32,
    /// RFC 3339 — the last time the dream cycle bumped the hub.
    pub last_regenerated_at: String,
    /// RFC 3339 — when the host node was first created.
    pub created_at: String,
    pub tags: Vec<String>,
}

impl HubDto {
    /// Build a wire DTO from a host node + its parsed metadata.
    pub fn from_metadata(node: &KnowledgeNode, metadata: &HubMetadata) -> Self {
        Self {
            id: node.id.clone(),
            content: node.content.clone(),
            cluster_signature: metadata.cluster_signature.clone(),
            child_ids: metadata.child_ids.clone(),
            dominant_tags: metadata.dominant_tags.clone(),
            date_range: (
                metadata.date_range.0.to_rfc3339(),
                metadata.date_range.1.to_rfc3339(),
            ),
            generation_method: metadata.generation_method.clone(),
            regeneration_count: metadata.regeneration_count,
            last_regenerated_at: metadata.last_regenerated_at.to_rfc3339(),
            created_at: node.created_at.to_rfc3339(),
            tags: node.tags.clone(),
        }
    }
}

/// Wrapper for `GET /api/hubs`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "HubListResponseDto.ts", rename_all = "camelCase")]
pub struct HubListResponseDto {
    pub total: usize,
    pub hubs: Vec<HubDto>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn fixture_metadata() -> HubMetadata {
        let now = Utc::now();
        HubMetadata {
            child_ids: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
            cluster_signature: "deadbeefdeadbeef".into(),
            regeneration_count: 3,
            last_regenerated_at: now,
            generation_method: "template_v1".into(),
            dominant_tags: vec!["oauth".into(), "auth".into()],
            date_range: (now, now),
        }
    }

    #[test]
    fn from_metadata_copies_all_fields() {
        let mut node = KnowledgeNode::default();
        node.id = "hub-1".into();
        node.content = "Five OAuth memories cluster around session timeout".into();
        node.tags = vec!["hub".into(), "auto-generated".into(), "oauth".into()];
        let metadata = fixture_metadata();

        let dto = HubDto::from_metadata(&node, &metadata);
        assert_eq!(dto.id, "hub-1");
        assert_eq!(dto.cluster_signature, "deadbeefdeadbeef");
        assert_eq!(dto.child_ids.len(), 5);
        assert_eq!(dto.dominant_tags, vec!["oauth", "auth"]);
        assert_eq!(dto.regeneration_count, 3);
        assert_eq!(dto.generation_method, "template_v1");
        assert!(dto.tags.iter().any(|t| t == "hub"));
    }

    #[test]
    fn date_range_is_rfc3339_tuple() {
        let node = KnowledgeNode::default();
        let metadata = fixture_metadata();
        let dto = HubDto::from_metadata(&node, &metadata);
        assert!(dto.date_range.0.contains('T'));
        assert!(dto.date_range.1.contains('T'));
    }
}
