//! Ordered registry of every known migration.

use super::migration::Migration;
use super::sql::*;

/// Migration definitions
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "Initial schema with FSRS-6 and embeddings",
        up: MIGRATION_V1_UP,
    },
    Migration {
        version: 2,
        description: "Add temporal columns",
        up: MIGRATION_V2_UP,
    },
    Migration {
        version: 3,
        description: "Add persistence tables for neuroscience features",
        up: MIGRATION_V3_UP,
    },
    Migration {
        version: 4,
        description: "Temporal knowledge graph, memory scopes, embedding versioning",
        up: MIGRATION_V4_UP,
    },
    Migration {
        version: 5,
        description: "FSRS-6 upgrade: access history, ACT-R activation, personalized decay",
        up: MIGRATION_V5_UP,
    },
    Migration {
        version: 6,
        description: "Dream history persistence for automation triggers",
        up: MIGRATION_V6_UP,
    },
    Migration {
        version: 7,
        description: "Performance: page_size 8192, FTS5 porter tokenizer",
        up: MIGRATION_V7_UP,
    },
    Migration {
        version: 8,
        description: "v1.9.0 Autonomic: waking SWR tags, utility scoring, retention tracking",
        up: MIGRATION_V8_UP,
    },
    Migration {
        version: 9,
        description: "v2.0.0 Cognitive Leap: emotional memory, flashbulb encoding, temporal hierarchy",
        up: MIGRATION_V9_UP,
    },
    Migration {
        version: 10,
        description: "v3.1.0 Content Intelligence: provenance tracking for memory lineage",
        up: MIGRATION_V10_UP,
    },
    Migration {
        version: 11,
        description: "v3.3.0 typed memory (kind, subject, predicate, object, episodic_at, procedural_frequency)",
        up: MIGRATION_V11_UP,
    },
    Migration {
        version: 12,
        description: "v3.4.0 typed extensions: extra_json column for Decision matrix, Hub metadata, Insight payload",
        up: MIGRATION_V12_UP,
    },
    Migration {
        version: 13,
        description: "v3.5.0 Proposal A: partial indexes on extra_json.hub.clusterSignature for hub dedup",
        up: MIGRATION_V13_UP,
    },
    Migration {
        version: 14,
        description: "v3.6.0 PRAGMA auto_vacuum=INCREMENTAL (page reclamation without full VACUUM)",
        up: MIGRATION_V14_UP,
    },
    Migration {
        version: 15,
        description: "FTS5 tokenizer: porter unicode61 remove_diacritics 2 (accent folding, non-ASCII tokens)",
        up: MIGRATION_V15_UP,
    },
    Migration {
        version: 16,
        description: "Narrow FTS update trigger to content/tags; partial index for waking-tag replay",
        up: MIGRATION_V16_UP,
    },
];
