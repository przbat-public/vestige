//! memory_health tool — Retention dashboard for memory quality monitoring.
//! v1.9.0: Lightweight alternative to full system_status focused on memory health.
//!
//! Two views of one store, and they answer different questions. Retention is
//! about what is still retrievable; the `quality` section is the wave-5
//! measurement (`docs/SELF-CONTAINED-MEMORY-DESIGN.md` §12) of what was *worth*
//! storing — self-containment, anchor resolvability, use, and the gate's
//! rejections. A store can look healthy here and be full of memories nobody can
//! read six months later, which is the failure the quality numbers exist to make
//! visible.

use std::sync::Arc;
use vestige_core::Storage;

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
}

pub async fn execute(
    storage: &Arc<Storage>,
    _args: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    type HealthSnapshot = (
        f64,
        Vec<(String, i64)>,
        String,
        vestige_core::MemoryStats,
        i64,
        i64,
        vestige_core::storage::MemoryQualityReport,
    );

    // Seven storage reads in a row. Group them into a single blocking task
    // so the async runtime sees exactly one suspension point.
    let storage_clone = storage.clone();
    let (avg_retention, distribution, trend, stats, below_30, below_50, quality) =
        tokio::task::spawn_blocking(move || -> Result<HealthSnapshot, String> {
            let avg_retention = storage_clone
                .get_avg_retention()
                .map_err(|e| format!("Failed to get avg retention: {}", e))?;
            let distribution = storage_clone
                .get_retention_distribution()
                .map_err(|e| format!("Failed to get retention distribution: {}", e))?;
            let trend = storage_clone
                .get_retention_trend()
                .unwrap_or_else(|_| "unknown".to_string());
            let stats = storage_clone
                .get_stats()
                .map_err(|e| format!("Failed to get stats: {}", e))?;
            let below_30 = storage_clone
                .count_memories_below_retention(0.3)
                .unwrap_or(0);
            let below_50 = storage_clone
                .count_memories_below_retention(0.5)
                .unwrap_or(0);
            // No window: this is the state of the store, and a period would make
            // the section a different question than the retention fields beside
            // it. `vestige quality --since` is where a window belongs.
            let quality = storage_clone
                .memory_quality(None)
                .map_err(|e| format!("Failed to measure memory quality: {}", e))?;
            Ok((
                avg_retention,
                distribution,
                trend,
                stats,
                below_30,
                below_50,
                quality,
            ))
        })
        .await
        .map_err(|e| format!("memory_health task panicked: {}", e))??;

    let distribution_json: serde_json::Value = distribution
        .iter()
        .map(|(bucket, count)| serde_json::json!({ "bucket": bucket, "count": count }))
        .collect();

    // Retention target
    let retention_target: f64 = std::env::var("VESTIGE_RETENTION_TARGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.8);

    let meets_target = avg_retention >= retention_target;

    // Generate recommendation
    let recommendation = if avg_retention >= 0.8 {
        "Excellent memory health. Retention is strong across the board."
    } else if avg_retention >= 0.6 {
        "Good memory health. Consider reviewing memories in the 0-40% range."
    } else if avg_retention >= 0.4 {
        "Fair memory health. Many memories are decaying. Run consolidation and consider GC."
    } else {
        "Poor memory health. Urgent: run consolidation, then GC stale memories below 0.3."
    };

    Ok(serde_json::json!({
        "avgRetention": format!("{:.1}%", avg_retention * 100.0),
        "avgRetentionRaw": avg_retention,
        "retentionTarget": retention_target,
        "meetsTarget": meets_target,
        "totalMemories": stats.total_nodes,
        "distribution": distribution_json,
        "trend": trend,
        "memoriesBelow30pct": below_30,
        "memoriesBelow50pct": below_50,
        "recommendation": recommendation,
        // Serialised as it comes out of storage rather than re-shaped here: the
        // `notes` are the traps the numbers avoid, the containment categories
        // carry their own denominator, and a summary assembled in this layer is
        // exactly where `unchecked` gets folded into `clean`.
        "quality": quality,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    #[test]
    fn test_schema_is_valid() {
        let s = schema();
        assert_eq!(s["type"], "object");
    }

    #[tokio::test]
    async fn test_health_empty_database() {
        let (storage, _dir) = test_storage().await;
        let result = execute(&storage, None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["totalMemories"], 0);
        assert!(value["avgRetention"].is_string());
        assert!(value["recommendation"].is_string());
    }

    #[tokio::test]
    async fn test_health_with_memories() {
        let (storage, _dir) = test_storage().await;
        // Ingest some test memories
        for i in 0..5 {
            storage
                .ingest(vestige_core::IngestInput {
                    content: format!("Health test memory {}", i),
                    node_type: "fact".to_string(),
                    source: None,
                    sentiment_score: 0.0,
                    sentiment_magnitude: 0.0,
                    tags: vec!["test".to_string()],
                    valid_from: None,
                    valid_until: None,
                    provenance: None,
                    ..Default::default()
                })
                .unwrap();
        }

        let result = execute(&storage, None).await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["totalMemories"], 5);
        assert!(value["distribution"].is_array());
        assert!(value["meetsTarget"].is_boolean());
    }

    #[tokio::test]
    async fn test_health_distribution_buckets() {
        let (storage, _dir) = test_storage().await;
        storage
            .ingest(vestige_core::IngestInput {
                content: "Test memory for distribution".to_string(),
                node_type: "fact".to_string(),
                source: None,
                sentiment_score: 0.0,
                sentiment_magnitude: 0.0,
                tags: vec![],
                valid_from: None,
                valid_until: None,
                provenance: None,
                ..Default::default()
            })
            .unwrap();

        let result = execute(&storage, None).await.unwrap();
        let dist = result["distribution"].as_array().unwrap();
        // Should have at least one bucket with data
        assert!(!dist.is_empty());
        let total: i64 = dist.iter().map(|b| b["count"].as_i64().unwrap_or(0)).sum();
        assert_eq!(total, 1);
    }

    /// The wave-5 measures ride along with the retention numbers, because
    /// `memory_health` is where an agent already looks and §12 asks for the
    /// numbers where they will be read.
    ///
    /// The three containment categories are asserted apart, not summed: the trap
    /// §12.1 names is that `NULL` ("the gate never ran") gets folded into
    /// `clean`, which lets a store written before the gate existed report a
    /// perfect self-containment rate. The markers here are set directly because
    /// the gate is run by the write paths above this layer — that a real flagged
    /// write lands in these counts is asserted in
    /// `smart_ingest::tests::a_flagged_write_increments_one_counter_per_finding`.
    #[tokio::test]
    async fn quality_carries_the_three_containment_categories_apart() {
        let (storage, _dir) = test_storage().await;
        storage
            .ingest(vestige_core::IngestInput {
                content: "Marek keeps the migration lock in Postgres".to_string(),
                node_type: "fact".to_string(),
                self_contained: Some(true),
                ..Default::default()
            })
            .unwrap();
        storage
            .ingest(vestige_core::IngestInput {
                content: "Deploy the migration next week".to_string(),
                node_type: "fact".to_string(),
                self_contained: Some(false),
                self_contained_findings: Some(serde_json::json!([
                    { "kind": "relative_time", "span": "next week", "hint": "use an absolute date" }
                ])),
                ..Default::default()
            })
            .unwrap();
        storage
            .ingest(vestige_core::IngestInput {
                content: "a memory from before the gate existed".to_string(),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap();

        let value = execute(&storage, None).await.unwrap();
        let quality = &value["quality"];
        assert_eq!(
            quality["containment"]["clean"], 1,
            "the gate passed one memory: {value}"
        );
        assert_eq!(
            quality["containment"]["flagged"], 1,
            "the gate flagged one and it was written anyway: {value}"
        );
        assert_eq!(
            quality["containment"]["unchecked"], 1,
            "a memory the gate never saw is not a memory the gate passed: {value}"
        );
        assert_eq!(quality["containment"]["total"], 3, "{value}");
        // The rate travels with the counts and is derived here the same way, so
        // that a consumer never has to decide the denominator itself: the
        // question §12.1 warns about is exactly whether `unchecked` belongs in
        // it, and the answer has to be in the payload rather than in each
        // reader's head. Both routes must agree.
        let clean = quality["containment"]["clean"].as_f64().unwrap();
        let checked = clean + quality["containment"]["flagged"].as_f64().unwrap();
        assert_eq!(
            clean / checked,
            0.5,
            "the rate is over what the gate checked, never over `unchecked`: {value}"
        );
        assert_eq!(
            quality["rates"]["selfContainment"], 0.5,
            "the payload carries the rate the docs name: {value}"
        );
        assert!(
            quality["rates"]["anchorResolvability"].is_null(),
            "no anchors in this store means no measurement, not zero percent: {value}"
        );
        assert_eq!(
            quality["flaggedByKind"][0]["kind"], "relative_time",
            "the rule breakdown reaches the reader: {value}"
        );
        assert_eq!(
            quality["windowBasis"], "recorded_at",
            "the window is the moment we wrote it down: {value}"
        );
        assert!(
            quality["notes"].as_array().is_some_and(|n| !n.is_empty()),
            "the traps have to reach the reader, not stay in a code comment: {value}"
        );
        assert!(
            quality["useCounts"]["everAccessed"].is_number(),
            "the use measure travels with the containment one: {value}"
        );
    }

    /// The existing retention fields keep working beside the new section: a
    /// caller that only ever read `avgRetention` must not have to change.
    #[tokio::test]
    async fn the_retention_fields_survive_the_quality_section() {
        let (storage, _dir) = test_storage().await;
        let value = execute(&storage, None).await.unwrap();
        for field in [
            "avgRetention",
            "avgRetentionRaw",
            "retentionTarget",
            "meetsTarget",
            "totalMemories",
            "distribution",
            "trend",
            "recommendation",
        ] {
            assert!(
                !value[field].is_null(),
                "{field} disappeared from memory_health: {value}"
            );
        }
        assert!(
            value["quality"]["containment"]["total"] == 0
                && value["quality"]["anchors"]["total"] == 0
                && value["quality"]["useCounts"]["total"] == 0,
            "an empty window reports zero counts and no rate — no measurement, not zero percent: {value}"
        );
    }
}
