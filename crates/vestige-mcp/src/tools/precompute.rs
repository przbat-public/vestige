//! Precompute-for-Context Tool — "sleep-time compute".
//!
//! Repositions the dream cycle from passive consolidation to **active
//! pre-fetch**: the agent announces "I expect to work on X" before the next
//! query, and the tool materializes a summary memory the next `search` /
//! `session_context` / `deep_reference` call hits for free.
//!
//! Workflow:
//!   1. Hybrid-search the topic (falls back to keyword search when the
//!      embedding service isn't compiled in or hasn't warmed up).
//!   2. Pick top-K source memories.
//!   3. Store a summary as a fresh memory with:
//!      - `node_type = "precomputed_summary"` so dashboards / search can
//!        deprioritise it after expiry,
//!      - `tags = ["precomputed", "topic:<slug>"]` so retrieval queries can
//!        whitelist it explicitly,
//!      - `valid_until = now + ttl_hours` so the existing `temporal` /
//!        `gc` machinery can age it out without bespoke code,
//!      - `provenance.derived_from = [source_ids]` so the audit trail keeps
//!        pointing back at the underlying facts.
//!   4. Return `{ summary_id, expires_at, topic, source_count, … }`.
//!
//! See `ARCHITECTURE.md` "Sleep-time compute" section for the rationale —
//! precomputing is cheaper than re-running the cognitive pipeline on every
//! follow-up question.

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::cognitive::CognitiveEngine;
use vestige_core::{IngestInput, Storage};

const DEFAULT_TOP_K: i64 = 5;
const MIN_TOP_K: i64 = 1;
const MAX_TOP_K: i64 = 20;
const DEFAULT_TTL_HOURS: i64 = 24;
const MIN_TTL_HOURS: i64 = 1;
/// Seven days. Anything longer should go through `smart_ingest` directly,
/// because pretending a heuristic summary is fresh for >1 week erodes trust
/// in the precomputed-summary node_type.
const MAX_TTL_HOURS: i64 = 168;

pub fn schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "topic": {
                "type": "string",
                "description": "Topic to pre-compute. Be specific (`OAuth2 token refresh` beats `auth`)."
            },
            "top_k": {
                "type": "integer",
                "description": "Number of source memories to fold into the summary.",
                "minimum": MIN_TOP_K,
                "maximum": MAX_TOP_K,
                "default": DEFAULT_TOP_K
            },
            "ttl_hours": {
                "type": "integer",
                "description": "How long the precomputed summary stays valid. Clamped to 1..=168 (7 days). Past `ttl_hours` the entry is treated as expired by `temporal` and search demotes it.",
                "minimum": MIN_TTL_HOURS,
                "maximum": MAX_TTL_HOURS,
                "default": DEFAULT_TTL_HOURS
            }
        },
        "required": ["topic"]
    })
}

pub async fn execute(
    storage: &Arc<Storage>,
    _cognitive: &Arc<Mutex<CognitiveEngine>>,
    args: Option<Value>,
) -> Result<Value, String> {
    let args = args.ok_or("Missing arguments")?;

    let topic_raw = args
        .get("topic")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'topic'")?;
    let topic = topic_raw.trim();
    if topic.is_empty() {
        return Err("'topic' cannot be empty".into());
    }

    let top_k = parse_int(&args, "top_k", "topK")
        .unwrap_or(DEFAULT_TOP_K)
        .clamp(MIN_TOP_K, MAX_TOP_K) as i32;

    let ttl_hours = parse_int(&args, "ttl_hours", "ttlHours")
        .unwrap_or(DEFAULT_TTL_HOURS)
        .clamp(MIN_TTL_HOURS, MAX_TTL_HOURS);

    let sources = fetch_sources(storage, topic, top_k)?;

    if sources.is_empty() {
        return Ok(serde_json::json!({
            "status": "no_sources",
            "topic": topic,
            "topic_slug": slugify_topic(topic),
            "summary_id": Value::Null,
            "source_count": 0,
            "source_ids": [],
            "message": format!(
                "No memories matched topic `{}`; nothing to precompute.",
                topic
            ),
        }));
    }

    let now = Utc::now();
    let expires_at = now + Duration::hours(ttl_hours);

    let source_ids: Vec<String> = sources.iter().map(|s| s.id.clone()).collect();
    let topic_slug = slugify_topic(topic);

    let mut preview_lines = String::new();
    for (i, src) in sources.iter().enumerate() {
        let snippet = first_chars(&src.content, 200);
        preview_lines.push_str(&format!("{}. {} (id: {})\n", i + 1, snippet, src.id));
    }

    let content = format!(
        "[Precomputed summary for \"{}\" — {} sources, valid until {}]\n\n{}",
        topic,
        sources.len(),
        expires_at.to_rfc3339(),
        preview_lines.trim_end(),
    );

    let provenance = serde_json::json!({
        "derived_from": source_ids,
        "topic": topic,
        "topic_slug": topic_slug,
        "generated_at": now.to_rfc3339(),
        "generator": "precompute_for_context",
        "ttl_hours": ttl_hours,
    });

    let input = IngestInput {
        content: content.clone(),
        node_type: "precomputed_summary".to_string(),
        source: Some("precompute_for_context".to_string()),
        sentiment_score: 0.0,
        sentiment_magnitude: 0.0,
        tags: vec!["precomputed".to_string(), format!("topic:{}", topic_slug)],
        valid_from: Some(now),
        valid_until: Some(expires_at),
        provenance: Some(provenance),
        ..Default::default()
    };

    let saved = {
        let storage = Arc::clone(storage);
        tokio::task::spawn_blocking(move || storage.ingest(input))
            .await
            .map_err(|e| format!("Spawn join failed: {}", e))?
    }
    .map_err(|e| format!("Failed to persist precomputed summary: {}", e))?;

    Ok(serde_json::json!({
        "status": "saved",
        "summary_id": saved.id,
        "topic": topic,
        "topic_slug": topic_slug,
        "expires_at": expires_at.to_rfc3339(),
        "ttl_hours": ttl_hours,
        "source_count": source_ids.len(),
        "source_ids": source_ids,
        "preview": first_chars(&content, 400),
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Reads either snake_case or camelCase from the arg object. The MCP spec
/// is camelCase friendly, but every other tool in this crate accepts the
/// snake_case form too, so we keep parity.
fn parse_int(args: &Value, snake: &str, camel: &str) -> Option<i64> {
    args.get(snake)
        .or_else(|| args.get(camel))
        .and_then(|v| v.as_i64())
}

struct SourceSnippet {
    id: String,
    content: String,
}

/// Pulls top-K candidate memories for `topic`. With `embeddings` +
/// `vector-search` we run the full hybrid (RRF-fused) pipeline; otherwise
/// we keyword-only. Both paths converge on `(id, content)` so the caller
/// doesn't need to care which branch fired.
fn fetch_sources(
    storage: &Arc<Storage>,
    topic: &str,
    top_k: i32,
) -> Result<Vec<SourceSnippet>, String> {
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        let results = storage
            .hybrid_search(topic, top_k, 0.3, 0.7)
            .map_err(|e| format!("Hybrid search failed: {}", e))?;
        Ok(results
            .into_iter()
            .map(|r| SourceSnippet {
                id: r.node.id,
                content: r.node.content,
            })
            .collect())
    }
    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        let nodes = storage
            .keyword_search(topic, top_k, 0.0)
            .map_err(|e| format!("Keyword search failed: {}", e))?;
        Ok(nodes
            .into_iter()
            .map(|n| SourceSnippet {
                id: n.id,
                content: n.content,
            })
            .collect())
    }
}

fn first_chars(s: &str, n: usize) -> String {
    let collected: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{}…", collected)
    } else {
        collected
    }
}

/// Best-effort slug. Lowercases, keeps ASCII alphanumerics, collapses every
/// other rune to a single dash, trims leading/trailing dashes. Non-ASCII
/// scripts (`日本語`, `cyrillic`) collapse to `unknown` rather than emit an
/// empty slug — the slug is only a tag handle so the topic field is the
/// authoritative source for display.
fn slugify_topic(topic: &str) -> String {
    let mut slug = String::with_capacity(topic.len());
    let mut prev_dash = true;
    for ch in topic.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("unknown");
    }
    slug
}

// ---------------------------------------------------------------------------
// Tests — RED/GREEN.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::CognitiveEngine;
    use tempfile::TempDir;

    fn test_cognitive() -> Arc<Mutex<CognitiveEngine>> {
        Arc::new(Mutex::new(CognitiveEngine::new()))
    }

    async fn test_storage() -> (Arc<Storage>, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(Some(dir.path().join("test.db"))).unwrap();
        (Arc::new(storage), dir)
    }

    async fn seed(storage: &Arc<Storage>, content: &str, tags: Vec<&str>) -> String {
        let input = IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            tags: tags.into_iter().map(String::from).collect(),
            ..Default::default()
        };
        let storage_w = Arc::clone(storage);
        tokio::task::spawn_blocking(move || storage_w.ingest(input))
            .await
            .unwrap()
            .unwrap()
            .id
    }

    // ---- Schema --------------------------------------------------------

    #[test]
    fn schema_requires_topic_only() {
        let s = schema();
        assert_eq!(s["type"], "object");
        let required = s["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "topic");
        assert!(s["properties"]["top_k"].is_object());
        assert!(s["properties"]["ttl_hours"].is_object());
    }

    #[test]
    fn schema_documents_clamps() {
        let s = schema();
        assert_eq!(s["properties"]["top_k"]["minimum"], MIN_TOP_K);
        assert_eq!(s["properties"]["top_k"]["maximum"], MAX_TOP_K);
        assert_eq!(s["properties"]["ttl_hours"]["minimum"], MIN_TTL_HOURS);
        assert_eq!(s["properties"]["ttl_hours"]["maximum"], MAX_TTL_HOURS);
    }

    // ---- Validation ----------------------------------------------------

    #[tokio::test]
    async fn rejects_missing_arguments() {
        let (storage, _dir) = test_storage().await;
        let err = execute(&storage, &test_cognitive(), None)
            .await
            .expect_err("missing args must reject");
        assert!(err.contains("Missing arguments"), "got: {err}");
    }

    #[tokio::test]
    async fn rejects_empty_topic() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "topic": "   " });
        let err = execute(&storage, &test_cognitive(), Some(args))
            .await
            .expect_err("empty topic must reject");
        assert!(err.contains("'topic' cannot be empty"), "got: {err}");
    }

    // ---- Empty result --------------------------------------------------

    #[tokio::test]
    async fn returns_no_sources_when_search_empty() {
        let (storage, _dir) = test_storage().await;
        let args = serde_json::json!({ "topic": "novel-topic-xyz-no-match" });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        assert_eq!(result["status"], "no_sources");
        assert_eq!(result["source_count"], 0);
        assert!(result["summary_id"].is_null());
        // A `no_sources` reply must NOT persist a memory.
        let stats = storage.get_stats().unwrap();
        assert_eq!(
            stats.total_nodes, 0,
            "no_sources path leaked a precomputed memory: {:?}",
            stats
        );
    }

    // ---- Happy path ----------------------------------------------------

    #[tokio::test]
    async fn creates_summary_with_precomputed_node_type_and_tags() {
        let (storage, _dir) = test_storage().await;
        seed(
            &storage,
            "OAuth2 token refresh requires PKCE for public clients",
            vec!["auth"],
        )
        .await;
        seed(
            &storage,
            "OAuth2 refresh tokens rotate on every use by default",
            vec!["auth"],
        )
        .await;

        let args = serde_json::json!({ "topic": "OAuth2 refresh" });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        assert_eq!(result["status"], "saved");

        let summary_id = result["summary_id"].as_str().unwrap();
        let node = storage.get_node(summary_id).unwrap().expect("must exist");
        assert_eq!(node.node_type, "precomputed_summary");
        assert!(
            node.tags.iter().any(|t| t == "precomputed"),
            "tags missing `precomputed`: {:?}",
            node.tags
        );
        assert!(
            node.tags.iter().any(|t| t.starts_with("topic:")),
            "tags missing topic slug: {:?}",
            node.tags
        );
    }

    #[tokio::test]
    async fn sets_valid_until_to_now_plus_ttl() {
        let (storage, _dir) = test_storage().await;
        seed(&storage, "PKCE prevents auth-code interception attacks", vec![]).await;

        let before = Utc::now();
        let args = serde_json::json!({ "topic": "PKCE", "ttl_hours": 3 });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        let after = Utc::now();

        let id = result["summary_id"].as_str().unwrap();
        let node = storage.get_node(id).unwrap().expect("must exist");
        let valid_until = node.valid_until.expect("must have TTL");

        let min_expected = before + Duration::hours(3) - Duration::seconds(5);
        let max_expected = after + Duration::hours(3) + Duration::seconds(5);
        assert!(
            valid_until >= min_expected,
            "valid_until too early: {valid_until} < {min_expected}",
        );
        assert!(
            valid_until <= max_expected,
            "valid_until too late: {valid_until} > {max_expected}",
        );
    }

    #[tokio::test]
    async fn clamps_ttl_hours_to_max_168() {
        let (storage, _dir) = test_storage().await;
        seed(&storage, "Some content about TTL clamping", vec![]).await;

        let args = serde_json::json!({ "topic": "TTL", "ttl_hours": 999_999 });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        assert_eq!(result["ttl_hours"], MAX_TTL_HOURS);
    }

    #[tokio::test]
    async fn clamps_ttl_hours_to_min_1() {
        let (storage, _dir) = test_storage().await;
        seed(&storage, "Some content for min-ttl clamp", vec![]).await;

        let args = serde_json::json!({ "topic": "min", "ttl_hours": 0 });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        assert_eq!(result["ttl_hours"], MIN_TTL_HOURS);
    }

    #[tokio::test]
    async fn clamps_top_k_to_max_20() {
        let (storage, _dir) = test_storage().await;
        for i in 0..30 {
            seed(&storage, &format!("topic K source {i}"), vec![]).await;
        }
        let args = serde_json::json!({ "topic": "topic K source", "top_k": 999 });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        let count = result["source_count"].as_u64().unwrap();
        assert!(
            count <= MAX_TOP_K as u64,
            "top_k clamp leaked: {} > {}",
            count,
            MAX_TOP_K,
        );
    }

    #[tokio::test]
    async fn provenance_lists_source_ids_on_persisted_memory() {
        let (storage, _dir) = test_storage().await;
        let id1 = seed(&storage, "Cache invalidation is hard", vec![]).await;
        let id2 = seed(&storage, "Cache lookup cost grows with key cardinality", vec![]).await;

        let args = serde_json::json!({ "topic": "cache" });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        let response_sources: Vec<String> = result["source_ids"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        assert!(
            response_sources.contains(&id1) || response_sources.contains(&id2),
            "response source_ids must include at least one seeded id; got {:?}",
            response_sources,
        );

        let summary_id = result["summary_id"].as_str().unwrap();
        let node = storage.get_node(summary_id).unwrap().expect("must exist");
        let prov = node.provenance.expect("must persist provenance");
        let derived_from = prov
            .get("derived_from")
            .and_then(|v| v.as_array())
            .expect("derived_from must be an array");
        assert!(
            !derived_from.is_empty(),
            "derived_from must not be empty when sources existed",
        );
        assert_eq!(
            prov.get("generator").and_then(|v| v.as_str()),
            Some("precompute_for_context"),
        );
    }

    // ---- Slug normalisation -------------------------------------------

    #[tokio::test]
    async fn slug_normalises_whitespace_and_punctuation() {
        let (storage, _dir) = test_storage().await;
        seed(&storage, "Slug normalisation source", vec![]).await;

        let args = serde_json::json!({ "topic": "  OAuth 2.0 / refresh  " });
        let result = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();
        let slug = result["topic_slug"].as_str().unwrap();
        assert!(
            slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "slug must be ascii-alnum + dash: `{slug}`",
        );
        assert!(!slug.starts_with('-') && !slug.ends_with('-'), "slug `{slug}` must not have boundary dashes");
        assert!(!slug.contains("--"), "slug `{slug}` should collapse repeated dashes");
    }

    #[test]
    fn slug_falls_back_to_unknown_for_non_ascii_only_input() {
        assert_eq!(slugify_topic("日本語"), "unknown");
        assert_eq!(slugify_topic("  "), "unknown");
    }

    // ---- Idempotence guard (cosmetic — every call adds a new memory) --

    #[tokio::test]
    async fn second_invocation_creates_another_summary() {
        let (storage, _dir) = test_storage().await;
        seed(&storage, "Idempotence test source one", vec![]).await;

        let args = serde_json::json!({ "topic": "Idempotence test" });
        let first = execute(&storage, &test_cognitive(), Some(args.clone()))
            .await
            .unwrap();
        let second = execute(&storage, &test_cognitive(), Some(args))
            .await
            .unwrap();

        let first_id = first["summary_id"].as_str().unwrap();
        let second_id = second["summary_id"].as_str().unwrap();
        assert_ne!(
            first_id, second_id,
            "every precompute call must mint a fresh memory; users rely on this to track expiry",
        );
    }
}
