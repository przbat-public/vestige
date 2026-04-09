//! Provenance metadata — track where each memory came from and how it was enriched.
//!
//! Records session context, agent identity, derivation chain, and
//! preprocessing artifacts so memory lineage can be traced.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Structured provenance metadata stored as JSON in the `provenance` column.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvenanceMetadata {
    /// Session identifier (conversation ID, request batch, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Which agent created this memory ("cursor", "claude", "api", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    /// When this memory was ingested
    pub ingested_at: DateTime<Utc>,

    /// IDs of parent memories this was derived from (merge, supersede, etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,

    /// How this memory was derived: "create", "update", "merge", "supersede"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derivation_type: Option<String>,

    /// Number of pronoun→entity replacements made by coreference rewriting
    #[serde(default, skip_serializing_if = "is_zero")]
    pub coref_rewrites: usize,

    /// Entities auto-extracted during preprocessing
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_entities: Vec<String>,

    /// Temporal expressions found and anchored
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub temporal_anchors_found: Vec<String>,

    /// Relation triples extracted (as "subject → predicate → object" strings)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations_extracted: Vec<String>,
}

fn is_zero(v: &usize) -> bool { *v == 0 }

impl ProvenanceMetadata {
    /// Create a new provenance with just the ingestion timestamp.
    pub fn new() -> Self {
        Self {
            ingested_at: Utc::now(),
            ..Default::default()
        }
    }

    /// Set the session and agent context.
    pub fn with_context(mut self, session_id: Option<String>, agent: Option<String>) -> Self {
        self.session_id = session_id;
        self.agent = agent;
        self
    }

    /// Record a derivation (from PE Gating decisions).
    pub fn with_derivation(mut self, parent_ids: Vec<String>, derivation_type: &str) -> Self {
        self.derived_from = parent_ids;
        self.derivation_type = Some(derivation_type.to_string());
        self
    }

    /// Serialize to JSON Value for storage.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }

    /// Deserialize from JSON Value.
    pub fn from_json(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_provenance() {
        let p = ProvenanceMetadata::new();
        assert!(p.session_id.is_none());
        assert!(p.agent.is_none());
        assert!(p.derived_from.is_empty());
        assert_eq!(p.coref_rewrites, 0);
    }

    #[test]
    fn test_with_context() {
        let p = ProvenanceMetadata::new()
            .with_context(Some("sess-123".into()), Some("cursor".into()));
        assert_eq!(p.session_id.as_deref(), Some("sess-123"));
        assert_eq!(p.agent.as_deref(), Some("cursor"));
    }

    #[test]
    fn test_with_derivation() {
        let p = ProvenanceMetadata::new()
            .with_derivation(vec!["parent-uuid".into()], "supersede");
        assert_eq!(p.derived_from, vec!["parent-uuid"]);
        assert_eq!(p.derivation_type.as_deref(), Some("supersede"));
    }

    #[test]
    fn test_json_roundtrip() {
        let mut p = ProvenanceMetadata::new()
            .with_context(Some("sess".into()), Some("agent".into()));
        p.coref_rewrites = 3;
        p.auto_entities = vec!["Alice".into(), "Vestige".into()];
        p.temporal_anchors_found = vec!["by next Friday".into()];
        p.relations_extracted = vec!["Alice → manages → Vestige".into()];

        let json = p.to_json();
        let p2 = ProvenanceMetadata::from_json(&json);

        assert_eq!(p2.session_id.as_deref(), Some("sess"));
        assert_eq!(p2.coref_rewrites, 3);
        assert_eq!(p2.auto_entities.len(), 2);
        assert_eq!(p2.temporal_anchors_found.len(), 1);
        assert_eq!(p2.relations_extracted.len(), 1);
    }

    #[test]
    fn test_empty_fields_not_serialized() {
        let p = ProvenanceMetadata::new();
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("session_id"), "Null fields should be skipped");
        assert!(!json.contains("auto_entities"), "Empty arrays should be skipped");
        assert!(!json.contains("coref_rewrites"), "Zero values should be skipped");
    }

    #[test]
    fn test_all_fields_populated() {
        let mut p = ProvenanceMetadata::new()
            .with_context(Some("s1".into()), Some("claude".into()))
            .with_derivation(vec!["p1".into(), "p2".into()], "merge");
        p.coref_rewrites = 5;
        p.auto_entities = vec!["Alice".into(), "Vestige".into()];
        p.temporal_anchors_found = vec!["next Friday".into()];
        p.relations_extracted = vec!["Alice → manages → Vestige".into()];

        let json = p.to_json();
        let p2 = ProvenanceMetadata::from_json(&json);

        assert_eq!(p2.session_id.as_deref(), Some("s1"));
        assert_eq!(p2.agent.as_deref(), Some("claude"));
        assert_eq!(p2.derived_from, vec!["p1", "p2"]);
        assert_eq!(p2.derivation_type.as_deref(), Some("merge"));
        assert_eq!(p2.coref_rewrites, 5);
        assert_eq!(p2.auto_entities.len(), 2);
        assert_eq!(p2.temporal_anchors_found.len(), 1);
        assert_eq!(p2.relations_extracted.len(), 1);
    }

    #[test]
    fn test_from_invalid_json_returns_default() {
        let invalid = serde_json::json!("not an object");
        let p = ProvenanceMetadata::from_json(&invalid);
        assert!(p.session_id.is_none());
        assert_eq!(p.coref_rewrites, 0);
    }

    #[test]
    fn test_ingested_at_is_recent() {
        let p = ProvenanceMetadata::new();
        let now = Utc::now();
        let diff = now - p.ingested_at;
        assert!(diff.num_seconds() < 2, "ingested_at should be ~now");
    }
}
