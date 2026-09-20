//! Dashboard handlers — stats, health, retention distribution, metrics
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use std::collections::HashMap;

use super::super::state::AppState;
use super::super::wire::{
    DashboardLimitsDto, EndangeredMemoryDto, HealthCheckDto, HealthStatus, RetentionBucketDto,
    RetentionDistributionDto, SystemStatsDto,
};
use super::{log_err, log_join_err};

/// Get system stats — totals, averages, embedding coverage.
pub async fn get_stats(State(state): State<AppState>) -> Result<Json<SystemStatsDto>, StatusCode> {
    let storage = state.storage.clone();
    let stats = tokio::task::spawn_blocking(move || storage.get_stats())
        .await
        .map_err(log_join_err("get_stats task panicked"))?
        .map_err(log_err("storage operation"))?;

    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(SystemStatsDto {
        total_memories: stats.total_nodes,
        due_for_review: stats.nodes_due_for_review,
        average_retention: stats.average_retention,
        average_storage_strength: stats.average_storage_strength,
        average_retrieval_strength: stats.average_retrieval_strength,
        with_embeddings: stats.nodes_with_embeddings,
        embedding_coverage,
        embedding_model: stats.embedding_model.unwrap_or_default(),
        oldest_memory: stats.oldest_memory.map(|dt| dt.to_rfc3339()),
        newest_memory: stats.newest_memory.map(|dt| dt.to_rfc3339()),
    }))
}

/// Health check — derives a single-word status from the average
/// retention. Mirrors the legacy threshold ladder
/// (empty → critical < 0.3 → degraded < 0.5 → healthy).
///
/// The status is also mapped onto the HTTP status code (see
/// [`health_status_code`]): answering 200 for `critical` told every container
/// probe and load balancer that a memory base with almost nothing retrievable
/// was healthy, so no alert and no restart ever fired.
pub async fn health_check(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<HealthCheckDto>), StatusCode> {
    let storage = state.storage.clone();
    let stats = tokio::task::spawn_blocking(move || storage.get_stats())
        .await
        .map_err(log_join_err("get_stats task panicked"))?
        .map_err(log_err("storage operation"))?;

    let status = if stats.total_nodes == 0 {
        HealthStatus::Empty
    } else if stats.average_retention < 0.3 {
        HealthStatus::Critical
    } else if stats.average_retention < 0.5 {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    };

    // The DTO is returned for `critical` too: a 503 with a body is strictly
    // more informative than a 503 without one, and an operator curling the
    // endpoint should see *why* it is failing rather than a bare status line.
    Ok((
        health_status_code(status),
        Json(HealthCheckDto {
            status,
            total_memories: stats.total_nodes,
            average_retention: stats.average_retention,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
    ))
}

/// Map a health status onto the status code a probe should see.
///
/// - `healthy` → 200. Nothing to do.
/// - `empty` → 200. A fresh install with zero memories is not an outage;
///   `empty` is a lifecycle stage, and failing readiness here would make the
///   first run look broken.
/// - `degraded` → 200, deliberately. Degraded means average retention slipped
///   below 0.5 — the documented remedy is `gc`/consolidation, not a restart. A
///   503 would make orchestrators kill a process that is serving every request
///   correctly, and a restart cannot raise retention. The degradation is
///   reported through the body (`status`) and through `/metrics`, which is
///   where an alert on retention belongs.
/// - `critical` → 503. Under 0.3 average retention the store is barely usable,
///   so the service is not ready to be part of a serving set; this is the one
///   status where a load balancer must stop sending traffic.
///
/// `HealthStatus` is `#[non_exhaustive]`-free, so a new variant will fail to
/// compile here instead of silently defaulting to 200.
pub(crate) fn health_status_code(status: HealthStatus) -> StatusCode {
    match status {
        HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Empty => StatusCode::OK,
        HealthStatus::Critical => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Prometheus text exposition for `GET /metrics`.
///
/// Hand-rendered on purpose: the process already tracks everything worth
/// scraping (storage stats plus the per-stage `try_lock` skip counters), so a
/// metrics crate would buy a second registry and a dependency for ~20 lines of
/// formatting. Content type is the format's own
/// (`text/plain; version=0.0.4`), which is what every scraper sniffs.
///
/// Series:
/// - `vestige_up` — 1 while the process answers scrapes (the `up == 0` alert
///   convention when a scrape fails outright),
/// - `vestige_memories`, `vestige_memories_due_for_review`,
///   `vestige_memories_with_embeddings`, `vestige_embedding_coverage_ratio`,
///   `vestige_average_retention` — the store's shape,
/// - `vestige_try_lock_skips_total{stage}` — cognitive-engine enrichments
///   dropped because `try_lock()` was contended, the one counter that says
///   "the engine is saturated and searches are running degraded",
/// - `vestige_gate_outcomes_total{outcome,kind}` — self-containedness gate
///   verdicts from **this process's lifetime**: writes it refused and memories
///   it flagged before they were written anyway. A refused write stores nothing
///   — that is the point of the reject path — so the store cannot remember it
///   and this is the only witness; the numbers reset with a restart,
/// - `vestige_websocket_subscribers`, `vestige_dashboard_uptime_seconds`.
pub async fn metrics(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let storage = state.storage.clone();
    let stats = tokio::task::spawn_blocking(move || storage.get_stats())
        .await
        .map_err(log_join_err("get_stats task panicked"))?
        .map_err(log_err("storage operation"))?;

    let embedding_coverage = if stats.total_nodes > 0 {
        stats.nodes_with_embeddings as f64 / stats.total_nodes as f64
    } else {
        0.0
    };

    let mut body = String::with_capacity(1024);
    gauge(
        &mut body,
        "vestige_up",
        "1 while the Vestige process is answering scrapes.",
        1.0,
    );
    gauge(
        &mut body,
        "vestige_memories",
        "Memories currently stored.",
        stats.total_nodes as f64,
    );
    gauge(
        &mut body,
        "vestige_memories_due_for_review",
        "Memories whose next review date has passed.",
        stats.nodes_due_for_review as f64,
    );
    gauge(
        &mut body,
        "vestige_memories_with_embeddings",
        "Memories that have a semantic embedding.",
        stats.nodes_with_embeddings as f64,
    );
    gauge(
        &mut body,
        "vestige_embedding_coverage_ratio",
        "Fraction of memories with an embedding (0..1).",
        embedding_coverage,
    );
    gauge(
        &mut body,
        "vestige_average_retention",
        "Average FSRS retention strength across all memories (0..1). \
         Below 0.3 the /api/health status is critical and answers 503.",
        stats.average_retention,
    );

    body.push_str(
        "# HELP vestige_try_lock_skips_total Cognitive-engine stages skipped because try_lock() \
         was contended; a rising rate means searches are running without enrichment.\n",
    );
    body.push_str("# TYPE vestige_try_lock_skips_total counter\n");
    for (stage, count) in crate::cognitive::try_lock_metrics::snapshot() {
        body.push_str(&format!(
            "vestige_try_lock_skips_total{{stage=\"{}\"}} {}\n",
            escape_label_value(stage),
            count
        ));
    }

    // Process-lifetime counters, and the HELP says so: a refusal is stored
    // nowhere by design, so a reader who mistook this for a store-wide rate
    // would conclude that a restart erased the refusals — which it does.
    body.push_str(
        "# HELP vestige_gate_outcomes_total Self-containedness gate verdicts seen by this \
         process since it started: writes refused (nothing was stored, so no other surface can \
         report them) and memories flagged before being written anyway, broken down by rule.\n",
    );
    body.push_str("# TYPE vestige_gate_outcomes_total counter\n");
    let gate = vestige_core::storage::GateOutcomeCounters::global().snapshot();
    counter_by_kind(&mut body, "rejected", &gate.rejected_by_kind);
    counter_by_kind(&mut body, "flagged", &gate.flagged_by_kind);

    gauge(
        &mut body,
        "vestige_websocket_subscribers",
        "Dashboard WebSocket clients currently subscribed to the event stream.",
        state.event_tx.receiver_count() as f64,
    );
    gauge(
        &mut body,
        "vestige_dashboard_uptime_seconds",
        "Seconds since the dashboard state was created.",
        state.start_time.elapsed().as_secs_f64(),
    );

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response())
}

/// Append one `# HELP` / `# TYPE` / sample triple for a gauge.
fn gauge(body: &mut String, name: &str, help: &str, value: f64) {
    body.push_str(&format!("# HELP {name} {help}\n"));
    body.push_str(&format!("# TYPE {name} gauge\n"));
    body.push_str(&format!("{name} {value}\n"));
}

/// Escape a Prometheus label value: backslash, double quote and newline are
/// the only characters the format reserves. Stage names are static snake_case
/// today, but the renderer must stay correct for a label that is not.
fn escape_label_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Append one family's samples, one line per rule.
///
/// One series per `(outcome, kind)` rather than one total per outcome: the
/// question §12.1 asks is *which rule* is firing, and a total that says "14
/// refusals" cannot tell a gate that is too loud about `no_subject` from one
/// catching copies of the repository. Zero-count kinds are simply absent — the
/// registry only learns a kind by seeing it, and inventing rows for rules that
/// never fired would make a fresh process look like it had measured something.
fn counter_by_kind(body: &mut String, outcome: &str, rules: &[vestige_core::storage::RuleCount]) {
    for rule in rules {
        body.push_str(&format!(
            "vestige_gate_outcomes_total{{outcome=\"{}\",kind=\"{}\"}} {}\n",
            escape_label_value(outcome),
            escape_label_value(&rule.kind),
            rule.count
        ));
    }
}

/// Get retention distribution (for histogram + heatmap visualization).
pub async fn retention_distribution(
    State(state): State<AppState>,
) -> Result<Json<RetentionDistributionDto>, StatusCode> {
    // Cap at 1000 to prevent excessive memory usage on large databases.
    // Even capped, a full SELECT + per-row deserialization runs >10ms on
    // a warm DB, so dispatch to the blocking pool.
    let storage = state.storage.clone();
    let nodes = tokio::task::spawn_blocking(move || storage.get_all_nodes(1000, 0))
        .await
        .map_err(log_join_err("get_all_nodes task panicked"))?
        .map_err(log_err("storage operation"))?;

    let mut buckets = [0u32; 10];
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut endangered = Vec::new();

    for node in &nodes {
        let bucket = ((node.retention_strength * 10.0).floor() as usize).min(9);
        buckets[bucket] += 1;
        *by_type.entry(node.node_type.clone()).or_default() += 1;

        if node.retention_strength < 0.3 {
            endangered.push(EndangeredMemoryDto {
                id: node.id.clone(),
                content: node.content.chars().take(60).collect::<String>(),
                retention: node.retention_strength,
                node_type: node.node_type.clone(),
            });
        }
    }

    let distribution: Vec<RetentionBucketDto> = buckets
        .iter()
        .enumerate()
        .map(|(i, &count)| RetentionBucketDto {
            range: format!("{}-{}%", i * 10, (i + 1) * 10),
            count,
        })
        .collect();

    Ok(Json(RetentionDistributionDto {
        distribution,
        by_type,
        endangered,
        total: nodes.len(),
    }))
}

/// Expose the dashboard's compile-time limits at `GET /api/_meta/limits`.
///
/// The frontend fetches this once on boot — used to seed the graph
/// node-cap input, the search "load more" threshold, and the WebSocket
/// event ring buffer. Living next to the constants in
/// `wire::limits::DashboardLimitsDto::DEFAULT` keeps backend clamp
/// values and frontend defaults in lockstep.
pub async fn get_limits() -> Json<DashboardLimitsDto> {
    Json(DashboardLimitsDto::DEFAULT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, header};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tower::ServiceExt;
    use vestige_core::{IngestInput, Storage};

    /// Same port the router is normally built with, so the OriginGuard layer
    /// sees the same-origin Host it expects.
    const PORT: u16 = 3927;

    fn test_storage(path: &std::path::Path) -> Arc<Storage> {
        Arc::new(Storage::new(Some(path.to_path_buf())).unwrap())
    }

    fn ingest_one(storage: &Storage) {
        storage
            .ingest(IngestInput {
                content: "a memory whose retention the test controls".to_string(),
                node_type: "fact".to_string(),
                ..Default::default()
            })
            .unwrap();
    }

    /// Write a retention value straight into the database.
    ///
    /// `get_stats` averages whatever is in `knowledge_nodes`; the FSRS writers
    /// are the only public way to move that number, and they move it by small
    /// decay steps. A second connection to the same file sets the exact value
    /// the health ladder branches on, which is what these tests are about.
    fn set_retention(path: &std::path::Path, value: f64) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute(
            "UPDATE knowledge_nodes SET retention_strength = ?1",
            rusqlite::params![value],
        )
        .unwrap();
    }

    async fn get(storage: Arc<Storage>, uri: &str) -> axum::response::Response {
        let (app, _state): (Router, AppState) = crate::dashboard::build_router(storage, None, PORT);
        app.oneshot(
            Request::builder()
                .uri(uri)
                .header(header::HOST, format!("127.0.0.1:{PORT}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
    }

    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ---- health: status -> HTTP status code -------------------------------

    #[test]
    fn only_critical_maps_to_a_failure_code() {
        // The finding: every status answered 200, so a critical memory base
        // looked healthy to probes and no alert could fire.
        assert_eq!(health_status_code(HealthStatus::Healthy), StatusCode::OK);
        assert_eq!(
            health_status_code(HealthStatus::Degraded),
            StatusCode::OK,
            "degraded is a maintenance signal (run gc), not an outage"
        );
        assert_eq!(health_status_code(HealthStatus::Empty), StatusCode::OK);
        assert_eq!(
            health_status_code(HealthStatus::Critical),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn health_endpoint_answers_200_for_an_empty_store() {
        let dir = TempDir::new().unwrap();
        let storage = test_storage(&dir.path().join("empty.db"));

        let response = get(storage, "/api/health").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["status"], "empty");
    }

    #[tokio::test]
    async fn health_endpoint_tracks_the_retention_ladder() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("health.db");
        let storage = test_storage(&db);
        ingest_one(&storage);

        set_retention(&db, 0.9);
        let response = get(storage.clone(), "/api/health").await;
        assert_eq!(response.status(), StatusCode::OK, "healthy -> 200");
        assert_eq!(body_json(response).await["status"], "healthy");

        set_retention(&db, 0.4);
        let response = get(storage.clone(), "/api/health").await;
        assert_eq!(response.status(), StatusCode::OK, "degraded -> 200");
        assert_eq!(body_json(response).await["status"], "degraded");

        set_retention(&db, 0.1);
        let response = get(storage, "/api/health").await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "critical -> 503"
        );
        // The body survives the failure code: an operator curling the endpoint
        // must still see which status and which retention triggered it.
        let body = body_json(response).await;
        assert_eq!(body["status"], "critical");
        assert_eq!(body["totalMemories"], 1);
    }

    // ---- metrics ----------------------------------------------------------

    /// Strict minimal parser for Prometheus exposition format v0.0.4.
    ///
    /// Deliberately hand-written (no regex dependency): the assertion being
    /// made is "this body is valid exposition text", so the parser has to
    /// reject what a scraper would reject instead of just searching for a
    /// substring. Returns every `(series, value)` sample line.
    fn parse_prometheus(body: &str) -> Vec<(String, f64)> {
        let mut samples = Vec::new();
        for line in body.lines() {
            if line.is_empty() {
                continue;
            }
            if let Some(directive) = line.strip_prefix("# ") {
                assert!(
                    directive.starts_with("HELP ") || directive.starts_with("TYPE "),
                    "unknown directive: {line:?}"
                );
                continue;
            }
            assert!(!line.starts_with('#'), "malformed directive: {line:?}");

            let (series, value) = if let Some(open) = line.find('{') {
                let close = line.find('}').expect("unterminated label set");
                assert!(close > open, "empty label set: {line:?}");
                for pair in line[open + 1..close].split(',') {
                    let (key, value) = pair.split_once('=').expect("label without '='");
                    assert!(
                        !key.is_empty()
                            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                        "bad label name in {line:?}"
                    );
                    assert!(
                        value.starts_with('"') && value.ends_with('"') && value.len() >= 2,
                        "label value must be quoted in {line:?}"
                    );
                }
                (&line[..open], &line[close + 1..])
            } else {
                line.split_once(' ')
                    .unwrap_or_else(|| panic!("sample line without a value: {line:?}"))
            };

            let series = series.trim();
            assert!(
                !series.is_empty()
                    && !series.starts_with(|c: char| c.is_ascii_digit())
                    && series
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':'),
                "invalid metric name {series:?}"
            );
            let value: f64 = value
                .trim()
                .parse()
                .unwrap_or_else(|e| panic!("sample value is not a float in {line:?}: {e}"));
            samples.push((series.to_string(), value));
        }
        samples
    }

    #[tokio::test]
    async fn metrics_are_prometheus_text_with_known_series() {
        let dir = TempDir::new().unwrap();
        let storage = test_storage(&dir.path().join("metrics.db"));
        ingest_one(&storage);

        let response = get(storage, "/metrics").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/plain; version=0.0.4; charset=utf-8",
            "a scraper sniffs the format from the content type"
        );

        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        let samples = parse_prometheus(&body);
        let find = |name: &str| {
            samples
                .iter()
                .find(|(series, _)| series == name)
                .unwrap_or_else(|| panic!("series {name} missing from:\n{body}"))
                .1
        };

        assert_eq!(find("vestige_up"), 1.0, "body:\n{body}");
        assert_eq!(find("vestige_memories"), 1.0, "body:\n{body}");
        // Whether the ingest path managed to embed the node depends on whether
        // the local embedding service is available in this build, so pin the
        // *relationship* the endpoint promises rather than the model's
        // presence: coverage is exactly `with_embeddings / memories`.
        let with_embeddings = find("vestige_memories_with_embeddings");
        assert!(
            (0.0..=1.0).contains(&with_embeddings),
            "with_embeddings out of range: {with_embeddings}\n{body}"
        );
        let coverage = find("vestige_embedding_coverage_ratio");
        assert_eq!(
            coverage, with_embeddings,
            "coverage must be with_embeddings / memories\n{body}"
        );
        assert!(find("vestige_dashboard_uptime_seconds") >= 0.0);
        // The per-stage contention counters are the reason this endpoint
        // exists — they were previously visible only through `system_status`.
        assert!(
            samples
                .iter()
                .any(|(series, _)| series == "vestige_try_lock_skips_total"),
            "try_lock counters must be scrapeable"
        );

        for family in [
            "vestige_memories",
            "vestige_try_lock_skips_total",
            "vestige_dashboard_uptime_seconds",
        ] {
            assert!(
                body.contains(&format!("# TYPE {family} ")),
                "{family} must declare its type:\n{body}"
            );
        }
    }

    #[test]
    fn label_values_escape_the_characters_the_format_reserves() {
        assert_eq!(escape_label_value("plain_stage"), "plain_stage");
        assert_eq!(escape_label_value("a\"b"), "a\\\"b");
        assert_eq!(escape_label_value("a\\b"), "a\\\\b");
        assert_eq!(escape_label_value("a\nb"), "a\\nb");
    }

    /// A refusal leaves nothing in the store, so the only place a scraper can
    /// learn the rejection rate §12.1 asks for is this series. It is fed by the
    /// process registry, and the test drives the registry directly because that
    /// is exactly the seam: the write paths record, the endpoint renders.
    #[tokio::test]
    async fn metrics_expose_the_gate_outcome_counters_by_outcome_and_kind() {
        let counters = vestige_core::storage::GateOutcomeCounters::global();
        // A kind of its own, so the assertion is about the label pair the
        // renderer produces rather than about a total other tests move.
        counters.record_rejected("test_only_rule_rejected");
        counters.record_flagged("test_only_rule_flagged");

        let dir = TempDir::new().unwrap();
        let storage = test_storage(&dir.path().join("gate-metrics.db"));
        let response = get(storage, "/metrics").await;

        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        // Matched as whole sample lines, so the assertion is about the exact
        // `{outcome,kind}` label pair and not about the two words appearing
        // somewhere in the exposition. `metrics_are_prometheus_text_with_known_series`
        // is the test that the body parses; this one is that the pair is there.
        for line in [
            "vestige_gate_outcomes_total{outcome=\"rejected\",kind=\"test_only_rule_rejected\"} 1",
            "vestige_gate_outcomes_total{outcome=\"flagged\",kind=\"test_only_rule_flagged\"} 1",
        ] {
            assert!(
                body.lines().any(|l| l == line),
                "sample {line:?} missing from:\n{body}"
            );
        }
        assert!(
            body.contains("# TYPE vestige_gate_outcomes_total counter"),
            "the family has to declare its type:\n{body}"
        );
        assert!(
            body.contains("# HELP vestige_gate_outcomes_total") && body.contains("process"),
            "the HELP text must say the numbers are process-lifetime, not a property of the \
             store:\n{body}"
        );
    }
}
