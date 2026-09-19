//! Deterministic freshness resolution for conflicting memories.
//!
//! When two stored memories state conflicting facts, exactly one of them is the
//! current version. This module owns that decision, so every caller — the
//! ingest gate, the dream cycle, `deep_reference`, `temporal current` — names
//! the same winner for the same pair.
//!
//! # The rule
//!
//! The current version is the one that is **newer**, where a candidate's
//! freshness is the greatest timestamp it actually holds, taken over this
//! explicit, ordered list of slots:
//!
//! 1. [`FreshnessKey::valid_from`] — valid time: when the stated fact became
//!    true.
//! 2. [`FreshnessKey::recorded_at`] — transaction time: when the memory was
//!    recorded.
//!
//! The greater instant wins. Only if both candidates hold the same instant does
//! [`FreshnessKey::memory_id`] decide: the lexicographically greater id wins.
//! Ids are UUIDs, so that winner is arbitrary; it is a determinism device, not a
//! claim about the world. What it buys is that an exact tie resolves the same
//! way on every run and in every input order.
//!
//! A slot a candidate does not hold contributes nothing. It is never
//! substituted with `Utc::now()`, with the other candidate's value, or with a
//! lower-priority field, and it never counts against the candidate either: a
//! memory is never judged older than the moment it was recorded. A candidate
//! that holds no timestamp at all has no instant, is ordered below every
//! candidate that has one, and so never wins.
//!
//! # Why the maximum, and not a field-by-field comparison
//!
//! Comparing the slots in priority order — `valid_from` first, `recorded_at`
//! only if the first is equal — is the other natural reading of a bi-temporal
//! tuple, and it is wrong for this schema. `valid_from` is nullable and only
//! the temporal extractor fills it, so with `A(valid_from = 2024-01-01,
//! recorded 2024-01-01)` and `B(valid_from = none, recorded 2026-09-01)` — an
//! anchored old version against a later correction that carries no anchor — the
//! priority comparison names A current and the superseded fact wins. That is
//! the failure this module exists to remove, and it would also mean a candidate
//! loses for lacking a field rather than for being older. Taking the maximum
//! never judges a memory older than its own recording, and it agrees with the
//! incumbent read path's `ORDER BY created_at DESC` whenever no anchor lies
//! after the recording.
//!
//! # Slots that are excluded on purpose
//!
//! `updated_at` and `last_accessed` are *not* freshness slots. Both are bumped
//! by every review and retrieval (`storage/sqlite/review.rs` writes
//! `updated_at = last_accessed = now()`), so they measure how often a memory was
//! used, not how recent its claim is. Ranking by them is how an invalidated fact
//! returns and outranks its own correction. `access_count` is excluded for the
//! same reason: popularity is not freshness.
//!
//! # What this removes
//!
//! Before this rule the same pair could resolve differently depending on who
//! asked. The dream cycle ranked the survivor by `access_count` first and fell
//! through to "whichever memory the discovered connection named second", so on
//! an unchanged database one run demoted a memory that the read path, the
//! `reflect` pass and the next run all called current. Conflicts now resolve
//! from stored fields, in Rust, with no LLM and no incidental ordering in the
//! loop: replaying a run over the same database returns the same answer.
//!
//! Explicit revocation — a memory whose `valid_until` has passed — is enforced
//! by the read paths that filter on it, not here: this rule decides which of
//! two *live* versions is current, and folding revocation into it would make
//! the answer depend on the wall clock.

use std::cmp::Ordering;

use chrono::{DateTime, Utc};

use super::node::KnowledgeNode;

/// A memory reduced to the fields that decide freshness.
///
/// Build the key from the richest representation available
/// ([`FreshnessKey::from_node`] for stored nodes). Passing `None` for a slot is
/// a statement that the caller holds no value for it — the decision then falls
/// to the slots that do hold one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshnessKey {
    /// Valid time: when the stated fact became true.
    pub valid_from: Option<DateTime<Utc>>,
    /// Transaction time: when the memory was recorded.
    pub recorded_at: Option<DateTime<Utc>>,
    /// Identity, consulted only to break an exact tie. See the module docs.
    pub memory_id: String,
}

impl FreshnessKey {
    /// Build a key from explicit slots.
    pub fn new(
        memory_id: impl Into<String>,
        valid_from: Option<DateTime<Utc>>,
        recorded_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            valid_from,
            recorded_at,
            memory_id: memory_id.into(),
        }
    }

    /// Build a key from a stored node.
    ///
    /// `created_at` is the transaction-time slot. `updated_at` is intentionally
    /// left out: it is advanced by review and retrieval as well as by content
    /// edits, so it cannot distinguish "revised" from "looked at again".
    pub fn from_node(node: &KnowledgeNode) -> Self {
        Self::new(node.id.clone(), node.valid_from, Some(node.created_at))
    }

    /// The greatest timestamp this candidate holds, or `None` when it holds
    /// none. This is the value the rule compares — see the module docs for why
    /// the greatest one and not the highest-priority one.
    pub fn instant(&self) -> Option<DateTime<Utc>> {
        self.valid_from.max(self.recorded_at)
    }

    /// Total order over two conflicting memories: `Greater` means `self` is the
    /// current version.
    pub fn compare(&self, other: &Self) -> Ordering {
        self.instant()
            .cmp(&other.instant())
            .then_with(|| compare_memory_ids(&self.memory_id, &other.memory_id))
    }

    /// Whether `self` is the current version of the fact both memories state.
    pub fn is_fresher_than(&self, other: &Self) -> bool {
        self.compare(other) == Ordering::Greater
    }
}

/// The stable identity tiebreak, exposed for conflict sites that have no
/// timestamps to compare but must still name a deterministic winner of an exact
/// tie. `Greater` means `a` wins.
pub fn compare_memory_ids(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}

/// The freshest candidate, or `None` when there are no candidates.
///
/// The result depends only on the set of keys, never on their order: the total
/// order above makes this a fold with a unique maximum.
pub fn newest<'a, I>(candidates: I) -> Option<&'a FreshnessKey>
where
    I: IntoIterator<Item = &'a FreshnessKey>,
{
    candidates.into_iter().fold(None, |best, key| match best {
        Some(current) if !key.is_fresher_than(current) => Some(current),
        _ => Some(key),
    })
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn base() -> DateTime<Utc> {
        // Fixed instant: the rule must not read the clock, and a test that
        // cannot depend on "now" is the cheapest way to keep it that way.
        DateTime::parse_from_rfc3339("2026-09-19T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn key(id: &str, valid_from_days: Option<i64>, recorded_days: i64) -> FreshnessKey {
        FreshnessKey::new(
            id,
            valid_from_days.map(|d| base() + Duration::days(d)),
            Some(base() + Duration::days(recorded_days)),
        )
    }

    #[test]
    fn newest_is_independent_of_candidate_order_and_repetition() {
        let alpha = key("alpha", None, 5);
        let bravo = key("bravo", Some(2), 1);
        let charlie = key("charlie", Some(2), 3);
        let delta = key("delta", Some(10), 0);

        // Every ordering of the same set, including the reversed one, plus the
        // same order repeated: the winner is a property of the set.
        let orderings: [[&FreshnessKey; 4]; 4] = [
            [&alpha, &bravo, &charlie, &delta],
            [&delta, &charlie, &bravo, &alpha],
            [&charlie, &alpha, &delta, &bravo],
            [&bravo, &delta, &alpha, &charlie],
        ];

        for ordering in orderings {
            for _ in 0..3 {
                let winner = newest(ordering).expect("non-empty candidate set");
                assert_eq!(winner.memory_id, "delta");
            }
        }
    }

    #[test]
    fn candidate_without_any_timestamp_never_beats_one_with_a_timestamp() {
        let undated = FreshnessKey::new("undated", None, None);
        let recorded = key("recorded", None, 1);
        let anchored = key("anchored", Some(-30), -30);

        assert!(recorded.is_fresher_than(&undated));
        assert!(anchored.is_fresher_than(&undated));
        assert!(!undated.is_fresher_than(&recorded));
        assert_eq!(newest([&undated, &recorded]).unwrap().memory_id, "recorded");
        assert_eq!(newest([&anchored, &undated]).unwrap().memory_id, "anchored");
    }

    #[test]
    fn a_past_valid_time_anchor_does_not_make_a_later_recording_stale() {
        // The pair that a field-by-field comparison gets wrong: the older
        // version carries a valid-time anchor, the correction carries none.
        // Presence of a field must not outrank being newer, otherwise the
        // superseded fact wins every time an extractor anchored it.
        let anchored_old = key("anchored-old", Some(0), 0);
        let later_correction = key("later-correction", None, 100);

        assert!(later_correction.is_fresher_than(&anchored_old));
        assert!(!anchored_old.is_fresher_than(&later_correction));
        assert_eq!(
            newest([&anchored_old, &later_correction])
                .unwrap()
                .memory_id,
            "later-correction"
        );
    }

    #[test]
    fn exact_tie_is_decided_by_memory_id_not_insertion_order() {
        // Equal instants reached through different slots, so the tiebreak is on
        // the compared value and not on which slots happened to be filled.
        let from_anchor = key("zzz", Some(1), 0);
        let from_recording = key("aaa", None, 1);
        assert_eq!(from_anchor.instant(), from_recording.instant());

        for _ in 0..8 {
            assert_eq!(
                newest([&from_anchor, &from_recording]).unwrap().memory_id,
                "zzz"
            );
            assert_eq!(
                newest([&from_recording, &from_anchor]).unwrap().memory_id,
                "zzz"
            );
        }
    }

    #[test]
    fn freshness_is_a_total_order_over_candidates_with_missing_slots() {
        // A comparator that skipped absent slots could not order this triple:
        // it would answer `b > a`, `c > b` and `a > c` at once, and a cyclic
        // comparator makes "first candidate wins" depend on input order again.
        let a = key("a", Some(0), 0);
        let b = key("b", None, 10);
        let c = key("c", Some(-5), 20);

        assert!(c.is_fresher_than(&b));
        assert!(b.is_fresher_than(&a));
        assert!(
            c.is_fresher_than(&a),
            "transitivity must survive missing slots"
        );

        for ordering in [
            [&a, &b, &c],
            [&a, &c, &b],
            [&b, &a, &c],
            [&b, &c, &a],
            [&c, &a, &b],
            [&c, &b, &a],
        ] {
            assert_eq!(newest(ordering).unwrap().memory_id, "c");
        }
    }
}
