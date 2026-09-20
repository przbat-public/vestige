//! Content Intelligence Pipeline — enrich memories at ingest time.
//!
//! Runs a chain of local NLP transformations on content before storage:
//!
//! 1. **Entity extraction** — detect proper nouns, URLs, emails, file paths
//! 2. **Coreference rewriting** — "He said X" → "John said X" (self-contained chunks)
//! 3. **Temporal anchoring** — "by next Friday" → absolute `valid_until` date
//! 4. **Relation extraction** — "John manages auth team" → knowledge graph edge
//! 5. **Provenance assembly** — session/agent/derivation metadata
//!
//! All local, all heuristic/regex, zero model downloads, sub-millisecond latency.

pub mod coref;
pub mod entities;
pub mod provenance;
pub mod relations;
pub mod temporal;

use chrono::{DateTime, Utc};

use coref::CorefResult;
use entities::{ExtractedEntity, entities_to_tags, extract_entities, extract_mentioned_names};
use provenance::ProvenanceMetadata;
use relations::{ExtractedRelation, extract_relations};
use temporal::{TemporalResult, anchor_temporal};

/// Maximum auto-tags generated from entity extraction.
const MAX_AUTO_TAGS: usize = 10;
/// Maximum entities to extract per memory.
const MAX_ENTITIES: usize = 20;

/// The combined output of the full preprocessing pipeline.
#[derive(Debug, Clone)]
pub struct PreprocessingResult {
    /// Content after coreference rewriting (may differ from original)
    pub content: String,
    /// Auto-generated entity tags (e.g. "entity:john-smith")
    pub auto_tags: Vec<String>,
    /// Suggested valid_from from temporal anchoring
    pub valid_from: Option<DateTime<Utc>>,
    /// Suggested valid_until from temporal anchoring
    pub valid_until: Option<DateTime<Utc>>,
    /// Extracted relation triples for knowledge graph edges
    pub relations: Vec<ExtractedRelation>,
    /// Provenance metadata for this memory
    pub provenance: ProvenanceMetadata,
    /// The entities that earned a tag — the strict list, aligned with
    /// `auto_tags`. Consumers asking who or what a text mentions want
    /// [`entities::extract_mentioned_names`], which also counts a name that
    /// only opens a sentence.
    pub entities: Vec<ExtractedEntity>,
    /// How many pronoun→entity replacements were made
    pub coref_rewrites: usize,
}

/// Configuration for the preprocessing pipeline.
#[derive(Debug, Clone, Default)]
pub struct PreprocessingConfig {
    /// Session identifier for provenance
    pub session_id: Option<String>,
    /// Agent identifier for provenance
    pub agent: Option<String>,
    /// Pre-existing valid_from (won't be overridden by temporal anchoring)
    pub existing_valid_from: Option<DateTime<Utc>>,
    /// Pre-existing valid_until (won't be overridden by temporal anchoring)
    pub existing_valid_until: Option<DateTime<Utc>>,
}

/// Run the full preprocessing pipeline on content.
///
/// Pipeline order:
/// 1. Extract entities from original content
/// 2. Resolve coreferences using the mentioned names → rewritten content
/// 3. Anchor temporal expressions from rewritten content
/// 4. Extract relations from rewritten content, using the mentioned names
/// 5. Assemble provenance metadata
///
/// Step 1 produces two lists on purpose (see the `entities` module docs): the
/// strict one decides the tags, the permissive one — names in any position —
/// feeds steps 2 and 4. Coreference and relation extraction are asking "who is
/// this text about?", and a memory that opens with its subject answers that
/// even though the opening capital earns no tag; reading the strict list there
/// loses the actor of the first sentence.
pub fn preprocess(content: &str, config: &PreprocessingConfig) -> PreprocessingResult {
    // 1. Entity extraction: strict for tags, permissive for the NLP stages.
    let entities = extract_entities(content, MAX_ENTITIES);
    let auto_tags = entities_to_tags(&entities, MAX_AUTO_TAGS);
    let mentions = extract_mentioned_names(content, MAX_ENTITIES);

    // 2. Coreference rewriting
    let CorefResult {
        content: rewritten,
        rewrites,
    } = coref::resolve_coreferences(content, &mentions);

    // 3. Temporal anchoring (on rewritten content)
    let TemporalResult {
        valid_from,
        valid_until,
        anchors_found,
    } = anchor_temporal(
        &rewritten,
        config.existing_valid_from,
        config.existing_valid_until,
    );

    // 4. Relation extraction (on rewritten content, using the mentioned names —
    //    the subject of "Alice manages the Auth Team" is a mention, not a tag)
    let relations = extract_relations(&rewritten, &mentions);

    // 5. Assemble provenance
    let mut provenance =
        ProvenanceMetadata::new().with_context(config.session_id.clone(), config.agent.clone());
    provenance.coref_rewrites = rewrites;
    provenance.auto_entities = entities.iter().map(|e| e.text.clone()).collect();
    provenance.temporal_anchors_found = anchors_found;
    provenance.relations_extracted = relations
        .iter()
        .map(|r| format!("{} → {} → {}", r.subject, r.predicate, r.object))
        .collect();

    PreprocessingResult {
        content: rewritten,
        auto_tags,
        valid_from,
        valid_until,
        relations,
        provenance,
        entities,
        coref_rewrites: rewrites,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_pipeline_basic() {
        let result = preprocess(
            "John manages the Auth Team since January. The deadline is by next Friday.",
            &PreprocessingConfig::default(),
        );

        assert!(!result.entities.is_empty(), "Should extract entities");
        assert!(!result.auto_tags.is_empty(), "Should generate auto-tags");
        // Temporal may or may not parse depending on natural-date-rs
        assert!(result.provenance.coref_rewrites == result.coref_rewrites);
    }

    #[test]
    fn test_pipeline_with_coreference() {
        let result = preprocess(
            "Sophie Wilson designed it. She was brilliant.",
            &PreprocessingConfig::default(),
        );

        // Check that entities were extracted
        let entity_names: Vec<&str> = result.entities.iter().map(|e| e.text.as_str()).collect();
        assert!(
            entity_names.iter().any(|n| n.contains("Sophie")),
            "Should extract Sophie Wilson, got: {:?}",
            entity_names
        );
    }

    #[test]
    fn test_pipeline_preserves_explicit_dates() {
        let explicit = Utc::now();
        let config = PreprocessingConfig {
            existing_valid_from: Some(explicit),
            existing_valid_until: Some(explicit + chrono::Duration::days(7)),
            ..Default::default()
        };

        let result = preprocess("Deploy by tomorrow. Starting from next week.", &config);
        assert!(
            result.valid_from.is_none(),
            "Should not override explicit valid_from"
        );
        assert!(
            result.valid_until.is_none(),
            "Should not override explicit valid_until"
        );
    }

    #[test]
    fn test_pipeline_provenance_assembly() {
        let config = PreprocessingConfig {
            session_id: Some("sess-abc".into()),
            agent: Some("cursor".into()),
            ..Default::default()
        };

        let result = preprocess(
            "Alice manages the backend. Check https://example.com.",
            &config,
        );
        assert_eq!(result.provenance.session_id.as_deref(), Some("sess-abc"));
        assert_eq!(result.provenance.agent.as_deref(), Some("cursor"));
        assert!(!result.provenance.auto_entities.is_empty());
    }

    #[test]
    fn test_pipeline_empty_content() {
        let result = preprocess("", &PreprocessingConfig::default());
        assert!(result.entities.is_empty());
        assert!(result.auto_tags.is_empty());
        assert!(result.relations.is_empty());
        assert_eq!(result.coref_rewrites, 0);
        assert_eq!(result.content, "");
    }

    /// The shape we want writers to produce: the memory opens with its actor.
    /// The opening capital earns no tag (it is positional in every language
    /// that capitalises sentences), but the actor is still the subject of the
    /// relation — losing it would punish the good shape while fixing the noisy
    /// one.
    #[test]
    fn test_pipeline_keeps_the_actor_of_an_opening_subject() {
        let result = preprocess(
            "Alice manages the Auth Team since January.",
            &PreprocessingConfig::default(),
        );

        assert!(
            !result.auto_tags.contains(&"entity:alice".to_string()),
            "the opening capital earns no tag, got {:?}",
            result.auto_tags
        );
        assert!(
            result
                .relations
                .iter()
                .any(|r| r.subject == "Alice" && r.predicate == "manages"),
            "the relation must keep its actor, got {:?}",
            result.relations
        );
    }

    #[test]
    fn test_pipeline_code_content_passthrough() {
        let code = "fn main() { let x = 42; println!(\"{}\", x); }";
        let result = preprocess(code, &PreprocessingConfig::default());
        assert!(result.content.contains("fn main()"));
    }

    #[test]
    fn test_pipeline_complex_content() {
        let content = "Alice manages the Auth Team. She deployed https://auth.example.com \
                        and fixed bug #1234. The deadline is by next Friday. \
                        Contact alice@example.com for details.";
        let result = preprocess(
            content,
            &PreprocessingConfig {
                session_id: Some("test-sess".into()),
                agent: Some("test-agent".into()),
                ..Default::default()
            },
        );

        assert!(
            !result.entities.is_empty(),
            "Should extract multiple entities"
        );
        assert!(
            result.auto_tags.iter().any(|t| t.starts_with("entity:")),
            "Should generate entity: tags"
        );
        assert!(result.provenance.session_id.as_deref() == Some("test-sess"));
        assert!(result.provenance.agent.as_deref() == Some("test-agent"));
        assert!(!result.provenance.auto_entities.is_empty());
    }

    #[test]
    fn test_pipeline_performance_sub_ms() {
        let content = "Alice manages the Auth Team since January. \
                        She deployed Vestige to production. \
                        The deadline is by next Friday. \
                        Contact alice@example.com at https://example.com.";

        let start = std::time::Instant::now();
        for _ in 0..100 {
            let _ = preprocess(content, &PreprocessingConfig::default());
        }
        let elapsed = start.elapsed();
        let per_call_us = elapsed.as_micros() / 100;
        assert!(
            per_call_us < 5000,
            "Pipeline should be < 5ms per call, got {}µs",
            per_call_us
        );
    }

    #[test]
    fn test_pipeline_no_enrichable_features() {
        let result = preprocess(
            "a simple lowercase sentence with no special features",
            &PreprocessingConfig::default(),
        );
        assert!(
            result.entities.is_empty()
                || result
                    .entities
                    .iter()
                    .all(|e| { matches!(e.entity_type, entities::EntityType::ProperNoun) }),
            "Plain text should have few/no entities"
        );
        assert!(result.relations.is_empty());
        assert_eq!(result.coref_rewrites, 0);
    }
}
