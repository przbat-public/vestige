//! Entity extraction via heuristic/regex analysis.
//!
//! Extracts proper nouns, URLs, emails, file paths, and other named entities
//! from text content. Returns normalized entity tags prefixed with `entity:`.
//!
//! No model downloads, no ONNX — pure Rust, sub-millisecond latency.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// An extracted entity with its type and surface form.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtractedEntity {
    /// The raw text of the entity as it appears in content
    pub text: String,
    /// Category: person, organization, url, email, path, date, monetary, misc
    pub entity_type: EntityType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Person,
    Organization,
    Url,
    Email,
    FilePath,
    Monetary,
    ProperNoun,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Organization => "org",
            Self::Url => "url",
            Self::Email => "email",
            Self::FilePath => "path",
            Self::Monetary => "monetary",
            Self::ProperNoun => "noun",
        }
    }
}

// Each `expect()` below documents WHY the regex can't fail at runtime — the
// pattern is a compile-time string literal vetted by tests. If any of these
// ever panics, the developer who edited the literal needs to know which one
// they broke; an explicit message beats `unwrap()`'s "called Option::unwrap on
// a None value" trace.
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s)<>\]]+").expect("entities.rs URL regex literal"));

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
        .expect("entities.rs EMAIL regex literal")
});

static FILE_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s(])(/[a-zA-Z0-9_.\-]+(?:/[a-zA-Z0-9_.\-]+)+|[a-zA-Z]:\\[^\s]+|[a-zA-Z0-9_\-]+(?:/[a-zA-Z0-9_.\-]+){2,})")
        .expect("entities.rs FILE_PATH regex literal")
});

static MONETARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\$€£¥]\s?\d[\d,]*(?:\.\d{1,2})?|\d[\d,]*(?:\.\d{1,2})?\s?(?:USD|EUR|GBP|PLN|JPY)")
        .expect("entities.rs MONETARY regex literal")
});

// Capitalized multi-word sequence (proper noun detection).
// Matches 1-4 capitalized words in a row, excluding sentence starts.
static PROPER_NOUN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[.!?]\s+|,\s+|;\s+|\b(?:is|was|by|at|in|on|to|for|with|from|and|or|the|a|an)\s+)([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,3})")
        .expect("entities.rs PROPER_NOUN regex literal")
});

/// Common words that look like proper nouns but aren't, when appearing at sentence boundaries.
const STOP_PROPER: &[&str] = &[
    "The", "This", "That", "These", "Those", "There", "Here", "It", "He", "She", "They", "We",
    "You", "I", "What", "Which", "Who", "When", "Where", "Why", "How", "And", "But", "Or", "Not",
    "If", "So", "Then", "Also", "Just", "Only", "Still", "Even", "However", "Because", "Since",
    "While", "After", "Before", "Some", "Any", "All", "Each", "Every", "Most", "New", "Old",
    "Good", "Bad", "First", "Last", "Many", "Much", "More", "Few", "Less",
];

/// Extract entities from text content.
///
/// Returns up to `max_entities` entities, prioritized by type:
/// URLs > Emails > File paths > Monetary > Proper nouns.
pub fn extract_entities(content: &str, max_entities: usize) -> Vec<ExtractedEntity> {
    let mut entities = Vec::new();
    let mut seen_texts: HashSet<String> = HashSet::new();

    let mut add = |text: String, etype: EntityType| {
        let lower = text.to_lowercase();
        if !seen_texts.contains(&lower) {
            seen_texts.insert(lower);
            entities.push(ExtractedEntity {
                text,
                entity_type: etype,
            });
        }
    };

    for m in URL_RE.find_iter(content) {
        add(m.as_str().to_string(), EntityType::Url);
    }

    for m in EMAIL_RE.find_iter(content) {
        add(m.as_str().to_string(), EntityType::Email);
    }

    for cap in FILE_PATH_RE.captures_iter(content) {
        if let Some(m) = cap.get(1).or(cap.get(0)) {
            let path = m.as_str().trim();
            if path.contains('/') || path.contains('\\') {
                add(path.to_string(), EntityType::FilePath);
            }
        }
    }

    for m in MONETARY_RE.find_iter(content) {
        add(m.as_str().to_string(), EntityType::Monetary);
    }

    // Proper nouns (capitalized sequences not at sentence starts)
    extract_proper_nouns(content, &mut seen_texts, &mut entities);

    entities.truncate(max_entities);
    entities
}

fn extract_proper_nouns(
    content: &str,
    seen_texts: &mut HashSet<String>,
    entities: &mut Vec<ExtractedEntity>,
) {
    for cap in PROPER_NOUN_RE.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let text = m.as_str().trim();

            if text.split_whitespace().all(|w| STOP_PROPER.contains(&w)) {
                continue;
            }

            let lower = text.to_lowercase();
            if seen_texts.contains(&lower) {
                continue;
            }

            let etype = classify_proper_noun(text);
            seen_texts.insert(lower);
            entities.push(ExtractedEntity {
                text: text.to_string(),
                entity_type: etype,
            });
        }
    }
}

fn classify_proper_noun(text: &str) -> EntityType {
    let words: Vec<&str> = text.split_whitespace().collect();

    // Single capitalized word with >= 3 chars that ends in common org suffixes
    if words.len() == 1 {
        let w = words[0];
        if w.ends_with("Corp") || w.ends_with("Inc") || w.ends_with("Ltd") || w.ends_with("LLC") {
            return EntityType::Organization;
        }
    }

    // Multi-word with org indicators
    let lower = text.to_lowercase();
    if lower.contains("team")
        || lower.contains("group")
        || lower.contains("corp")
        || lower.contains("inc")
        || lower.contains("company")
        || lower.contains("dept")
        || lower.contains("department")
        || lower.contains("university")
        || lower.contains("institute")
        || lower.contains("foundation")
    {
        return EntityType::Organization;
    }

    // 2-3 capitalized words → likely a person name
    if words.len() >= 2
        && words.len() <= 3
        && words
            .iter()
            .all(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
    {
        return EntityType::Person;
    }

    EntityType::ProperNoun
}

/// Convert extracted entities to auto-tags with `entity:` prefix.
///
/// Tags are slugified: "John Smith" → "entity:john-smith".
/// Capped at `max_tags` (default 10).
pub fn entities_to_tags(entities: &[ExtractedEntity], max_tags: usize) -> Vec<String> {
    entities
        .iter()
        .take(max_tags)
        .filter_map(|e| {
            let slug = slugify(&e.text);
            if slug.is_empty() || slug.len() > 60 {
                return None;
            }
            Some(format!("entity:{}", slug))
        })
        .collect()
}

fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c == ' ' || c == '_' || c == '.' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_urls() {
        let entities = extract_entities("Check https://github.com/vestige for details", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::Url && e.text.contains("github.com"))
        );
    }

    #[test]
    fn test_extract_emails() {
        let entities = extract_entities("Contact user@example.com for help", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::Email && e.text == "user@example.com")
        );
    }

    #[test]
    fn test_extract_file_paths() {
        let entities = extract_entities("Edit the file /src/main.rs to fix the bug", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::FilePath)
        );
    }

    #[test]
    fn test_extract_monetary() {
        let entities = extract_entities("The cost was $1,500.00 per month", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::Monetary)
        );
    }

    #[test]
    fn test_extract_proper_nouns() {
        let entities =
            extract_entities("The project was designed by Sophie Wilson at Cambridge", 20);
        let names: Vec<&str> = entities
            .iter()
            .filter(|e| matches!(e.entity_type, EntityType::Person | EntityType::ProperNoun))
            .map(|e| e.text.as_str())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n.contains("Sophie") || n.contains("Wilson") || n.contains("Cambridge")),
            "Expected proper nouns, got: {:?}",
            names
        );
    }

    #[test]
    fn test_stop_words_filtered() {
        let entities = extract_entities("The quick brown fox. This is not an entity.", 10);
        let texts: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
        assert!(!texts.contains(&"The"), "Should filter stop words");
        assert!(!texts.contains(&"This"), "Should filter stop words");
    }

    #[test]
    fn test_entities_to_tags() {
        let entities = vec![
            ExtractedEntity {
                text: "John Smith".to_string(),
                entity_type: EntityType::Person,
            },
            ExtractedEntity {
                text: "https://example.com".to_string(),
                entity_type: EntityType::Url,
            },
        ];
        let tags = entities_to_tags(&entities, 10);
        assert!(tags.contains(&"entity:john-smith".to_string()));
    }

    #[test]
    fn test_max_entities_cap() {
        let content = "Alice, Bob, Carol, Dave, Eve, Frank, Grace, Heidi, Ivan, Judy, Karl, Lisa";
        let entities = extract_entities(content, 5);
        assert!(entities.len() <= 5);
    }

    #[test]
    fn test_empty_content() {
        let entities = extract_entities("", 10);
        assert!(entities.is_empty());
    }

    #[test]
    fn test_dedup_entities() {
        let entities = extract_entities("Email user@test.com and also user@test.com again", 10);
        let emails: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Email)
            .collect();
        assert_eq!(emails.len(), 1, "Duplicate emails should be deduped");
    }

    #[test]
    fn test_mixed_entity_types() {
        let content =
            "Contact user@test.com at https://example.com for the $500 report on /var/log/app.log";
        let entities = extract_entities(content, 20);
        let types: Vec<EntityType> = entities.iter().map(|e| e.entity_type).collect();
        assert!(types.contains(&EntityType::Email));
        assert!(types.contains(&EntityType::Url));
        assert!(types.contains(&EntityType::Monetary));
    }

    #[test]
    fn test_organization_detection() {
        let content = "The project was led by the Engineering Department at Stanford University";
        let entities = extract_entities(content, 20);
        let orgs: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Organization)
            .collect();
        assert!(
            !orgs.is_empty(),
            "Should detect organization, got entities: {:?}",
            entities
        );
    }

    #[test]
    fn test_entity_type_as_str() {
        assert_eq!(EntityType::Person.as_str(), "person");
        assert_eq!(EntityType::Organization.as_str(), "org");
        assert_eq!(EntityType::Url.as_str(), "url");
        assert_eq!(EntityType::Email.as_str(), "email");
        assert_eq!(EntityType::FilePath.as_str(), "path");
        assert_eq!(EntityType::Monetary.as_str(), "monetary");
        assert_eq!(EntityType::ProperNoun.as_str(), "noun");
    }

    #[test]
    fn test_slugify_special_chars() {
        let entities = vec![ExtractedEntity {
            text: "user@test.com".to_string(),
            entity_type: EntityType::Email,
        }];
        let tags = entities_to_tags(&entities, 10);
        assert!(!tags.is_empty());
        assert!(tags[0].starts_with("entity:"));
        assert!(!tags[0].contains('@'), "Slug should not contain @");
    }

    #[test]
    fn test_entities_to_tags_max_cap() {
        let entities: Vec<_> = (0..20)
            .map(|i| ExtractedEntity {
                text: format!("Entity{}", i),
                entity_type: EntityType::ProperNoun,
            })
            .collect();
        let tags = entities_to_tags(&entities, 5);
        assert_eq!(tags.len(), 5);
    }

    #[test]
    fn test_eur_currency() {
        let entities = extract_entities("The price is €1,200.50 plus tax", 10);
        assert!(
            entities
                .iter()
                .any(|e| e.entity_type == EntityType::Monetary),
            "Should detect EUR currency"
        );
    }

    #[test]
    fn test_case_insensitive_dedup() {
        let entities = extract_entities("Contact admin@TEST.COM and also admin@test.com again", 10);
        let emails: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Email)
            .collect();
        assert_eq!(
            emails.len(),
            1,
            "Case-insensitive dedup should collapse identical emails"
        );
    }
}
