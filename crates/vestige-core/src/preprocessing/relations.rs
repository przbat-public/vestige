//! Relation extraction — extract subject-verb-object triples from text.
//!
//! Builds knowledge graph edges at ingest time by detecting simple
//! SVO (subject-verb-object) patterns. These edges feed the spreading
//! activation network, making graph traversal richer from day one
//! instead of waiting for dream consolidation.
//!
//! Heuristic-based: no model downloads, no ONNX.
//!
//! # Causality vs other relations
//!
//! Until 2026-05-22 every extracted triple was forced to
//! [`LinkType::Causal`] when handed to the activation network — even
//! when the verb was clearly non-causal (`manages`, `uses`, `contains`).
//! That polluted causal-chain traversals with reporting/ownership links
//! and made the "why X?" queries useless.
//!
//! Each predicate is now classified into a [`LinkType`] at extraction
//! time so downstream code (post-ingest edge creation, the
//! `explore_connections` tool, dream traversal) sees the right kind of
//! relationship without re-parsing the verb. The taxonomy is small on
//! purpose:
//!
//! | LinkType  | Predicates (examples)                          |
//! |-----------|------------------------------------------------|
//! | `Causal`  | causes, requires, enables, triggers, leads to  |
//! | `PartOf`  | contains, includes, belongs to, is part of     |
//! | `Temporal`| precedes, follows, before, after               |
//! | `Semantic`| manages, leads, uses, depends on, implements…  |
//!
//! `Semantic` is the default — every predicate that doesn't fall into
//! one of the more specific buckets is treated as a generic semantic
//! association.

use super::entities::{EntityType, ExtractedEntity};
use crate::neuroscience::spreading_activation::LinkType;
use regex::Regex;
use std::sync::LazyLock;

/// An extracted relation triple: subject → predicate → object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedRelation {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    /// Classified relationship type derived from the predicate.
    ///
    /// Defaulted to [`LinkType::Semantic`] for backwards compatibility
    /// with callers that build `ExtractedRelation` literals manually.
    pub link_type: LinkType,
}

/// Map a predicate verb phrase to the appropriate [`LinkType`].
///
/// Comparison is case-insensitive and tolerant of stray whitespace.
/// Unknown verbs are classified as [`LinkType::Semantic`] — that's the
/// safe default because the spreading-activation network treats Semantic
/// edges as the baseline link and we don't want to over-claim causality.
pub fn predicate_to_link_type(predicate: &str) -> LinkType {
    let key = predicate.trim().to_lowercase();
    match key.as_str() {
        // Causal — A makes B happen, A is required for B, A enables B.
        // "leads to" intentionally NOT included here: in business prose
        // ("Alice leads the team") it's hierarchical, not causal.
        "causes" | "requires" | "enables" | "triggers" => LinkType::Causal,
        // Part–whole / containment.
        "contains" | "includes" | "belongs to" | "is part of" => LinkType::PartOf,
        // Temporal ordering. Currently only "precedes" / "follows" make
        // it through the relation-verb regex; the others are listed for
        // future verb-set expansion so the mapper stays exhaustive.
        "precedes" | "follows" | "before" | "after" => LinkType::Temporal,
        // Everything else (manages, leads, owns, uses, depends on,
        // implements, extends, replaces, …) is a generic semantic link.
        _ => LinkType::Semantic,
    }
}

// Common relationship verbs that connect entities. The literal is constant
// so `expect()` documents that this can only fail if a future edit breaks
// the regex syntax — never on user input.
static RELATION_VERBS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(manages|leads|owns|created|designed|built|maintains|uses|depends on|implements|extends|replaces|causes|requires|enables|contains|includes|belongs to|works on|reports to|wrote|authored|developed|runs|deploys|hosts|serves|handles|processes|stores|connects to|integrates with|is part of|is responsible for)\b")
        .expect("relations.rs RELATION_VERBS regex literal")
});

/// Extract relation triples from content using the extracted entities.
///
/// Looks for patterns: `[Entity] [verb phrase] [Entity/noun phrase]`
/// Only extracts relations where at least the subject is a known entity.
pub fn extract_relations(content: &str, entities: &[ExtractedEntity]) -> Vec<ExtractedRelation> {
    let mut relations = Vec::new();

    // Build a set of entity texts for fast lookup
    let entity_texts: Vec<&str> = entities
        .iter()
        .filter(|e| {
            matches!(
                e.entity_type,
                EntityType::Person | EntityType::Organization | EntityType::ProperNoun
            )
        })
        .map(|e| e.text.as_str())
        .collect();

    if entity_texts.is_empty() {
        return relations;
    }

    // Split into sentences for local analysis
    for sentence in split_sentences(content) {
        extract_from_sentence(sentence.trim(), &entity_texts, &mut relations);
    }

    relations
}

fn extract_from_sentence(
    sentence: &str,
    entity_texts: &[&str],
    relations: &mut Vec<ExtractedRelation>,
) {
    // Find all entity positions in this sentence
    let mut entity_positions: Vec<(&str, usize)> = Vec::new();
    for entity in entity_texts {
        if let Some(pos) = sentence.find(entity) {
            entity_positions.push((entity, pos));
        }
    }

    entity_positions.sort_by_key(|&(_, pos)| pos);

    // Find verbs in the sentence
    for verb_match in RELATION_VERBS.find_iter(sentence) {
        let verb_start = verb_match.start();
        let verb_end = verb_match.end();
        let verb = verb_match.as_str().to_lowercase();

        // Find the closest entity BEFORE the verb (subject)
        let subject = entity_positions
            .iter()
            .filter(|&&(entity, pos)| pos + entity.len() <= verb_start)
            .max_by_key(|&&(_, pos)| pos);

        // Find the closest entity AFTER the verb (object)
        let object = entity_positions
            .iter()
            .filter(|&&(_, pos)| pos >= verb_end)
            .min_by_key(|&&(_, pos)| pos);

        if let Some(&(subj, _)) = subject {
            let obj_text = if let Some(&(obj, _)) = object {
                obj.to_string()
            } else {
                // No entity after verb — extract noun phrase after verb
                extract_noun_phrase(&sentence[verb_end..])
            };

            if !obj_text.is_empty() && subj != obj_text {
                let link_type = predicate_to_link_type(&verb);
                relations.push(ExtractedRelation {
                    subject: subj.to_string(),
                    predicate: verb.clone(),
                    object: obj_text,
                    link_type,
                });
            }
        }
    }
}

/// Extract a simple noun phrase from the beginning of text.
/// Takes the first 1-4 words that look like a noun phrase.
fn extract_noun_phrase(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let words: Vec<&str> = trimmed
        .split_whitespace()
        .take(5)
        .take_while(|w| {
            let first = w.chars().next().unwrap_or(' ');
            first.is_alphabetic() || first == '"'
        })
        .collect();

    // Skip leading articles/prepositions
    let skip = words
        .iter()
        .take_while(|w| {
            let lower = w.to_lowercase();
            matches!(
                lower.as_str(),
                "the" | "a" | "an" | "to" | "for" | "with" | "on" | "in" | "at"
            )
        })
        .count();

    let phrase: Vec<&str> = words.into_iter().skip(skip).take(4).collect();
    phrase
        .join(" ")
        .trim_end_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

fn split_sentences(text: &str) -> Vec<&str> {
    static SENTENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"[.!?]+\s+|[.!?]+$|\n+").expect("relations.rs SENTENCE_RE regex literal")
    });

    SENTENCE_RE
        .split(text)
        .filter(|s| !s.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str) -> ExtractedEntity {
        ExtractedEntity {
            text: name.to_string(),
            entity_type: EntityType::Person,
        }
    }

    fn org(name: &str) -> ExtractedEntity {
        ExtractedEntity {
            text: name.to_string(),
            entity_type: EntityType::Organization,
        }
    }

    fn proper(name: &str) -> ExtractedEntity {
        ExtractedEntity {
            text: name.to_string(),
            entity_type: EntityType::ProperNoun,
        }
    }

    #[test]
    fn test_basic_relation_extraction() {
        let entities = vec![person("John"), org("Auth Team")];
        let relations = extract_relations("John manages the Auth Team since January.", &entities);
        assert!(
            !relations.is_empty(),
            "Should extract at least one relation"
        );
        let r = &relations[0];
        assert_eq!(r.subject, "John");
        assert_eq!(r.predicate, "manages");
        assert!(r.object.contains("Auth Team"));
    }

    #[test]
    fn test_no_entities_no_relations() {
        let relations = extract_relations("Someone manages something.", &[]);
        assert!(relations.is_empty());
    }

    #[test]
    fn test_no_verbs_no_relations() {
        let entities = vec![person("Alice"), proper("Vestige")];
        let relations = extract_relations("Alice and Vestige.", &entities);
        assert!(relations.is_empty(), "No relation verbs → no relations");
    }

    #[test]
    fn test_multiple_relations() {
        let entities = vec![person("Alice"), proper("Vestige"), proper("MCP")];
        let relations =
            extract_relations("Alice created Vestige. Vestige implements MCP.", &entities);
        assert!(
            !relations.is_empty(),
            "Should extract at least one relation, got: {:?}",
            relations
        );
    }

    #[test]
    fn test_relation_with_noun_phrase_object() {
        let entities = vec![proper("Vestige")];
        let relations = extract_relations("Vestige uses FSRS-6 for scheduling.", &entities);
        if !relations.is_empty() {
            assert_eq!(relations[0].subject, "Vestige");
            assert!(relations[0].object.contains("FSRS"));
        }
    }

    #[test]
    fn test_empty_content() {
        let entities = vec![person("Alice")];
        let relations = extract_relations("", &entities);
        assert!(relations.is_empty());
    }

    #[test]
    fn test_different_verb_types() {
        let entities = vec![proper("Vestige"), proper("SQLite")];
        let rels = extract_relations("Vestige depends on SQLite for storage.", &entities);
        if !rels.is_empty() {
            assert_eq!(rels[0].subject, "Vestige");
            assert!(rels[0].predicate.contains("depends on"));
        }
    }

    #[test]
    fn test_subject_object_not_same() {
        let entities = vec![proper("Vestige")];
        let rels = extract_relations("Vestige extends Vestige for more features.", &entities);
        for r in &rels {
            assert_ne!(r.subject, r.object, "Subject and object should differ");
        }
    }

    #[test]
    fn test_sentence_splitting() {
        let entities = vec![person("Alice"), person("Bob")];
        let rels = extract_relations("Alice manages the team. Bob leads the project.", &entities);
        assert!(
            !rels.is_empty(),
            "Should extract relations from separate sentences"
        );
    }

    // ------------------------------------------------------------------
    // Causality-graph classification (2026-05-22)
    //
    // Before the per-predicate mapping, every relation was forced to
    // `LinkType::Causal` by the post-ingest edge writer. Asserting the
    // mapping here pins the contract so that downstream code (the
    // `explore_connections` tool, dream traversal) can rely on it.
    // ------------------------------------------------------------------

    #[test]
    fn predicate_to_link_type_recognises_causal_verbs() {
        for verb in ["causes", "requires", "enables", "triggers"] {
            assert_eq!(
                predicate_to_link_type(verb),
                LinkType::Causal,
                "`{verb}` must map to Causal — the causal-chain tool depends on this taxonomy",
            );
        }
    }

    #[test]
    fn predicate_to_link_type_recognises_part_of_verbs() {
        for verb in ["contains", "includes", "belongs to", "is part of"] {
            assert_eq!(
                predicate_to_link_type(verb),
                LinkType::PartOf,
                "`{verb}` must map to PartOf so containment isn't laundered as causality",
            );
        }
    }

    #[test]
    fn predicate_to_link_type_defaults_to_semantic() {
        // "manages" / "leads" / "uses" are common verbs whose old
        // behaviour was to silently materialise as causal edges.
        for verb in ["manages", "leads", "uses", "depends on", "implements"] {
            assert_eq!(
                predicate_to_link_type(verb),
                LinkType::Semantic,
                "`{verb}` is not causal — must default to Semantic",
            );
        }
    }

    #[test]
    fn predicate_to_link_type_is_case_insensitive() {
        assert_eq!(predicate_to_link_type("Causes"), LinkType::Causal);
        assert_eq!(predicate_to_link_type("  CAUSES  "), LinkType::Causal);
        assert_eq!(predicate_to_link_type("Belongs To"), LinkType::PartOf);
    }

    #[test]
    fn extract_relations_propagates_link_type_for_causal_sentence() {
        let entities = vec![proper("Stress"), proper("Insomnia")];
        let relations = extract_relations("Stress causes Insomnia in many cases.", &entities);
        assert!(
            !relations.is_empty(),
            "Should extract a relation for `Stress causes Insomnia`"
        );
        let causal = relations
            .iter()
            .find(|r| r.predicate.contains("causes"))
            .expect("expected at least one `causes` relation");
        assert_eq!(
            causal.link_type,
            LinkType::Causal,
            "extract_relations must classify `causes` as Causal so the causal-chain MCP \
             tool can filter on it without re-parsing the verb",
        );
    }

    #[test]
    fn extract_relations_does_not_mislabel_management_as_causal() {
        let entities = vec![person("Alice"), org("Auth Team")];
        let relations = extract_relations("Alice manages the Auth Team since January.", &entities);
        assert!(!relations.is_empty());
        for r in &relations {
            assert_ne!(
                r.link_type,
                LinkType::Causal,
                "`{}` was misclassified as causal — Alice managing a team is not causation. \
                 Pre-fix, this triple polluted causal traversals with hierarchical links.",
                r.predicate,
            );
        }
    }

    #[test]
    fn extract_relations_classifies_containment_as_part_of() {
        let entities = vec![proper("Sprint 5"), proper("Login feature")];
        let relations =
            extract_relations("Sprint 5 includes the Login feature this week.", &entities);
        let containment = relations
            .iter()
            .find(|r| r.predicate.contains("includes"))
            .expect("expected `includes` triple");
        assert_eq!(containment.link_type, LinkType::PartOf);
    }
}
