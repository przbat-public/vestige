//! Coreference rewriting — resolve pronouns to their referents.
//!
//! Makes each memory self-contained by replacing pronouns ("he", "she", "they", "it")
//! with the most recently mentioned entity. This improves search recall because
//! the stored text contains the actual entity name instead of an opaque pronoun.
//!
//! Heuristic approach (no LLM):
//! - Track entities extracted from the text
//! - Replace pronouns with the closest matching entity by type
//! - Only rewrite when confidence is reasonable (single clear referent)

use super::entities::{EntityType, ExtractedEntity};

/// Result of coreference rewriting.
#[derive(Debug, Clone)]
pub struct CorefResult {
    /// The rewritten content (or original if no changes)
    pub content: String,
    /// Number of pronoun replacements made
    pub rewrites: usize,
}

const PERSON_PRONOUNS: &[&str] = &[
    "he", "she", "him", "her", "his", "hers",
    "He", "She", "Him", "Her", "His", "Hers",
];

const THING_PRONOUNS: &[&str] = &["it", "It", "its", "Its"];

const PLURAL_PRONOUNS: &[&str] = &["they", "They", "them", "Them", "their", "Their"];

/// Rewrite pronouns in `content` using the provided extracted entities.
///
/// Only rewrites when there's a single unambiguous referent for the pronoun type.
/// Returns the original content unchanged if no entities match or if ambiguous.
pub fn resolve_coreferences(content: &str, entities: &[ExtractedEntity]) -> CorefResult {
    let persons: Vec<&ExtractedEntity> = entities.iter()
        .filter(|e| e.entity_type == EntityType::Person)
        .collect();

    let things: Vec<&ExtractedEntity> = entities.iter()
        .filter(|e| matches!(e.entity_type,
            EntityType::Organization | EntityType::ProperNoun
        ))
        .collect();

    if persons.is_empty() && things.is_empty() {
        return CorefResult { content: content.to_string(), rewrites: 0 };
    }

    let mut result = content.to_string();
    let mut rewrites = 0;

    // Only rewrite if there's exactly one person (unambiguous)
    if persons.len() == 1 {
        let person = &persons[0].text;
        for pronoun in PERSON_PRONOUNS {
            let replacement = match *pronoun {
                "his" | "His" => format!("{}'s", person),
                "her" if content.contains(&format!("her ")) => format!("{}'s", person),
                "hers" | "Hers" => format!("{}'s", person),
                "him" | "Him" | "her" | "Her" => person.clone(),
                _ => person.clone(),
            };
            let count = replace_pronoun_occurrences(&mut result, pronoun, &replacement);
            rewrites += count;
        }
    }

    // Only rewrite "it"/"its" if there's exactly one non-person entity
    if things.len() == 1 {
        let thing = &things[0].text;
        for pronoun in THING_PRONOUNS {
            let replacement = match *pronoun {
                "its" | "Its" => format!("{}'s", thing),
                _ => thing.clone(),
            };
            let count = replace_pronoun_occurrences(&mut result, pronoun, &replacement);
            rewrites += count;
        }
    }

    // Only rewrite "they"/"them" if there's exactly one organization
    let orgs: Vec<&&ExtractedEntity> = things.iter()
        .filter(|e| e.entity_type == EntityType::Organization)
        .collect();
    if orgs.len() == 1 && persons.is_empty() {
        let org = &orgs[0].text;
        for pronoun in PLURAL_PRONOUNS {
            let replacement = match *pronoun {
                "their" | "Their" => format!("{}'s", org),
                _ => org.clone(),
            };
            let count = replace_pronoun_occurrences(&mut result, pronoun, &replacement);
            rewrites += count;
        }
    }

    CorefResult { content: result, rewrites }
}

/// Replace whole-word occurrences of `pronoun` in `text` with `replacement`.
/// Returns the number of replacements made.
fn replace_pronoun_occurrences(text: &mut String, pronoun: &str, replacement: &str) -> usize {
    let mut count = 0;
    let mut new_text = String::with_capacity(text.len());
    let mut remaining = text.as_str();

    while let Some(pos) = remaining.find(pronoun) {
        let before = if pos > 0 { remaining.as_bytes()[pos - 1] } else { b' ' };
        let after_pos = pos + pronoun.len();
        let after = if after_pos < remaining.len() {
            remaining.as_bytes()[after_pos]
        } else {
            b' '
        };

        let is_word_boundary = |b: u8| -> bool {
            !b.is_ascii_alphanumeric() && b != b'_' && b != b'\''
        };

        if is_word_boundary(before) && is_word_boundary(after) {
            new_text.push_str(&remaining[..pos]);
            new_text.push_str(replacement);
            remaining = &remaining[after_pos..];
            count += 1;
        } else {
            new_text.push_str(&remaining[..after_pos]);
            remaining = &remaining[after_pos..];
        }
    }
    new_text.push_str(remaining);

    if count > 0 {
        *text = new_text;
    }

    count
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
    fn test_single_person_pronoun_resolution() {
        let entities = vec![person("Sophie Wilson")];
        let result = resolve_coreferences("She designed the ARM processor. He mentioned her work.", &entities);
        assert!(result.content.contains("Sophie Wilson designed the ARM processor"));
        assert!(result.rewrites > 0);
    }

    #[test]
    fn test_ambiguous_persons_no_rewrite() {
        let entities = vec![person("Alice"), person("Bob")];
        let result = resolve_coreferences("He said something to her.", &entities);
        assert_eq!(result.rewrites, 0, "Ambiguous referents should not be rewritten");
        assert!(result.content.contains("He said"));
    }

    #[test]
    fn test_single_org_it_resolution() {
        let entities = vec![proper("Vestige")];
        let result = resolve_coreferences("It uses FSRS-6 for scheduling. Its architecture is modular.", &entities);
        assert!(result.content.contains("Vestige uses FSRS-6"));
        assert!(result.content.contains("Vestige's architecture"));
    }

    #[test]
    fn test_no_entities_no_change() {
        let result = resolve_coreferences("He went to the store.", &[]);
        assert_eq!(result.content, "He went to the store.");
        assert_eq!(result.rewrites, 0);
    }

    #[test]
    fn test_no_pronouns_no_change() {
        let entities = vec![person("Alice")];
        let result = resolve_coreferences("Alice went to the store.", &entities);
        assert_eq!(result.rewrites, 0);
    }

    #[test]
    fn test_word_boundary_respected() {
        let entities = vec![person("Alice")];
        let result = resolve_coreferences("The shepherd walked the sheep.", &entities);
        // "he" inside "shepherd" and "the" should NOT be replaced
        assert_eq!(result.rewrites, 0);
    }

    #[test]
    fn test_org_they_resolution() {
        let entities = vec![org("Auth Team")];
        let result = resolve_coreferences("They handle authentication. Their codebase is clean.", &entities);
        assert!(result.content.contains("Auth Team handle authentication"));
        assert!(result.content.contains("Auth Team's codebase"));
    }

    #[test]
    fn test_possessive_his() {
        let entities = vec![person("John")];
        let result = resolve_coreferences("His code was clean.", &entities);
        assert!(result.content.contains("John's code"), "Expected possessive rewrite, got: {}", result.content);
    }

    #[test]
    fn test_multiple_pronouns_same_sentence() {
        let entities = vec![person("Alice")];
        let result = resolve_coreferences("She wrote the code and she tested it.", &entities);
        assert!(result.rewrites >= 2, "Should rewrite multiple pronouns, got {} rewrites", result.rewrites);
    }

    #[test]
    fn test_mixed_org_and_person_no_they_rewrite() {
        let entities = vec![person("Bob"), org("Acme Corp")];
        let result = resolve_coreferences("They have a meeting.", &entities);
        assert!(!result.content.contains("Bob"), "Ambiguous: both person and org present, should not rewrite 'they'");
    }

    #[test]
    fn test_unicode_content_passthrough() {
        let entities = vec![person("Józef")];
        let result = resolve_coreferences("He napisał raport.", &entities);
        assert!(result.content.contains("Józef napisał"), "Unicode names should be preserved");
    }
}
