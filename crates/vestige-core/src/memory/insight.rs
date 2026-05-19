//! Structured insight payload (Proposal B — Insight Tier).
//!
//! The dream cycle synthesises pattern-level observations
//! ("3 of your last 5 OAuth bugs were token-cache invalidation
//! issues"). Historically those observations only lived in a side
//! table (`InsightRecord`) and never participated in search,
//! consolidation, or the FSRS-6 review cycle. The Insight Tier turns
//! each one into a first-class [`KnowledgeNode`] of type
//! `NodeType::Insight`, carrying the structured metadata defined here
//! under `extra_json.insight`.
//!
//! Stored insights are **tentative until validated**: they surface in
//! search results visually marked as "agent insight", and the agent
//! flips `validated_by_agent` to `true` by calling
//! `memory(action="promote")` after a user confirms the observation.
//! Until then, retention follows the same FSRS-6 schedule as any
//! other memory; the only behavioural difference is the dashboard
//! UI styling and the optional search filter to hide them.
//!
//! Backward compatibility: `InsightRecord` rows persist alongside —
//! the dream cycle writes both today so existing dashboards keep
//! rendering. A future migration can backfill `InsightMetadata` from
//! `InsightRecord` for legacy entries if we want them tracked.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Where an insight came from. Tracks provenance so we can recompute
/// or invalidate insights when the underlying engine changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InsightOrigin {
    /// 4-phase DreamEngine — emitted from REM_Creative or Integration.
    Dream,
    /// Legacy `MemoryDreamer::synthesize_insights` heuristic pipeline.
    Synthesized,
    /// `reflect` tool — produced during deliberate metacognitive
    /// examination (contradictions, gaps, stale decisions).
    Reflect,
    /// Hand-authored by a human, ingested through `smart_ingest` with
    /// `extra_json.insight` already filled.
    Manual,
}

impl InsightOrigin {
    /// Wire format string the dashboard reads off `extra_json.insight.origin`.
    pub fn as_str(&self) -> &'static str {
        match self {
            InsightOrigin::Dream => "dream",
            InsightOrigin::Synthesized => "synthesized",
            InsightOrigin::Reflect => "reflect",
            InsightOrigin::Manual => "manual",
        }
    }
}

/// Structured metadata for an Insight node, stored at
/// `extra_json.insight`.
///
/// Validation invariants (enforced by [`Self::validate`]):
/// - `source_memory_ids` must be non-empty (an insight without
///   evidence is just a guess).
/// - `confidence` and `novelty` must be in `[0.0, 1.0]`.
/// - `insight_type` must be non-empty (the dashboard uses it as a
///   chip label).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InsightMetadata {
    /// Free-form category — e.g. `"pattern"`, `"contradiction"`,
    /// `"cross_domain"`, `"causal"`. The dashboard groups insights
    /// by this field; keep it short and stable per engine.
    pub insight_type: String,
    /// What spawned this insight. Used for filtering and to decide
    /// whether to recompute when the engine is updated.
    pub origin: InsightOrigin,
    /// IDs of memories the insight was derived from. Always at least
    /// one; the dashboard hyperlinks each one so the user can audit
    /// the chain of reasoning.
    pub source_memory_ids: Vec<String>,
    /// Engine confidence in the insight (0..=1). Renders as a
    /// horizontal bar in the UI.
    pub confidence: f32,
    /// Engine novelty score (0..=1). Lower-novelty insights tend to
    /// just restate something already known.
    pub novelty: f32,
    /// User/agent has confirmed the insight. While `false`, the
    /// dashboard renders it with an "unvalidated" badge and search
    /// can optionally hide it. Flipped to `true` by
    /// `memory(action="promote")`.
    #[serde(default)]
    pub validated_by_agent: bool,
    /// When validation flipped to `true`. `None` while unvalidated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validated_at: Option<DateTime<Utc>>,
    /// Optional pointer to the `InsightRecord.id` row. The dream
    /// pipeline writes both representations during the transition
    /// window so the old dashboard keeps working — this field lets
    /// us correlate the two without re-querying by content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insight_record_id: Option<String>,
}

impl InsightMetadata {
    /// Returns `Ok(())` when every structural invariant holds. Run
    /// this in the producer before serializing to `extra_json` so
    /// malformed payloads never reach storage.
    pub fn validate(&self) -> Result<(), String> {
        if self.insight_type.trim().is_empty() {
            return Err("insight_type must be non-empty".into());
        }
        if self.source_memory_ids.is_empty() {
            return Err("source_memory_ids must contain at least one ID".into());
        }
        if !(0.0..=1.0).contains(&self.confidence) || !self.confidence.is_finite() {
            return Err(format!(
                "confidence must be in [0,1], got {}",
                self.confidence
            ));
        }
        if !(0.0..=1.0).contains(&self.novelty) || !self.novelty.is_finite() {
            return Err(format!("novelty must be in [0,1], got {}", self.novelty));
        }
        if self.validated_by_agent && self.validated_at.is_none() {
            return Err(
                "validated_at must be set when validated_by_agent is true".into(),
            );
        }
        Ok(())
    }

    /// Convenience: flip the validation flag and stamp the time.
    /// Idempotent — calling twice is a no-op (`validated_at` is
    /// only set on the first call).
    pub fn mark_validated(&mut self, now: DateTime<Utc>) {
        if !self.validated_by_agent {
            self.validated_by_agent = true;
            self.validated_at = Some(now);
        }
    }
}

/// Pull an `InsightMetadata` out of a node's `extra_json.insight`
/// slot. Drift-tolerant — a single corrupted row returns `None`
/// rather than poisoning a whole list endpoint.
pub fn extract_insight(extra_json: Option<&serde_json::Value>) -> Option<InsightMetadata> {
    let extra = extra_json?;
    let raw = extra.get("insight")?.clone();
    serde_json::from_value::<InsightMetadata>(raw).ok()
}

/// Replace the `insight` slot on an existing `extra_json` value
/// without dropping any other keys (`decision`, `hub`, …). Used by
/// the `promote` action to flip `validated_by_agent` without
/// rewriting the whole extra blob.
pub fn merge_insight_into_extra(
    existing: Option<&serde_json::Value>,
    insight: &InsightMetadata,
) -> serde_json::Value {
    let mut obj = match existing.and_then(|v| v.as_object()) {
        Some(o) => o.clone(),
        None => serde_json::Map::new(),
    };
    obj.insert(
        "insight".to_string(),
        serde_json::to_value(insight).unwrap_or(serde_json::Value::Null),
    );
    serde_json::Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn fixture_valid() -> InsightMetadata {
        InsightMetadata {
            insight_type: "pattern".into(),
            origin: InsightOrigin::Dream,
            source_memory_ids: vec!["a".into(), "b".into()],
            confidence: 0.8,
            novelty: 0.6,
            validated_by_agent: false,
            validated_at: None,
            insight_record_id: Some("rec-1".into()),
        }
    }

    #[test]
    fn validate_accepts_well_formed_payload() {
        let m = fixture_valid();
        assert!(m.validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_insight_type() {
        let mut m = fixture_valid();
        m.insight_type = "".into();
        assert!(m.validate().unwrap_err().contains("insight_type"));
    }

    #[test]
    fn validate_rejects_empty_sources() {
        let mut m = fixture_valid();
        m.source_memory_ids.clear();
        assert!(m.validate().unwrap_err().contains("source_memory_ids"));
    }

    #[test]
    fn validate_rejects_out_of_range_confidence() {
        let mut m = fixture_valid();
        m.confidence = 1.5;
        assert!(m.validate().unwrap_err().contains("confidence"));
        m.confidence = -0.1;
        assert!(m.validate().unwrap_err().contains("confidence"));
        m.confidence = f32::NAN;
        assert!(m.validate().unwrap_err().contains("confidence"));
    }

    #[test]
    fn validate_rejects_out_of_range_novelty() {
        let mut m = fixture_valid();
        m.novelty = 2.0;
        assert!(m.validate().unwrap_err().contains("novelty"));
    }

    #[test]
    fn validate_requires_timestamp_when_validated() {
        let mut m = fixture_valid();
        m.validated_by_agent = true;
        assert!(m.validate().unwrap_err().contains("validated_at"));
        m.validated_at = Some(Utc::now());
        assert!(m.validate().is_ok());
    }

    #[test]
    fn mark_validated_is_idempotent() {
        let mut m = fixture_valid();
        let t0 = Utc::now();
        m.mark_validated(t0);
        assert!(m.validated_by_agent);
        assert_eq!(m.validated_at, Some(t0));

        // Second call must NOT bump the timestamp.
        m.mark_validated(t0 + Duration::days(1));
        assert_eq!(m.validated_at, Some(t0));
    }

    #[test]
    fn extract_insight_handles_missing_and_corrupt() {
        assert!(extract_insight(None).is_none());
        assert!(extract_insight(Some(&serde_json::json!({}))).is_none());
        assert!(extract_insight(Some(&serde_json::json!({"insight": "junk"}))).is_none());

        let m = fixture_valid();
        let wrapped = serde_json::json!({ "insight": m });
        let parsed = extract_insight(Some(&wrapped)).unwrap();
        assert_eq!(parsed.insight_type, m.insight_type);
        assert_eq!(parsed.source_memory_ids, m.source_memory_ids);
    }

    #[test]
    fn merge_insight_preserves_other_keys() {
        let existing = serde_json::json!({
            "decision": { "question": "stays put" },
            "hub": { "signature": "stays put too" }
        });
        let m = fixture_valid();
        let merged = merge_insight_into_extra(Some(&existing), &m);
        assert_eq!(merged["decision"]["question"], "stays put");
        assert_eq!(merged["hub"]["signature"], "stays put too");
        assert_eq!(merged["insight"]["insightType"], "pattern");
    }

    #[test]
    fn merge_insight_creates_object_when_extra_is_missing() {
        let m = fixture_valid();
        let merged = merge_insight_into_extra(None, &m);
        assert!(merged.is_object());
        assert_eq!(merged["insight"]["insightType"], "pattern");
    }

    #[test]
    fn origin_round_trips_through_serde() {
        for origin in [
            InsightOrigin::Dream,
            InsightOrigin::Synthesized,
            InsightOrigin::Reflect,
            InsightOrigin::Manual,
        ] {
            let raw = serde_json::to_value(&origin).unwrap();
            let parsed: InsightOrigin = serde_json::from_value(raw.clone()).unwrap();
            assert_eq!(parsed, origin);
            assert_eq!(raw.as_str().unwrap(), origin.as_str());
        }
    }
}
