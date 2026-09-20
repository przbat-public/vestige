//! Record time and content history — `knowledge_nodes.recorded_at` (V17) and
//! the `memory_revisions` audit trail.
//!
//! These tests pin the two properties the design depends on and that nothing
//! else in the suite would notice breaking:
//!
//! 1. A memory carries *when we wrote it down*, and that instant never moves —
//!    not when it is searched, strengthened, decayed or consolidated.
//! 2. Every operation that changes what a memory says leaves an append-only
//!    row showing what it said before, in the same transaction as the change.

use chrono::{Duration, Utc};
use tempfile::tempdir;

use crate::memory::IngestInput;
use crate::storage::RevisionKind;

use super::Storage;

fn create_test_storage() -> Storage {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    Storage::new(Some(db_path)).unwrap()
}

fn ingest(storage: &Storage, content: &str) -> crate::memory::KnowledgeNode {
    storage
        .ingest(IngestInput {
            content: content.to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap()
}

/// A record time must exist on every read, and equal `created_at` at ingest —
/// the same value the V17 backfill gives a pre-existing row, so a memory
/// written before and after the migration is indistinguishable to a reader.
#[test]
fn ingest_sets_recorded_at_to_created_at() {
    let storage = create_test_storage();
    let node = ingest(&storage, "record time is set at ingest");

    assert_eq!(
        node.recorded_at, node.created_at,
        "a freshly recorded memory must have recorded_at == created_at"
    );
    assert!(
        node.recorded_at > Utc::now() - Duration::minutes(5),
        "recorded_at must be a real timestamp, not the 1970 default: {}",
        node.recorded_at
    );

    // Every read path funnels through `row_to_node`, but a caller reading the
    // node back (rather than the ingest return value) is the case that would
    // silently see a default if the mapper ever stopped selecting the column.
    let read_back = storage.get_node(&node.id).unwrap().unwrap();
    assert_eq!(read_back.recorded_at, node.recorded_at);
    let all = storage.get_all_nodes(10, 0).unwrap();
    assert_eq!(all[0].recorded_at, node.recorded_at);

    // The stored column, not just the mapped field: a mapper that filled
    // `recorded_at` from `created_at` (the conflation this column exists to
    // end) would satisfy every assertion above while the database held no
    // record time at all.
    assert_eq!(
        storage.raw_recorded_at(&node.id).unwrap().as_deref(),
        Some(node.created_at.to_rfc3339().as_str()),
        "the record time must be persisted in its own column, not derived on read"
    );
}

/// The immutability contract from the design: after searches that refresh
/// `last_accessed`, after strengthening, and after a consolidation pass,
/// `recorded_at` is byte-identical. A record time that drifts with use cannot
/// support any claim about age, which is the whole reason the column exists.
#[test]
fn recorded_at_survives_searches_strengthening_and_consolidation() {
    let storage = create_test_storage();
    let node = ingest(&storage, "the deploy window is Thursday 09:00 UTC");

    // Overwrite with a recognisable instant. `created_at` is what the backfill
    // copies, so a writer that bumped the wrong column would otherwise be
    // invisible to this test.
    let pinned = "2026-01-02T03:04:05+00:00";
    storage.set_recorded_at_for_test(&node.id, pinned).unwrap();

    for _ in 0..20 {
        storage.search("deploy window", 5).unwrap();
        storage.strengthen_on_access(&node.id).unwrap();
    }
    storage
        .mark_reviewed(&node.id, crate::fsrs::Rating::Good)
        .unwrap();
    storage.apply_decay().unwrap();
    let _ = storage.run_consolidation();

    assert_eq!(
        storage.raw_recorded_at(&node.id).unwrap().as_deref(),
        Some(pinned),
        "recorded_at moved — a search, a strengthening pass or consolidation \
         rewrote the record time"
    );
    assert_eq!(
        storage.get_node(&node.id).unwrap().unwrap().recorded_at,
        chrono::DateTime::parse_from_rfc3339(pinned)
            .unwrap()
            .with_timezone(&Utc),
        "the stored column changed even though the raw read did not"
    );
}

/// Ingest is the genesis of the timeline: one `create` revision carrying the
/// text, written with the row it describes.
#[test]
fn ingest_records_a_create_revision() {
    let storage = create_test_storage();
    let node = ingest(&storage, "first wording of the fact");

    let revisions = storage.get_memory_revisions(&node.id, 10).unwrap();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0].kind, RevisionKind::Create);
    assert_eq!(
        revisions[0].new_content.as_deref(),
        Some("first wording of the fact")
    );
    assert_eq!(
        revisions[0].old_content, None,
        "a creation has no previous wording"
    );
    assert!(
        revisions[0].recorded_at >= node.recorded_at - Duration::seconds(1),
        "the revision must be stamped at the same moment as the node"
    );
}

/// The core of "how the picture changed": an edit must preserve the text it
/// replaced. Before this, `update_node_content` overwrote the memory and the
/// previous wording was unrecoverable from the database.
#[test]
fn edit_produces_exactly_one_revision_with_the_previous_content() {
    let storage = create_test_storage();
    let node = ingest(&storage, "the meeting is on Tuesday");
    let recorded_before = storage.raw_recorded_at(&node.id).unwrap();

    storage
        .update_node_content(&node.id, "the meeting is on Wednesday")
        .unwrap();

    let revisions = storage.get_memory_revisions(&node.id, 10).unwrap();
    assert_eq!(
        storage.count_revisions(&node.id).unwrap(),
        2,
        "one `create` plus one `edit`, and nothing else"
    );

    let edit = revisions
        .iter()
        .find(|r| r.kind == RevisionKind::Edit)
        .expect("the edit must have produced an `edit` revision");
    assert_eq!(
        edit.old_content.as_deref(),
        Some("the meeting is on Tuesday"),
        "the revision must carry the text the edit replaced"
    );
    assert_eq!(
        edit.new_content.as_deref(),
        Some("the meeting is on Wednesday")
    );

    // An edit changes the wording, not when we learned the memory. If this ever
    // moves, an edited memory reads as newly recorded and the timeline
    // collapses.
    assert_eq!(
        storage.raw_recorded_at(&node.id).unwrap(),
        recorded_before,
        "an edit must not move recorded_at"
    );
}

/// A second edit must not overwrite the first one's "before" — the table is
/// append-only, so three edits leave a readable chain rather than one row.
#[test]
fn successive_edits_append_their_history() {
    let storage = create_test_storage();
    let node = ingest(&storage, "v1");

    storage.update_node_content(&node.id, "v2").unwrap();
    storage.update_node_content(&node.id, "v3").unwrap();

    let revisions = storage.get_memory_revisions(&node.id, 10).unwrap();
    assert_eq!(revisions.len(), 3);
    let edits: Vec<(&str, &str)> = revisions
        .iter()
        .filter(|r| r.kind == RevisionKind::Edit)
        .map(|r| {
            (
                r.old_content.as_deref().unwrap(),
                r.new_content.as_deref().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        edits,
        vec![("v2", "v3"), ("v1", "v2")],
        "each edit must remember its own predecessor (newest first)"
    );
}

/// Invalidation changes a column, not the text, so the revision has to record
/// the *transition*: the previous `valid_until` alongside the new one. Without
/// the old value, a re-invalidation is indistinguishable from a first one and
/// "what did we believe" is still unanswerable for expired facts.
#[test]
fn invalidation_records_the_previous_and_new_validity() {
    let storage = create_test_storage();
    let node = ingest(&storage, "the API rate limit is 100 rpm");

    // First invalidation: the window was unbounded before.
    let when = Utc::now();
    assert!(
        storage
            .invalidate_with_revision(&node.id, when, Some("superseded by the v2 API"))
            .unwrap()
    );

    // Second invalidation re-anchors the same memory, so the revision must show
    // the window it is replacing rather than starting from nothing.
    let later = when + Duration::days(3);
    assert!(
        storage.set_valid_until(&node.id, later).unwrap(),
        "set_valid_until keeps working through the revision path"
    );

    let revisions = storage.get_memory_revisions(&node.id, 10).unwrap();
    let invalidations: Vec<_> = revisions
        .iter()
        .filter(|r| r.kind == RevisionKind::Invalidate)
        .collect();
    assert_eq!(invalidations.len(), 2);

    // Newest first.
    assert_eq!(
        invalidations[0].new_content.as_deref(),
        Some(later.to_rfc3339().as_str())
    );
    assert_eq!(
        invalidations[0].old_content.as_deref(),
        Some(when.to_rfc3339().as_str()),
        "the second invalidation must record the window it replaced"
    );
    assert_eq!(
        invalidations[1].old_content, None,
        "the first invalidation replaced an unbounded window"
    );
    assert_eq!(
        invalidations[1].reason.as_deref(),
        Some("superseded by the v2 API"),
        "the caller's reason must survive into the history"
    );
}

/// An invalidation that matches nothing must not append history: a revision for
/// a change that never happened makes the timeline lie.
#[test]
fn invalidating_an_unknown_memory_records_nothing() {
    let storage = create_test_storage();
    assert!(
        !storage
            .set_valid_until("no-such-memory", Utc::now())
            .unwrap()
    );
    assert!(
        storage
            .get_memory_revisions("no-such-memory", 10)
            .unwrap()
            .is_empty()
    );
}

/// GDPR erasure must remove the history too. The revision table stores the
/// memory's text a second time, so an erasure that leaves it behind reports a
/// deletion that did not happen — the exact failure the design calls out.
#[test]
fn erasure_removes_revisions_and_their_content() {
    let storage = create_test_storage();
    let secret = "subject-42 was diagnosed in confidence";
    let node = ingest(&storage, secret);
    storage
        .update_node_content(&node.id, "subject-42 moved to a different clinic")
        .unwrap();
    assert_eq!(storage.count_revisions(&node.id).unwrap(), 2);

    storage.right_to_erasure(&node.id).unwrap();

    assert!(storage.get_node(&node.id).unwrap().is_none());
    assert_eq!(
        storage.count_revisions(&node.id).unwrap(),
        0,
        "the content history survived the erasure"
    );
    assert!(
        storage
            .get_memory_revisions(&node.id, 10)
            .unwrap()
            .is_empty()
    );

    // Not just "no rows for this id": no row anywhere may still hold the text,
    // which is what a partial erasure (orphaned revisions left by a failed
    // delete) would look like.
    assert_eq!(
        storage.count_revisions_containing(secret).unwrap(),
        0,
        "the erased text is still readable in memory_revisions"
    );
    assert!(
        storage.search("diagnosed", 10).unwrap().is_empty(),
        "the erased memory is still reachable through search"
    );
}

/// `erase_by_tag` is the bulk path and shares `erasure_rows`, but it is the one
/// that runs in a loop — a per-id mistake there would leave every erased
/// subject's history behind. Pin it end to end.
#[test]
fn erase_by_tag_removes_revisions_of_every_matched_memory() {
    let storage = create_test_storage();
    let tagged = storage
        .ingest(IngestInput {
            content: "gdpr subject note one".to_string(),
            node_type: "fact".to_string(),
            tags: vec!["gdpr-subject".to_string()],
            ..Default::default()
        })
        .unwrap();
    let other = ingest(&storage, "unrelated memory that must survive");

    let (memories, _artifacts) = storage.erase_by_tag("gdpr-subject").unwrap();

    assert_eq!(memories, 1);
    assert!(storage.get_node(&tagged.id).unwrap().is_none());
    assert_eq!(storage.count_revisions(&tagged.id).unwrap(), 0);
    assert_eq!(
        storage.count_revisions(&other.id).unwrap(),
        1,
        "the unrelated memory's own `create` revision must survive"
    );
}

/// A plain `delete_node` is not erasure, but leaving the history behind would
/// keep the deleted text readable through the same table — so it removes it in
/// the same transaction.
#[test]
fn delete_node_removes_its_revisions_too() {
    let storage = create_test_storage();
    let node = ingest(&storage, "temporary note");
    storage
        .update_node_content(&node.id, "temporary note, revised")
        .unwrap();

    assert!(storage.delete_node(&node.id).unwrap());
    assert_eq!(
        storage.count_revisions(&node.id).unwrap(),
        0,
        "a deleted memory's history must not outlive it"
    );
}

/// The read API: newest first, and `limit` respected. The timeline is only
/// usable if the first row is the most recent event.
#[test]
fn get_memory_revisions_is_newest_first_and_limited() {
    let storage = create_test_storage();
    let node = ingest(&storage, "step 0");

    // Distinct stored timestamps, so the ordering under test is the SQL
    // ordering and not the id tiebreak.
    for step in 1..=3 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        storage
            .update_node_content_with_revision(&node.id, &format!("step {step}"), Some("test edit"))
            .unwrap();
    }

    let all = storage.get_memory_revisions(&node.id, 100).unwrap();
    assert_eq!(all.len(), 4, "create plus three edits");
    let order: Vec<&str> = all
        .iter()
        .map(|r| r.new_content.as_deref().unwrap())
        .collect();
    assert_eq!(
        order,
        vec!["step 3", "step 2", "step 1", "step 0"],
        "history must read newest first"
    );
    assert!(
        all.windows(2).all(|w| w[0].recorded_at >= w[1].recorded_at),
        "recorded_at must be non-increasing down the page"
    );
    assert_eq!(
        all[0].reason.as_deref(),
        Some("test edit"),
        "a supplied reason must be stored on the revision"
    );

    let limited = storage.get_memory_revisions(&node.id, 2).unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].new_content.as_deref(), Some("step 3"));
    assert_eq!(limited[1].new_content.as_deref(), Some("step 2"));

    // A limit of 0 is a request for "some history", not for an empty page that
    // would be indistinguishable from a memory with no history.
    assert_eq!(storage.get_memory_revisions(&node.id, 0).unwrap().len(), 1);

    // Another memory's history never leaks in.
    let other = ingest(&storage, "unrelated");
    let other_history = storage.get_memory_revisions(&other.id, 10).unwrap();
    assert_eq!(other_history.len(), 1);
    assert_eq!(other_history[0].new_content.as_deref(), Some("unrelated"));

    // ... and a memory with no history at all reads as empty, not as an error.
    assert!(
        storage
            .get_memory_revisions("no-such-memory", 10)
            .unwrap()
            .is_empty()
    );
}

/// `get_latest_revision` must agree with the head of `get_memory_revisions` —
/// two orderings that disagree would make the provenance line and the timeline
/// tell different stories.
#[test]
fn latest_revision_matches_the_head_of_the_history() {
    let storage = create_test_storage();
    let node = ingest(&storage, "one");
    std::thread::sleep(std::time::Duration::from_millis(5));
    storage.update_node_content(&node.id, "two").unwrap();

    let head = storage.get_memory_revisions(&node.id, 1).unwrap().remove(0);
    let latest = storage.get_latest_revision(&node.id).unwrap().unwrap();
    assert_eq!(head, latest);
    assert!(
        storage
            .get_latest_revision("no-such-memory")
            .unwrap()
            .is_none()
    );
}

/// The prediction-error gate's supersede decision is applied inside
/// `smart_ingest`, and it is the one write that changes a memory's standing
/// without touching its text. Pin that it leaves a revision naming what
/// replaced what and carrying the gate's reason — otherwise a superseded
/// memory keeps its old text with nothing anywhere saying it was retired.
#[test]
#[cfg(all(feature = "embeddings", feature = "vector-search"))]
fn supersede_records_a_revision_naming_the_replacement() {
    let storage = create_test_storage();
    if !storage.embedding_service_ready() {
        eprintln!("embedding service not ready — supersede path is unreachable");
        return;
    }

    let old = ingest(
        &storage,
        "The deployment window for the payments service is Thursday at 09:00 UTC.",
    );

    // The gate only supersedes a *demoted* memory at this similarity (a fresh
    // one would be reinforced), which is the documented trigger for the
    // `Improvement` reason.
    for _ in 0..6 {
        storage.demote_memory(&old.id).unwrap();
    }

    let result = storage
        .smart_ingest(IngestInput {
            content: "Payments deployment: Thursday 09:00 UTC, pending release-candidate sign-off."
                .to_string(),
            node_type: "fact".to_string(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        result.decision, "supersede",
        "test premise: this pair must reach the supersede branch (similarity {:?})",
        result.similarity
    );
    assert_eq!(result.superseded_id.as_deref(), Some(old.id.as_str()));

    let revisions = storage.get_memory_revisions(&old.id, 10).unwrap();
    let supersedes: Vec<_> = revisions
        .iter()
        .filter(|r| r.kind == RevisionKind::Supersede)
        .collect();
    assert_eq!(
        supersedes.len(),
        2,
        "retiring the memory and naming its successor are two recorded moments"
    );

    // Newest first: the successor row, then the retirement row.
    assert_eq!(
        supersedes[0].new_content.as_deref(),
        Some(result.node.id.as_str()),
        "the newest supersede revision must name the memory that replaced it"
    );
    assert_eq!(
        supersedes[0].reason.as_deref(),
        Some(result.reason.as_str()),
        "the gate's reason must be preserved verbatim"
    );
    assert_eq!(
        supersedes[1].old_content.as_deref(),
        Some("The deployment window for the payments service is Thursday at 09:00 UTC."),
        "the retirement revision must carry the text that was retired"
    );
    assert_eq!(
        supersedes[1].new_content.as_deref(),
        Some("unbounded"),
        "the memory had an open validity window before it was superseded"
    );
    assert!(supersedes[1].reason.is_some());

    // The old memory really was retired, not merely annotated.
    let retired = storage.get_node(&old.id).unwrap().unwrap();
    assert!(retired.valid_until.is_some());
    assert!(
        retired.retrieval_strength < 0.3,
        "a superseded memory must be demoted, got {}",
        retired.retrieval_strength
    );
}

// ============================================================================
// V18: the self-containedness marker
// ============================================================================

/// The marker has to survive the mapper, not only the INSERT: a reader that
/// cannot see it cannot act on it, and the whole reason to persist it is that
/// someone will look at this memory months later.
#[test]
fn the_self_contained_marker_round_trips_through_the_node_mapper() {
    let storage = create_test_storage();

    let flagged = storage
        .ingest(IngestInput {
            content: "BUG FIX: naprawiłem to, co omawialiśmy".to_string(),
            node_type: "fact".to_string(),
            self_contained: Some(false),
            self_contained_findings: Some(serde_json::json!([
                { "kind": "discourse_deixis", "span": "omawialiśmy", "hint": "nazwij, co było omawiane" }
            ])),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(flagged.self_contained, Some(false));
    let findings = flagged
        .self_contained_findings
        .as_ref()
        .and_then(|v| v.as_array())
        .expect("the findings must come back with the marker");
    assert_eq!(findings[0]["kind"], "discourse_deixis");
}

/// The distinction the column exists for: a row no gate has seen is not a row
/// that passed. Defaulting the unchecked case to `true` would let an audit
/// report a clean store it never checked.
#[test]
fn a_memory_no_gate_has_seen_is_unmarked_not_clean() {
    let storage = create_test_storage();
    let node = ingest(&storage, "A memory written by a path that runs no gate");

    assert_eq!(
        node.self_contained, None,
        "NULL means 'not checked'; claiming a pass would be a fabricated verdict"
    );
    assert_eq!(node.self_contained_findings, None);
}

/// Wave 1 recorded `actor` as NULL on every path. The edit path is the one a
/// tool drives, so it is the one where the identity has to reach the row.
#[test]
fn an_edit_records_the_actor_it_was_given() {
    let storage = create_test_storage();
    let node = ingest(&storage, "The deploy window is Thursday at 09:00 UTC");

    storage
        .update_node_content_with_revision_as(
            &node.id,
            "The deploy window is Friday at 09:00 UTC",
            Some("corrected after the freeze moved"),
            Some(r#"{"agent":"cursor","session_id":"session-42"}"#),
        )
        .unwrap();

    let revisions = storage.get_memory_revisions(&node.id, 10).unwrap();
    let edit = revisions
        .iter()
        .find(|r| r.kind == RevisionKind::Edit)
        .expect("the edit must be in the history");
    assert_eq!(
        edit.actor.as_deref(),
        Some(r#"{"agent":"cursor","session_id":"session-42"}"#),
        "the revision must carry the actor it was handed, verbatim"
    );

    // The old signature still works and still leaves the column NULL, because
    // the callers that use it have no identity to supply.
    storage
        .update_node_content(&node.id, "The deploy window is Saturday at 09:00 UTC")
        .unwrap();
    let revisions = storage.get_memory_revisions(&node.id, 10).unwrap();
    assert_eq!(
        revisions[0].actor, None,
        "a caller that knows no actor must not have one invented for it"
    );
}

/// Retraction is read from the revisions, not from `valid_until`, and "at or
/// before the instant" is what decides: a memory taken back before the instant
/// was no longer believed then, and one taken back after it still was.
///
/// This is the storage half of the as-of search filter. Reading `valid_until`
/// instead would call a write-time validity anchor ("this holds until Friday") a
/// retraction, and would miss that a memory superseded after the instant was the
/// current belief at it.
#[test]
fn retraction_is_dated_by_the_revision_not_by_the_validity_bound() {
    let storage = create_test_storage();
    let retracted_before = ingest(&storage, "the release channel is stable-3.4");
    let retracted_after = ingest(&storage, "the release channel is stable-3.5");
    let never_retracted = ingest(&storage, "the release channel is stable-3.6");

    let at = Utc::now();
    std::thread::sleep(std::time::Duration::from_millis(2));
    storage
        .invalidate_with_revision(
            &retracted_before.id,
            Utc::now(),
            Some("superseded by stable-3.5"),
        )
        .unwrap();

    let candidates: Vec<String> = vec![
        retracted_before.id.clone(),
        retracted_after.id.clone(),
        never_retracted.id.clone(),
    ];
    let before = storage
        .retracted_node_ids_at_or_before(at, &candidates)
        .unwrap();
    assert!(
        before.is_empty(),
        "nothing had been retracted at the instant, so nothing may be filtered: {before:?}"
    );

    std::thread::sleep(std::time::Duration::from_millis(2));
    let later = Utc::now();
    let after = storage
        .retracted_node_ids_at_or_before(later, &candidates)
        .unwrap();
    assert!(
        after.contains(&retracted_before.id),
        "the invalidate revision predates the later instant: {after:?}"
    );
    assert_eq!(
        after.len(),
        1,
        "a memory with no retraction revision is not retracted: {after:?}"
    );

    // An empty candidate list is a caller asking about nothing, not a reason to
    // scan the table.
    assert!(
        storage
            .retracted_node_ids_at_or_before(later, &[])
            .unwrap()
            .is_empty()
    );
}

/// The rendered timeline's content cap is part of the read contract every client
/// renders truncation from, so its value is pinned here — the same way the
/// dashboard's limits are pinned — and a change has to be deliberate.
#[test]
fn revision_content_cap_is_the_documented_value() {
    assert_eq!(
        crate::REVISION_CONTENT_CHAR_LIMIT,
        500,
        "the cap is advertised in tool output; changing it changes the wire contract"
    );
}
