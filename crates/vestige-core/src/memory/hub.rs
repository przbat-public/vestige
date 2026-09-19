//! Structured topic-hub payload (Proposal A — Topic Hubs).
//!
//! REM_Creative spots clusters of related memories ("68 shared
//! patterns found" in a single dream cycle) but historically those
//! observations only existed as pairwise graph edges. Topic Hubs
//! promote a cluster into a first-class [`KnowledgeNode`] of type
//! `NodeType::Hub`, carrying the structured metadata defined here
//! under `extra_json.hub`. The hub's `content` is a generated
//! summary (template or LLM); the metadata holds the bookkeeping
//! needed to regenerate, audit, and collapse on it.
//!
//! Layout choices:
//! - One memory per cluster. The same physical cluster
//!   (`cluster_signature`) is updated in place across dream cycles
//!   rather than spawning a new node every time — see
//!   `regeneration_count` / `last_regenerated_at`.
//! - The signature is the SHA-256 of the **sorted** child IDs so it
//!   is stable across iteration order and across processes.
//! - `generation_method` is a free-form string ("template_v1",
//!   "llm_gpt-4o-mini_v1") so we can A/B content quality and roll a
//!   bad model back without dropping the hub.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Structured metadata for a Hub node, stored at `extra_json.hub`.
///
/// Validation invariants (enforced by [`Self::validate`]):
/// - `child_ids` must contain at least 2 entries (a single-member
///   cluster is not a cluster).
/// - `cluster_signature` must be non-empty.
/// - `dominant_tags` may be empty for legacy/manual hubs but not
///   exceed 16 entries (UI chip budget).
/// - `date_range.0 <= date_range.1`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HubMetadata {
    /// IDs of the memories this hub summarises. Order is **not**
    /// significant — the signature is computed on a sorted copy so
    /// two callers passing the same set produce the same hash.
    pub child_ids: Vec<String>,
    /// SHA-256 hex of the sorted `child_ids`, used as a primary
    /// dedup key. Indexed via migration v13.
    pub cluster_signature: String,
    /// How many times the dream cycle has regenerated this hub.
    /// 1 = freshly created, ≥2 = updated in place.
    #[serde(default)]
    pub regeneration_count: u32,
    /// When the most recent regeneration happened. The hub's own
    /// `updated_at` also moves, but we duplicate it here so the
    /// dashboard can rank hubs without joining columns.
    pub last_regenerated_at: DateTime<Utc>,
    /// Pipeline that produced the current `content`. Free-form
    /// string; today: `"template_v1"`. Future LLM variants live
    /// behind the `VESTIGE_HUB_SYNTHESIS=llm` env switch and stamp
    /// e.g. `"llm_gpt-4o-mini_v1"` so we can roll back per-version.
    pub generation_method: String,
    /// Top tags shared by the cluster members. Used for filters and
    /// the chip row in the dashboard. Capped at 16 entries.
    #[serde(default)]
    pub dominant_tags: Vec<String>,
    /// `[earliest, latest]` `created_at` of the cluster members.
    /// Rendered as a "covers Mar–May 2026" subtitle.
    pub date_range: (DateTime<Utc>, DateTime<Utc>),
}

impl HubMetadata {
    /// Returns `Ok(())` when every structural invariant holds. Run
    /// in the producer before serializing so malformed payloads
    /// never reach storage.
    pub fn validate(&self) -> Result<(), String> {
        if self.child_ids.len() < 2 {
            return Err(format!(
                "child_ids must contain at least 2 entries, got {}",
                self.child_ids.len()
            ));
        }
        if self.cluster_signature.trim().is_empty() {
            return Err("cluster_signature must be non-empty".into());
        }
        if self.generation_method.trim().is_empty() {
            return Err("generation_method must be non-empty".into());
        }
        if self.dominant_tags.len() > 16 {
            return Err(format!(
                "dominant_tags is capped at 16, got {}",
                self.dominant_tags.len()
            ));
        }
        if self.date_range.0 > self.date_range.1 {
            return Err("date_range.0 must be <= date_range.1".into());
        }
        Ok(())
    }

    /// Convenience: bump `regeneration_count` and refresh the
    /// timestamp. Used by the hub generator when the same cluster
    /// is observed again in a later dream cycle.
    pub fn touch_regeneration(&mut self, now: DateTime<Utc>) {
        self.regeneration_count = self.regeneration_count.saturating_add(1);
        self.last_regenerated_at = now;
    }
}

/// Deterministic cluster identity — 64-bit FNV-1a of the sorted,
/// comma-joined IDs, lowercase hex (16 chars).
///
/// Two callers that observe the same set of child IDs (regardless
/// of iteration order or duplicates) must produce the same string.
/// Used to answer "is there already a hub for this cluster?" via
/// the partial index added in migration v13.
///
/// We do **not** need a cryptographic hash here — collisions in
/// the cluster_signature space just mean "occasionally two distinct
/// clusters look like the same hub", which is no worse than the
/// existing dedup heuristics. FNV-1a is a hard dep-free choice that
/// is deterministic across processes, OSes, and Rust versions.
pub fn cluster_signature(child_ids: &[String]) -> String {
    let mut sorted: Vec<&str> = child_ids.iter().map(|s| s.as_str()).collect();
    sorted.sort_unstable();
    sorted.dedup();

    let joined = sorted.join(",");
    let h = fnv1a_64(joined.as_bytes());
    format!("{h:016x}")
}

/// FNV-1a 64-bit. Constants per the FNV reference:
///   offset basis = 0xcbf29ce484222325
///   prime        = 0x100000001b3
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Pull a `HubMetadata` out of a node's `extra_json.hub` slot.
/// Drift-tolerant: a corrupt entry returns `None` rather than
/// poisoning a whole list endpoint.
pub fn extract_hub(extra_json: Option<&serde_json::Value>) -> Option<HubMetadata> {
    let extra = extra_json?;
    let raw = extra.get("hub")?.clone();
    serde_json::from_value::<HubMetadata>(raw).ok()
}

/// Replace the `hub` slot on an existing `extra_json` value
/// without dropping any other keys (`decision`, `insight`, …).
pub fn merge_hub_into_extra(
    existing: Option<&serde_json::Value>,
    hub: &HubMetadata,
) -> serde_json::Value {
    let mut obj = match existing.and_then(|v| v.as_object()) {
        Some(o) => o.clone(),
        None => serde_json::Map::new(),
    };
    obj.insert(
        "hub".to_string(),
        serde_json::to_value(hub).unwrap_or(serde_json::Value::Null),
    );
    serde_json::Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn fixture_valid() -> HubMetadata {
        let now = Utc::now();
        HubMetadata {
            child_ids: vec!["a".into(), "b".into(), "c".into()],
            cluster_signature: cluster_signature(&["a".into(), "b".into(), "c".into()]),
            regeneration_count: 1,
            last_regenerated_at: now,
            generation_method: "template_v1".into(),
            dominant_tags: vec!["rust".into(), "search".into()],
            date_range: (now - Duration::days(30), now),
        }
    }

    #[test]
    fn validate_accepts_well_formed() {
        assert!(fixture_valid().validate().is_ok());
    }

    #[test]
    fn validate_rejects_too_few_children() {
        let mut m = fixture_valid();
        m.child_ids = vec!["only-one".into()];
        assert!(m.validate().unwrap_err().contains("child_ids"));
    }

    #[test]
    fn validate_rejects_empty_signature() {
        let mut m = fixture_valid();
        m.cluster_signature = "".into();
        assert!(m.validate().unwrap_err().contains("cluster_signature"));
    }

    #[test]
    fn validate_rejects_empty_generation_method() {
        let mut m = fixture_valid();
        m.generation_method = "".into();
        assert!(m.validate().unwrap_err().contains("generation_method"));
    }

    #[test]
    fn validate_rejects_too_many_tags() {
        let mut m = fixture_valid();
        m.dominant_tags = (0..17).map(|i| format!("t{i}")).collect();
        assert!(m.validate().unwrap_err().contains("dominant_tags"));
    }

    #[test]
    fn validate_rejects_inverted_date_range() {
        let mut m = fixture_valid();
        let (lo, hi) = m.date_range;
        m.date_range = (hi + Duration::days(1), lo);
        assert!(m.validate().unwrap_err().contains("date_range"));
    }

    #[test]
    fn signature_is_order_independent_and_dedup() {
        let s1 = cluster_signature(&["a".into(), "b".into(), "c".into()]);
        let s2 = cluster_signature(&["c".into(), "b".into(), "a".into()]);
        let s3 = cluster_signature(&["c".into(), "b".into(), "a".into(), "a".into()]);
        assert_eq!(s1, s2);
        assert_eq!(s1, s3);
        assert_eq!(s1.len(), 16, "FNV-1a 64-bit hex is 16 chars");
    }

    #[test]
    fn signature_changes_when_members_change() {
        let s1 = cluster_signature(&["a".into(), "b".into()]);
        let s2 = cluster_signature(&["a".into(), "c".into()]);
        assert_ne!(s1, s2);
    }

    #[test]
    fn touch_regeneration_bumps_count_and_time() {
        let mut m = fixture_valid();
        let before = m.last_regenerated_at;
        let t1 = before + Duration::hours(1);
        m.touch_regeneration(t1);
        assert_eq!(m.regeneration_count, 2);
        assert_eq!(m.last_regenerated_at, t1);
    }

    #[test]
    fn extract_hub_handles_missing_and_corrupt() {
        assert!(extract_hub(None).is_none());
        assert!(extract_hub(Some(&serde_json::json!({}))).is_none());
        assert!(extract_hub(Some(&serde_json::json!({"hub": "junk"}))).is_none());

        let m = fixture_valid();
        let wrapped = serde_json::json!({ "hub": m });
        let parsed = extract_hub(Some(&wrapped)).unwrap();
        assert_eq!(parsed.cluster_signature, m.cluster_signature);
    }

    #[test]
    fn merge_hub_preserves_other_keys() {
        let existing = serde_json::json!({
            "decision": { "question": "stays put" },
            "insight": { "insightType": "stays put too" }
        });
        let m = fixture_valid();
        let merged = merge_hub_into_extra(Some(&existing), &m);
        assert_eq!(merged["decision"]["question"], "stays put");
        assert_eq!(merged["insight"]["insightType"], "stays put too");
        assert_eq!(merged["hub"]["clusterSignature"], m.cluster_signature);
    }

    #[test]
    fn merge_hub_creates_object_when_extra_is_missing() {
        let m = fixture_valid();
        let merged = merge_hub_into_extra(None, &m);
        assert!(merged.is_object());
        assert!(merged["hub"]["childIds"].is_array());
    }
}
