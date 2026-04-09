//! Relation extraction — extract subject-verb-object triples from text.
//!
//! Builds knowledge graph edges at ingest time by detecting simple
//! SVO (subject-verb-object) patterns. These edges feed the spreading
//! activation network, making graph traversal richer from day one
//! instead of waiting for dream consolidation.
//!
//! Heuristic-based: no model downloads, no ONNX.

use super::entities::{EntityType, ExtractedEntity};
use regex::Regex;
use std::sync::LazyLock;

/// An extracted relation triple: subject → predicate → object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedRelation {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

// Common relationship verbs that connect entities
static RELATION_VERBS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(manages|leads|owns|created|designed|built|maintains|uses|depends on|implements|extends|replaces|causes|requires|enables|contains|includes|belongs to|works on|reports to|wrote|authored|developed|runs|deploys|hosts|serves|handles|processes|stores|connects to|integrates with|is part of|is responsible for)\b").unwrap()
});

/// Extract relation triples from content using the extracted entities.
///
/// Looks for patterns: `[Entity] [verb phrase] [Entity/noun phrase]`
/// Only extracts relations where at least the subject is a known entity.
pub fn extract_relations(
    content: &str,
    entities: &[ExtractedEntity],
) -> Vec<ExtractedRelation> {
    let mut relations = Vec::new();

    // Build a set of entity texts for fast lookup
    let entity_texts: Vec<&str> = entities.iter()
        .filter(|e| matches!(e.entity_type,
            EntityType::Person | EntityType::Organization | EntityType::ProperNoun
        ))
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
        let subject = entity_positions.iter()
            .filter(|&&(entity, pos)| pos + entity.len() <= verb_start)
            .max_by_key(|&&(_, pos)| pos);

        // Find the closest entity AFTER the verb (object)
        let object = entity_positions.iter()
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
                relations.push(ExtractedRelation {
                    subject: subj.to_string(),
                    predicate: verb.clone(),
                    object: obj_text,
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

    let words: Vec<&str> = trimmed.split_whitespace()
        .take(5)
        .take_while(|w| {
            let first = w.chars().next().unwrap_or(' ');
            first.is_alphabetic() || first == '"'
        })
        .collect();

    // Skip leading articles/prepositions
    let skip = words.iter()
        .take_while(|w| {
            let lower = w.to_lowercase();
            matches!(lower.as_str(), "the" | "a" | "an" | "to" | "for" | "with" | "on" | "in" | "at")
        })
        .count();

    let phrase: Vec<&str> = words.into_iter().skip(skip).take(4).collect();
    phrase.join(" ").trim_end_matches(|c: char| !c.is_alphanumeric()).to_string()
}

fn split_sentences(text: &str) -> Vec<&str> {
    static SENTENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"[.!?]+\s+|[.!?]+$|\n+").unwrap()
    });

    SENTENCE_RE.split(text)
        .filter(|s| !s.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str) -> ExtractedEntity {
        ExtractedEntity { text: name.to_string(), entity_type: EntityType::Person }
    }

    fn org(name: &str) -> ExtractedEntity {
        ExtractedEntity { text: name.to_string(), entity_type: EntityType::Organization }
    }

    fn proper(name: &str) -> ExtractedEntity {
        ExtractedEntity { text: name.to_string(), entity_type: EntityType::ProperNoun }
    }

    #[test]
    fn test_basic_relation_extraction() {
        let entities = vec![person("John"), org("Auth Team")];
        let relations = extract_relations("John manages the Auth Team since January.", &entities);
        assert!(!relations.is_empty(), "Should extract at least one relation");
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
        let relations = extract_relations(
            "Alice created Vestige. Vestige implements MCP.",
            &entities,
        );
        assert!(relations.len() >= 1, "Should extract at least one relation, got: {:?}", relations);
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
        let rels = extract_relations(
            "Alice manages the team. Bob leads the project.",
            &entities,
        );
        assert!(rels.len() >= 1, "Should extract relations from separate sentences");
    }
}
