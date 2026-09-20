//! The four numbers wave 5 measures, and the traps each one has to avoid.
//!
//! `docs/SELF-CONTAINED-MEMORY-DESIGN.md` §12 defines acceptance as four
//! measures — self-containment, anchor resolvability, use, and the rejection /
//! flag breakdown — because "it feels better" is not a result and a benchmark
//! score is not the goal (§7, §12.3). This module produces exactly those four
//! from the store, in one read, with the window and the basis of every number
//! stated in the payload.
//!
//! Three traps shape the code, and each one is why a field is split rather than
//! merged:
//!
//! - `self_contained = NULL` means **the gate never ran**, not "clean". The
//!   three categories are counted separately, because folding `NULL` into
//!   `clean` would let a store that was never checked look perfect.
//! - `unchecked` anchors mean **the environment could not check** (no
//!   repository, no revision), which is not the same failure as `orphaned`
//!   (the file is gone). Merging them makes rot look better or worse depending
//!   on where the process happens to run.
//! - Read counts are a poor retention signal and a fair value signal (§12.1),
//!   so `use` is reported for judgement and is deliberately **not** wired to
//!   decay. Two numbers are given because "retrieved" and "touched" are
//!   different claims: a promotion is not a retrieval.
//!
//! Rejections are the one measure the store cannot answer: a refused write
//! leaves nothing behind, by design. Those come from process-lifetime counters
//! ([`GateOutcomeCounters`]) and are labelled as such, so a report never lets a
//! number from this process masquerade as a property of the store.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Storage;
use crate::storage::Result;

/// Which clock the window is applied to.
///
/// `recorded_at` rather than `created_at`: the question is "what did we write
/// down in this period", and a backfilled row's `created_at` may predate the
/// moment we learned it (§1).
pub const QUALITY_WINDOW_BASIS: &str = "recorded_at";

/// How much of a store's memories are readable without their conversation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainmentCounts {
    /// The gate ran and found nothing: `self_contained = 1`.
    pub clean: i64,
    /// The gate ran and flagged the memory: `self_contained = 0`.
    pub flagged: i64,
    /// The gate never ran on this memory: `self_contained IS NULL`.
    ///
    /// Not a synonym for clean, and never added to it. A window full of
    /// `unchecked` means the write paths that produced it skipped the gate.
    pub unchecked: i64,
    /// Every memory in the window.
    pub total: i64,
}

impl ContainmentCounts {
    /// Share of windowed memories the gate checked *and* passed.
    ///
    /// `None` when the window is empty: a rate over nothing is not zero
    /// percent, it is no measurement, and a report that prints `0.0%` for an
    /// empty store invites the wrong conclusion.
    #[must_use]
    pub fn self_containment_rate(&self) -> Option<f64> {
        let checked = self.clean + self.flagged;
        (checked > 0).then(|| self.clean as f64 / checked as f64)
    }
}

/// Where the stored anchors stand, as the store last observed them.
///
/// The stored verdicts, not a fresh audit: an audit is the answer to "is this
/// still true" and costs a repository walk (wave 3's rot audit, which
/// consolidation runs in batches). This is the answer to "what does the store
/// say", which is what a later reader inherits — and the acceptance criterion
/// "share of anchors still fresh after 30 days" is read as: let consolidation
/// audit, then measure the stored distribution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorCounts {
    /// Content hash still matches the recorded revision.
    pub fresh: i64,
    /// The path exists but its content moved on.
    pub stale: i64,
    /// The path or symbol is gone.
    pub orphaned: i64,
    /// Nobody could check: no repository, or no revision recorded.
    pub unchecked: i64,
    /// Every anchor in the window.
    pub total: i64,
}

impl AnchorCounts {
    /// Share of anchors whose claim still holds, out of the checkable ones.
    ///
    /// `unchecked` is excluded from the denominator and reported separately:
    /// an environment with no repository would otherwise make resolvability
    /// look like zero, punishing the store for where it was read.
    #[must_use]
    pub fn resolvability_rate(&self) -> Option<f64> {
        let checkable = self.fresh + self.stale + self.orphaned;
        (checkable > 0).then(|| self.fresh as f64 / checkable as f64)
    }
}

/// Whether the memories in the window were ever used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UseCounts {
    /// Retrieved at least once after being written, from the access log's
    /// `search_hit` rows — a retrieval, not a lifecycle event.
    pub retrieved_at_least_once: i64,
    /// Touched at least once by anything that records an access.
    ///
    /// Two sources because the write paths differ: the access log carries typed
    /// events, while an explicit `memory get` records only the state counter.
    pub ever_accessed: i64,
    /// Every memory in the window.
    pub total: i64,
}

impl UseCounts {
    /// Share retrieved at least once. `None` for an empty window.
    #[must_use]
    pub fn retrieval_rate(&self) -> Option<f64> {
        (self.total > 0).then(|| self.retrieved_at_least_once as f64 / self.total as f64)
    }
}

/// One rule's share of the flags (or refusals) in the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCount {
    /// The rule identifier the gate reports (`no_subject`, `derivable_from_repo`…).
    pub kind: String,
    pub count: i64,
}

/// Process-lifetime gate outcomes, which the store cannot know.
///
/// A refused write stores nothing — that is the point of the reject path — so
/// the only witness is the process that refused it. These numbers therefore
/// describe *this process*, not the store, and the field names say so.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessGateCounters {
    /// Writes refused, by rule.
    pub rejected_by_kind: Vec<RuleCount>,
    /// Writes written and flagged, by rule.
    pub flagged_by_kind: Vec<RuleCount>,
    /// Refusals seen by this process.
    pub rejected: i64,
    /// Flags written by this process.
    pub flagged: i64,
}

/// The three rates, computed once so no consumer divides differently.
///
/// They travel with the counts instead of being left to the reader, because the
/// denominator is the whole argument: whether `unchecked` memories and
/// unverifiable anchors belong in it is exactly the mistake §12.1 warns about,
/// and a consumer that recomputes can only get it wrong. `null` means the
/// denominator was zero — no measurement, not zero percent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityRates {
    /// `clean / (clean + flagged)`, or `null` when the gate checked nothing.
    pub self_containment: Option<f64>,
    /// `fresh / (fresh + stale + orphaned)`, or `null` when nothing was checkable.
    pub anchor_resolvability: Option<f64>,
    /// `retrievedAtLeastOnce / total`, or `null` for an empty window.
    pub retrieval: Option<f64>,
}

/// The wave-5 measurement, ready to serialize into a report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryQualityReport {
    /// Start of the window, or `None` for "everything ever written".
    pub since: Option<String>,
    /// End of the window.
    pub until: String,
    /// The clock the window was applied to.
    pub window_basis: String,
    pub containment: ContainmentCounts,
    /// Which rules flagged the windowed memories.
    pub flagged_by_kind: Vec<RuleCount>,
    pub anchors: AnchorCounts,
    pub use_counts: UseCounts,
    /// Gate outcomes from this process; see [`ProcessGateCounters`].
    pub process: ProcessGateCounters,
    /// Derived shares, with `null` where there was nothing to divide by.
    pub rates: QualityRates,
    /// The traps this report avoids, stated where a reader of the JSON sees them.
    pub notes: Vec<String>,
}

impl Storage {
    /// Build the wave-5 measurement for `since..=now`.
    ///
    /// One reader lock for the whole report: the four measures must describe
    /// the same instant, or a write landing between them makes the numbers
    /// disagree with each other and the report becomes unauditable.
    pub fn memory_quality(&self, since: Option<DateTime<Utc>>) -> Result<MemoryQualityReport> {
        let until = Utc::now();
        let since_str = since.map(|at| at.to_rfc3339());
        let reader = self.acquire_reader()?;

        // `recorded_at` is set on every row (V17) and backfilled to
        // `created_at`, so the bound is a straight comparison with a COALESCE
        // for the pathological row rather than a join.
        let in_window = "COALESCE(recorded_at, created_at) >= COALESCE(?1, '') \
                         AND COALESCE(recorded_at, created_at) <= ?2";

        let mut containment = ContainmentCounts::default();
        {
            let mut stmt = reader.prepare(&format!(
                "SELECT self_contained, COUNT(*) FROM knowledge_nodes
                 WHERE {in_window} GROUP BY self_contained"
            ))?;
            let rows = stmt.query_map(params![since_str, until.to_rfc3339()], |row| {
                Ok((row.get::<_, Option<bool>>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (marker, count) = row?;
                containment.total += count;
                match marker {
                    Some(true) => containment.clean += count,
                    Some(false) => containment.flagged += count,
                    None => containment.unchecked += count,
                }
            }
        }

        let flagged_by_kind = {
            let mut stmt = reader.prepare(&format!(
                "SELECT self_contained_findings FROM knowledge_nodes
                 WHERE self_contained = 0 AND {in_window}"
            ))?;
            let rows = stmt.query_map(params![since_str, until.to_rfc3339()], |row| {
                row.get::<_, Option<String>>(0)
            })?;
            let mut tally: BTreeMap<String, i64> = BTreeMap::new();
            for row in rows {
                // A malformed findings blob is skipped rather than failing the
                // report: this is a measurement, and one bad row must not make
                // the whole store unmeasurable.
                for kind in finding_kinds(row?.as_deref()) {
                    *tally.entry(kind).or_default() += 1;
                }
            }
            tally
                .into_iter()
                .map(|(kind, count)| RuleCount { kind, count })
                .collect()
        };

        let mut anchors = AnchorCounts::default();
        {
            let mut stmt = reader.prepare(
                "SELECT cr.verdict, COUNT(*) FROM code_refs cr
                 JOIN knowledge_nodes n ON n.id = cr.node_id
                 WHERE COALESCE(n.recorded_at, n.created_at) >= COALESCE(?1, '')
                   AND COALESCE(n.recorded_at, n.created_at) <= ?2
                 GROUP BY cr.verdict",
            )?;
            let rows = stmt.query_map(params![since_str, until.to_rfc3339()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (verdict, count) = row?;
                anchors.total += count;
                match verdict.as_str() {
                    "fresh" => anchors.fresh += count,
                    "stale" => anchors.stale += count,
                    "orphaned" => anchors.orphaned += count,
                    // Anything else — including `unchecked` — is an anchor whose
                    // claim nothing has confirmed, which is what unchecked means.
                    _ => anchors.unchecked += count,
                }
            }
        }

        let use_counts = {
            let mut stmt = reader.prepare(&format!(
                "SELECT
                    COUNT(*) AS total,
                    SUM(CASE WHEN EXISTS (
                        SELECT 1 FROM memory_access_log l
                        WHERE l.node_id = n.id
                          AND l.access_type = 'search_hit'
                          AND l.accessed_at >= COALESCE(n.recorded_at, n.created_at)
                    ) THEN 1 ELSE 0 END) AS retrieved,
                    SUM(CASE WHEN EXISTS (
                        SELECT 1 FROM memory_access_log l
                        WHERE l.node_id = n.id
                    ) OR EXISTS (
                        SELECT 1 FROM memory_states s
                        WHERE s.memory_id = n.id AND s.access_count > 0
                    ) THEN 1 ELSE 0 END) AS touched
                 FROM knowledge_nodes n WHERE {in_window}"
            ))?;
            stmt.query_row(params![since_str, until.to_rfc3339()], |row| {
                Ok(UseCounts {
                    total: row.get(0)?,
                    retrieved_at_least_once: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    ever_accessed: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                })
            })?
        };

        Ok(MemoryQualityReport {
            since: since_str,
            until: until.to_rfc3339(),
            window_basis: QUALITY_WINDOW_BASIS.to_string(),
            containment,
            flagged_by_kind,
            anchors,
            rates: QualityRates {
                self_containment: containment.self_containment_rate(),
                anchor_resolvability: anchors.resolvability_rate(),
                retrieval: use_counts.retrieval_rate(),
            },
            use_counts,
            process: GateOutcomeCounters::global().snapshot(),
            notes: vec![
                "`self_contained: null` means the gate never ran on that memory; it is counted as \
                 `unchecked` and never as `clean`."
                    .to_string(),
                "`anchors.unchecked` means no repository or revision was available to check; it is \
                 reported beside resolvability, not inside it."
                    .to_string(),
                "`useCounts` describe value, not retention: nothing in the store decays because of \
                 them."
                    .to_string(),
                "`process.*` counts come from this process's lifetime, not from the window: a \
                 refused write stores nothing, so the store cannot remember it."
                    .to_string(),
            ],
        })
    }
}

/// The rule kinds inside one stored `self_contained_findings` blob.
fn finding_kinds(blob: Option<&str>) -> Vec<String> {
    let Some(blob) = blob else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(blob) else {
        return Vec::new();
    };
    value
        .as_array()
        .map(|findings| {
            findings
                .iter()
                .filter_map(|finding| {
                    finding
                        .get("kind")
                        .and_then(|kind| kind.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Process-lifetime gate counters, as a process-wide registry.
///
/// One registry rather than a field threaded through every write path: the
/// reject path returns before storage is reached, so the counter has to be
/// reachable from where the verdict is made. Reset only by a restart, which is
/// why the report calls them process-scoped.
#[derive(Debug, Default)]
pub struct GateOutcomeCounters {
    rejected: std::sync::atomic::AtomicI64,
    flagged: std::sync::atomic::AtomicI64,
    rejected_by_kind: std::sync::Mutex<BTreeMap<String, i64>>,
    flagged_by_kind: std::sync::Mutex<BTreeMap<String, i64>>,
}

impl GateOutcomeCounters {
    /// The process-wide registry.
    #[must_use]
    pub fn global() -> &'static Self {
        static COUNTERS: std::sync::OnceLock<GateOutcomeCounters> = std::sync::OnceLock::new();
        COUNTERS.get_or_init(GateOutcomeCounters::default)
    }

    /// Record a refusal, by rule.
    pub fn record_rejected(&self, kind: &str) {
        use std::sync::atomic::Ordering;
        self.rejected.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut tally) = self.rejected_by_kind.lock() {
            *tally.entry(kind.to_string()).or_default() += 1;
        }
    }

    /// Record one rule that flagged a memory that was written anyway.
    pub fn record_flagged(&self, kind: &str) {
        use std::sync::atomic::Ordering;
        self.flagged.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut tally) = self.flagged_by_kind.lock() {
            *tally.entry(kind.to_string()).or_default() += 1;
        }
    }

    /// A snapshot for a report.
    #[must_use]
    pub fn snapshot(&self) -> ProcessGateCounters {
        use std::sync::atomic::Ordering;
        let tally = |cell: &std::sync::Mutex<BTreeMap<String, i64>>| {
            cell.lock()
                .map(|tally| {
                    tally
                        .iter()
                        .map(|(kind, count)| RuleCount {
                            kind: kind.clone(),
                            count: *count,
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        ProcessGateCounters {
            rejected: self.rejected.load(Ordering::Relaxed),
            flagged: self.flagged.load(Ordering::Relaxed),
            rejected_by_kind: tally(&self.rejected_by_kind),
            flagged_by_kind: tally(&self.flagged_by_kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::IngestInput;
    use tempfile::tempdir;

    fn storage() -> Storage {
        let dir = tempdir().unwrap();
        Storage::new(Some(dir.path().join("test.db"))).unwrap()
    }

    fn ingest_with(storage: &Storage, content: &str, marker: Option<bool>) -> String {
        storage
            .ingest(IngestInput {
                content: content.to_string(),
                node_type: "fact".to_string(),
                self_contained: marker,
                ..Default::default()
            })
            .unwrap()
            .id
    }

    fn set_recorded_at(storage: &Storage, id: &str, at: &str) {
        let writer = storage.writer.lock().unwrap();
        writer
            .execute(
                "UPDATE knowledge_nodes SET recorded_at = ?1, created_at = ?1 WHERE id = ?2",
                params![at, id],
            )
            .unwrap();
    }

    fn set_verdict(storage: &Storage, id: &str, verdict: &str) {
        let writer = storage.writer.lock().unwrap();
        writer
            .execute(
                "UPDATE code_refs SET verdict = ?1 WHERE node_id = ?2",
                params![verdict, id],
            )
            .unwrap();
    }

    fn anchor(path: &str) -> crate::code_refs::IngestAnchor {
        crate::code_refs::IngestAnchor::unchecked(crate::code_refs::CodeAnchor {
            repo_remote: None,
            commit_sha: None,
            path: path.to_string(),
            symbol: None,
            hint_line: None,
            content_hash: None,
        })
    }

    /// The measure that keeps a store honest: a memory the gate never saw is
    /// not a memory the gate passed. Folding `NULL` into `clean` would let a
    /// store written entirely before the gate existed report 100%.
    #[test]
    fn unchecked_memories_are_counted_separately_and_stay_out_of_the_rate() {
        let storage = storage();
        ingest_with(&storage, "clean one", Some(true));
        let flagged = ingest_with(&storage, "flagged one", Some(false));
        ingest_with(&storage, "never checked", None);

        let writer = storage.writer.lock().unwrap();
        writer
            .execute(
                "UPDATE knowledge_nodes SET self_contained_findings = ?1 WHERE id = ?2",
                params![
                    r#"[{"kind":"no_subject","span":"to","hint":"say what this is about"},
                        {"kind":"bare_code_reference","span":"src/lib.rs:12","hint":"anchor it"}]"#,
                    flagged
                ],
            )
            .unwrap();
        drop(writer);

        let report = storage.memory_quality(None).unwrap();
        assert_eq!(report.containment.clean, 1);
        assert_eq!(report.containment.flagged, 1);
        assert_eq!(report.containment.unchecked, 1);
        assert_eq!(report.containment.total, 3);
        assert_eq!(
            report.containment.self_containment_rate(),
            Some(0.5),
            "the rate is over what the gate checked, not over the store"
        );
        assert_eq!(
            report.rates.self_containment,
            Some(0.5),
            "the payload must carry the rate, not only the counts: a consumer that \
             divides differently is the trap this field removes"
        );
        assert_eq!(report.window_basis, "recorded_at");

        let kinds: Vec<&str> = report
            .flagged_by_kind
            .iter()
            .map(|k| k.kind.as_str())
            .collect();
        assert_eq!(kinds, vec!["bare_code_reference", "no_subject"]);
        assert!(report.flagged_by_kind.iter().all(|k| k.count == 1));
    }

    /// `unchecked` anchors are an environment failure, `orphaned` a code one.
    /// Merging them would make resolvability depend on where the process runs.
    #[test]
    fn anchor_resolvability_excludes_the_anchors_nobody_could_check() {
        let storage = storage();
        let fresh = storage
            .ingest(IngestInput {
                content: "a fresh anchor".to_string(),
                node_type: "fact".to_string(),
                anchors: vec![anchor("src/fresh.rs")],
                ..Default::default()
            })
            .unwrap()
            .id;
        let orphaned = storage
            .ingest(IngestInput {
                content: "an orphaned anchor".to_string(),
                node_type: "fact".to_string(),
                anchors: vec![anchor("src/gone.rs")],
                ..Default::default()
            })
            .unwrap()
            .id;
        storage
            .ingest(IngestInput {
                content: "an anchor nobody could check".to_string(),
                node_type: "fact".to_string(),
                anchors: vec![anchor("src/unchecked.rs")],
                ..Default::default()
            })
            .unwrap();

        set_verdict(&storage, &fresh, "fresh");
        set_verdict(&storage, &orphaned, "orphaned");

        let report = storage.memory_quality(None).unwrap();
        assert_eq!(report.anchors.fresh, 1);
        assert_eq!(report.anchors.orphaned, 1);
        assert_eq!(report.anchors.unchecked, 1);
        assert_eq!(report.anchors.stale, 0);
        assert_eq!(report.anchors.total, 3);
        assert_eq!(
            report.anchors.resolvability_rate(),
            Some(0.5),
            "one of the two checkable anchors still holds"
        );
        assert_eq!(report.rates.anchor_resolvability, Some(0.5));
    }

    /// "Retrieved" and "touched" are different claims, and a lifecycle event is
    /// not a retrieval: a promotion records an access but says nothing about
    /// whether anyone read the memory.
    #[test]
    fn use_separates_retrieval_from_any_other_access() {
        let storage = storage();
        let retrieved = ingest_with(&storage, "was searched", Some(true));
        let promoted_only = ingest_with(&storage, "was only promoted", Some(true));
        ingest_with(&storage, "untouched", Some(true));

        storage.log_access(&retrieved, "search_hit").unwrap();
        storage.log_access(&promoted_only, "promote").unwrap();

        let report = storage.memory_quality(None).unwrap();
        assert_eq!(report.use_counts.total, 3);
        assert_eq!(report.use_counts.retrieved_at_least_once, 1);
        assert_eq!(
            report.use_counts.ever_accessed, 2,
            "a retrieval is also an access; the reverse is what the two numbers separate"
        );
        assert!(
            report.use_counts.retrieved_at_least_once <= report.use_counts.ever_accessed,
            "retrieval is a subset of access by construction"
        );
        let rate = report.use_counts.retrieval_rate().unwrap();
        assert!((rate - 1.0 / 3.0).abs() < 1e-9, "got {rate}");
        assert_eq!(report.rates.retrieval, Some(rate));
    }

    /// The window is the whole point of a before/after comparison, so it is
    /// pinned: an old memory must not appear in a report about this hour.
    #[test]
    fn the_window_is_applied_to_the_record_time() {
        let storage = storage();
        let recent = ingest_with(&storage, "written now", Some(true));
        let old = ingest_with(&storage, "written long ago", Some(false));
        set_recorded_at(&storage, &old, "2020-01-01T00:00:00+00:00");
        // Touched *before* it was recorded: the retrieval count is bounded by
        // the record time, so a backfilled row cannot inherit older reads.
        storage.log_access(&recent, "search_hit").unwrap();

        let since = Utc::now() - chrono::Duration::hours(1);
        let report = storage.memory_quality(Some(since)).unwrap();
        assert_eq!(report.containment.total, 1);
        assert_eq!(report.containment.clean, 1);
        assert_eq!(report.anchors.total, 0);
        assert_eq!(report.use_counts.retrieved_at_least_once, 1);

        let everything = storage.memory_quality(None).unwrap();
        assert_eq!(everything.containment.total, 2);
    }

    /// A rate over an empty window is not zero percent, it is no measurement —
    /// and a report that answers `0.0` for an empty store invites the wrong
    /// conclusion about a store nobody has written to yet.
    #[test]
    fn an_empty_window_reports_no_rate_rather_than_zero() {
        let storage = storage();
        ingest_with(&storage, "written now", Some(true));

        let future = Utc::now() + chrono::Duration::hours(1);
        let report = storage.memory_quality(Some(future)).unwrap();

        assert_eq!(report.containment.total, 0);
        assert_eq!(report.containment.self_containment_rate(), None);
        assert_eq!(report.anchors.resolvability_rate(), None);
        assert_eq!(report.use_counts.retrieval_rate(), None);
        // `null`, not `0.0`, and the payload says the same thing the methods do.
        assert_eq!(report.rates, QualityRates::default());
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["rates"]["selfContainment"], serde_json::Value::Null);
        assert_eq!(
            json["rates"]["anchorResolvability"],
            serde_json::Value::Null
        );
        assert_eq!(json["rates"]["retrieval"], serde_json::Value::Null);
        assert!(
            !report.notes.is_empty(),
            "the report has to carry the traps it avoids"
        );
    }

    /// The counters the store cannot keep — a refused write stores nothing —
    /// are pinned here, including that one refusal with two findings counts as
    /// one refusal and two flags.
    #[test]
    fn process_counters_tally_refusals_and_flags_by_rule() {
        let counters = GateOutcomeCounters::default();
        counters.record_rejected("derivable_from_repo");
        counters.record_rejected("derivable_from_repo");
        counters.record_flagged("no_subject");
        counters.record_flagged("bare_code_reference");

        let snapshot = counters.snapshot();
        assert_eq!(snapshot.rejected, 2);
        assert_eq!(snapshot.flagged, 2);
        assert_eq!(
            snapshot.rejected_by_kind,
            vec![RuleCount {
                kind: "derivable_from_repo".to_string(),
                count: 2
            }]
        );
        assert_eq!(snapshot.flagged_by_kind.len(), 2);
    }

    /// The report reads the process registry rather than a copy taken at
    /// startup, so a counter recorded just before the report shows up in it.
    #[test]
    fn the_report_carries_the_process_counters_it_can_see() {
        let storage = storage();
        GateOutcomeCounters::global().record_rejected("derivable_from_repo");

        let report = storage.memory_quality(None).unwrap();
        assert!(
            report
                .process
                .rejected_by_kind
                .iter()
                .any(|k| k.kind == "derivable_from_repo" && k.count >= 1),
            "expected the refusal just recorded, got {:?}",
            report.process
        );
    }
}
