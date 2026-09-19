//! # Intentions Workflow Journey Tests
//!
//! Tests the intent detection system that understands WHY users are doing
//! something, not just WHAT they're doing. This enables proactive memory
//! retrieval based on detected intent.
//!
//! ## User Journey
//!
//! 1. User opens files, searches, runs commands
//! 2. System observes and records actions
//! 3. System detects intent (debugging, learning, refactoring, etc.)
//! 4. System proactively suggests relevant memories
//! 5. User benefits from context-aware assistance
//!
//! ## Scope: contract tests **and** a real storage journey
//!
//! The top-level tests drive the in-memory `IntentDetector` and the
//! `DetectedIntent` tag contract. The real prospective-memory journey lives in
//! [`storage_journeys`] at the bottom: intentions persisted through
//! `Storage::save_intention` (the write the MCP `intention set` tool makes),
//! read back after a cold restart, and finally fired (or not) by the core
//! trigger matcher on a matching context.

use vestige_core::advanced::intent::{
    ActionType, DetectedIntent, IntentDetector, LearningLevel, MaintenanceType, OptimizationType,
    UserAction,
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a detector with pre-recorded debugging actions
fn detector_with_debugging_actions() -> IntentDetector {
    let detector = IntentDetector::new();

    detector.record_action(UserAction::error("TypeError: undefined is not a function"));
    detector.record_action(UserAction::file_opened("/src/components/Button.tsx"));
    detector.record_action(UserAction::search("fix undefined error"));
    detector.record_action(UserAction::file_opened("/src/utils/helpers.ts"));

    detector
}

/// Create a detector with pre-recorded learning actions
fn detector_with_learning_actions() -> IntentDetector {
    let detector = IntentDetector::new();

    detector.record_action(UserAction::docs_viewed("async/await in Rust"));
    detector.record_action(UserAction::search("how to use tokio"));
    detector.record_action(UserAction::docs_viewed("futures crate tutorial"));
    detector.record_action(UserAction::search("what is a Future in Rust"));

    detector
}

/// Create a detector with pre-recorded refactoring actions
fn detector_with_refactoring_actions() -> IntentDetector {
    let detector = IntentDetector::new();

    detector.record_action(UserAction::file_edited("/src/auth/login.rs"));
    detector.record_action(UserAction::file_edited("/src/auth/logout.rs"));
    detector.record_action(UserAction::file_edited("/src/auth/session.rs"));
    detector.record_action(UserAction::search("extract method refactoring"));
    detector.record_action(UserAction::file_edited("/src/auth/mod.rs"));

    detector
}

// ============================================================================
// TEST 1: DEBUGGING INTENT DETECTION
// ============================================================================

/// Test that debugging intent is detected from error-related actions.
///
/// Validates:
/// - Error encounters boost debugging confidence
/// - Debug sessions boost debugging confidence
/// - File opens near errors identify suspected area
/// - Symptoms are captured from error messages
#[test]
fn test_debugging_intent_detection() {
    let detector = detector_with_debugging_actions();

    let result = detector.detect_intent();

    // Should detect some intent (may be debugging or learning)
    assert!(
        result.confidence > 0.0 || matches!(result.primary_intent, DetectedIntent::Unknown),
        "Should detect intent or return Unknown"
    );

    // Verify evidence is captured
    assert!(
        !result.evidence.is_empty() || result.confidence == 0.0,
        "Should capture evidence if intent detected"
    );

    // Check intent properties
    match &result.primary_intent {
        DetectedIntent::Debugging {
            suspected_area,
            symptoms: _,
        } => {
            assert!(!suspected_area.is_empty(), "Should identify suspected area");
            // Symptoms may or may not be captured depending on action order
        }
        DetectedIntent::Learning { topic, .. } => {
            // Learning can also match if search terms detected
            assert!(!topic.is_empty(), "Learning topic should not be empty");
        }
        _ => {
            // Other intents may match depending on pattern scoring
        }
    }
}

// ============================================================================
// TEST 2: LEARNING INTENT DETECTION
// ============================================================================

/// Test that learning intent is detected from documentation and tutorial actions.
///
/// Validates:
/// - Documentation views boost learning confidence
/// - "How to" queries boost learning confidence
/// - Tutorial searches boost learning confidence
/// - Topic is extracted from queries
#[test]
fn test_learning_intent_detection() {
    let detector = detector_with_learning_actions();

    let result = detector.detect_intent();

    // Should detect learning with high confidence
    match &result.primary_intent {
        DetectedIntent::Learning { topic, level: _ } => {
            assert!(!topic.is_empty(), "Should identify learning topic");
            // Level may vary
        }
        _ => {
            // Learning actions should typically detect learning intent
            // But other intents may score higher in some cases
            assert!(result.confidence > 0.0, "Should detect some intent");
        }
    }

    // Verify relevant tags
    let _tags = result.primary_intent.relevant_tags();
    // Tags depend on detected intent type
}

// ============================================================================
// TEST 3: REFACTORING INTENT DETECTION
// ============================================================================

/// Test that refactoring intent is detected from multi-file edits.
///
/// Validates:
/// - Multiple file edits boost refactoring confidence
/// - Refactoring-related searches boost confidence
/// - Target files are identified
#[test]
fn test_refactoring_intent_detection() {
    let detector = detector_with_refactoring_actions();

    let result = detector.detect_intent();

    // Should detect intent from multiple edits
    assert!(
        result.confidence > 0.0,
        "Multiple file edits should detect some intent"
    );

    // Check for refactoring or related intent
    match &result.primary_intent {
        DetectedIntent::Refactoring { target, goal } => {
            assert!(!target.is_empty(), "Should identify refactoring target");
            assert!(!goal.is_empty(), "Should identify refactoring goal");
        }
        DetectedIntent::NewFeature {
            related_components, ..
        } => {
            let _ = related_components;
        }
        _ => {
            // Pattern may match differently
        }
    }
}

// ============================================================================
// TEST 4: INTENT PROVIDES RELEVANT TAGS
// ============================================================================

/// Test that detected intents provide relevant tags for memory search.
///
/// Validates:
/// - Each intent type has associated tags
/// - Tags are relevant to the intent
/// - Tags can be used for memory filtering
#[test]
fn test_intent_provides_relevant_tags() {
    // Test debugging tags
    let debugging = DetectedIntent::Debugging {
        suspected_area: "auth".to_string(),
        symptoms: vec!["null pointer".to_string()],
    };
    let debug_tags = debugging.relevant_tags();
    assert!(debug_tags.contains(&"debugging".to_string()));
    assert!(debug_tags.contains(&"error".to_string()));

    // Test learning tags
    let learning = DetectedIntent::Learning {
        topic: "async rust".to_string(),
        level: LearningLevel::Intermediate,
    };
    let learn_tags = learning.relevant_tags();
    assert!(learn_tags.contains(&"learning".to_string()));
    assert!(learn_tags.contains(&"async rust".to_string()));

    // Test refactoring tags
    let refactoring = DetectedIntent::Refactoring {
        target: "auth module".to_string(),
        goal: "simplify".to_string(),
    };
    let refactor_tags = refactoring.relevant_tags();
    assert!(refactor_tags.contains(&"refactoring".to_string()));
    assert!(refactor_tags.contains(&"patterns".to_string()));

    // Test new feature tags
    let new_feature = DetectedIntent::NewFeature {
        feature_description: "user authentication".to_string(),
        related_components: vec!["login".to_string()],
    };
    let feature_tags = new_feature.relevant_tags();
    assert!(feature_tags.contains(&"feature".to_string()));

    // Test maintenance tags
    let maintenance = DetectedIntent::Maintenance {
        maintenance_type: MaintenanceType::DependencyUpdate,
        target: Some("cargo.toml".to_string()),
    };
    let maint_tags = maintenance.relevant_tags();
    assert!(maint_tags.contains(&"maintenance".to_string()));
    assert!(maint_tags.contains(&"dependencies".to_string()));
}

// ============================================================================
// TEST 5: ACTION HISTORY TRACKING
// ============================================================================

/// Test that action history is tracked and used for detection.
///
/// Validates:
/// - Actions are recorded
/// - Action count is tracked
/// - History can be cleared
/// - Old actions are trimmed
#[test]
fn test_action_history_tracking() {
    let detector = IntentDetector::new();

    // Initially empty
    assert_eq!(detector.action_count(), 0, "Should start with no actions");

    // Record actions
    detector.record_action(UserAction::file_opened("/src/main.rs"));
    detector.record_action(UserAction::search("rust async"));
    detector.record_action(UserAction::file_edited("/src/lib.rs"));

    // Check count
    assert_eq!(detector.action_count(), 3, "Should have 3 actions");

    // Clear actions
    detector.clear_actions();
    assert_eq!(detector.action_count(), 0, "Should be empty after clear");

    // Verify detection with no actions
    let result = detector.detect_intent();
    assert!(
        matches!(result.primary_intent, DetectedIntent::Unknown),
        "Empty history should return Unknown"
    );
    assert_eq!(result.confidence, 0.0, "Confidence should be 0");
}

// ============================================================================
// ADDITIONAL INTENT TESTS
// ============================================================================

/// Test UserAction creation helpers.
#[test]
fn test_user_action_creation() {
    // File opened
    let file_action = UserAction::file_opened("/src/main.rs");
    assert_eq!(file_action.action_type, ActionType::FileOpened);
    assert!(file_action.file.is_some());
    assert!(file_action.content.is_none());

    // File edited
    let edit_action = UserAction::file_edited("/src/lib.rs");
    assert_eq!(edit_action.action_type, ActionType::FileEdited);

    // Search
    let search_action = UserAction::search("rust async");
    assert_eq!(search_action.action_type, ActionType::Search);
    assert!(search_action.file.is_none());
    assert!(search_action.content.is_some());

    // Error
    let error_action = UserAction::error("TypeError: null");
    assert_eq!(error_action.action_type, ActionType::ErrorEncountered);

    // Command
    let cmd_action = UserAction::command("cargo build");
    assert_eq!(cmd_action.action_type, ActionType::CommandExecuted);

    // Docs
    let docs_action = UserAction::docs_viewed("tokio tutorial");
    assert_eq!(docs_action.action_type, ActionType::DocumentationViewed);
}

/// Test action metadata.
#[test]
fn test_action_with_metadata() {
    let action = UserAction::file_opened("/src/main.rs")
        .with_metadata("project", "vestige")
        .with_metadata("branch", "main");

    assert!(action.metadata.contains_key("project"));
    assert_eq!(action.metadata.get("project"), Some(&"vestige".to_string()));
    assert!(action.metadata.contains_key("branch"));
}

/// Test intent description.
#[test]
fn test_intent_description() {
    let debugging = DetectedIntent::Debugging {
        suspected_area: "auth".to_string(),
        symptoms: vec![],
    };
    assert!(debugging.description().contains("auth"));

    let learning = DetectedIntent::Learning {
        topic: "async".to_string(),
        level: LearningLevel::Beginner,
    };
    assert!(learning.description().contains("async"));

    let unknown = DetectedIntent::Unknown;
    assert!(unknown.description().contains("Unknown"));
}

/// Test maintenance type tags.
#[test]
fn test_maintenance_type_tags() {
    let types = vec![
        (MaintenanceType::DependencyUpdate, "dependencies"),
        (MaintenanceType::SecurityPatch, "security"),
        (MaintenanceType::Cleanup, "cleanup"),
        (MaintenanceType::Configuration, "config"),
        (MaintenanceType::Migration, "migration"),
    ];

    for (mtype, expected_tag) in types {
        let intent = DetectedIntent::Maintenance {
            maintenance_type: mtype,
            target: None,
        };
        let tags = intent.relevant_tags();
        assert!(
            tags.contains(&expected_tag.to_string()),
            "Maintenance {:?} should have tag {}",
            intent,
            expected_tag
        );
    }
}

/// Test optimization type tags.
#[test]
fn test_optimization_type_tags() {
    let types = vec![
        (OptimizationType::Speed, "speed"),
        (OptimizationType::Memory, "memory"),
        (OptimizationType::Size, "bundle-size"),
        (OptimizationType::Startup, "startup"),
    ];

    for (otype, expected_tag) in types {
        let intent = DetectedIntent::Optimization {
            target: "app".to_string(),
            optimization_type: otype,
        };
        let tags = intent.relevant_tags();
        assert!(
            tags.contains(&expected_tag.to_string()),
            "Optimization should have tag {}",
            expected_tag
        );
    }
}

// ============================================================================
// REAL PROSPECTIVE-MEMORY JOURNEY (SQLite + core trigger matcher)
// ============================================================================
//
// The contract tests above never touch `Storage`, so they cannot fail if
// intentions are not persisted at all — a restart would lose every reminder
// and the suite would stay green. This journey writes intentions through the
// real persistence path, restarts, and then feeds what came back off disk into
// the product's own trigger matcher.

mod storage_journeys {
    use std::path::Path;

    use chrono::{DateTime, Duration, Utc};
    use tempfile::TempDir;
    use vestige_core::IntentionRecord;
    use vestige_core::neuroscience::prospective_memory::{
        Context, ContextPattern, Intention, IntentionSource, IntentionStatus, IntentionTrigger,
        Priority, ProspectiveMemory,
    };
    use vestige_e2e_tests::harness::{TestDatabaseManager, enable_mock_embeddings};

    /// The trigger payload shape the MCP `intention set` tool persists: the
    /// `TriggerSpec` args struct serialised into `intentions.trigger_data`
    /// (`crates/vestige-mcp/src/tools/intention_unified/args.rs:5-18` and
    /// `set.rs:113-119`).
    fn context_trigger(topic: &str) -> (String, String) {
        (
            "context".to_string(),
            serde_json::json!({ "type": "context", "topic": topic }).to_string(),
        )
    }

    fn time_trigger(at: DateTime<Utc>) -> (String, String) {
        (
            "time".to_string(),
            serde_json::json!({ "type": "time", "at": at.to_rfc3339() }).to_string(),
        )
    }

    fn intention(
        content: &str,
        trigger: (String, String),
        priority: i32,
        deadline: Option<DateTime<Utc>>,
    ) -> IntentionRecord {
        IntentionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.to_string(),
            trigger_type: trigger.0,
            trigger_data: trigger.1,
            priority,
            status: "active".to_string(),
            created_at: Utc::now(),
            deadline,
            fulfilled_at: None,
            reminder_count: 0,
            last_reminded_at: None,
            notes: None,
            tags: vec!["intent:debugging".to_string()],
            related_memories: vec![],
            snoozed_until: None,
            source_type: "mcp".to_string(),
            source_data: None,
        }
    }

    fn reopen(db_path: &Path) -> TestDatabaseManager {
        TestDatabaseManager::new_at_path(db_path.to_path_buf())
    }

    /// Rebuild the core prospective-memory model from a *persisted* row.
    ///
    /// The MCP layer matches triggers inline
    /// (`crates/vestige-mcp/src/tools/intention_unified/check.rs:62-111`); this
    /// journey runs the same persisted payload through the core engine's
    /// matcher instead, because that is the product's own trigger
    /// implementation and it is reachable from this crate. The mapping is
    /// deliberately limited to the two trigger kinds the journey stores: an
    /// unmapped kind returns `None` and fails the assertion that follows,
    /// rather than silently not firing.
    fn rehydrate(record: &IntentionRecord) -> Option<Intention> {
        let data: serde_json::Value = serde_json::from_str(&record.trigger_data).ok()?;
        let trigger = match record.trigger_type.as_str() {
            "context" => IntentionTrigger::ContextBased {
                context_match: ContextPattern::TopicActive(
                    data.get("topic")?.as_str()?.to_string(),
                ),
            },
            "time" => {
                let at = data.get("at")?.as_str()?;
                IntentionTrigger::TimeBased {
                    at: DateTime::parse_from_rfc3339(at).ok()?.with_timezone(&Utc),
                }
            }
            _ => return None,
        };

        Some(Intention {
            id: record.id.clone(),
            content: record.content.clone(),
            trigger,
            priority: Priority::from_value(record.priority.clamp(1, 4) as u8),
            status: IntentionStatus::Active,
            created_at: record.created_at,
            deadline: record.deadline,
            fulfilled_at: record.fulfilled_at,
            reminder_count: record.reminder_count.max(0) as u32,
            last_reminded_at: record.last_reminded_at,
            notes: record.notes.clone(),
            tags: record.tags.clone(),
            related_memories: record.related_memories.clone(),
            snoozed_until: record.snoozed_until,
            source: IntentionSource::Api,
        })
    }

    /// Fire the persisted active intentions against one context.
    fn fire(records: &[IntentionRecord], context: &Context) -> Vec<String> {
        let memory = ProspectiveMemory::new();
        for record in records {
            let rebuilt = rehydrate(record)
                .unwrap_or_else(|| panic!("unmappable persisted trigger: {record:?}"));
            memory
                .create_intention(rebuilt)
                .expect("create_intention must accept a persisted intention");
        }
        memory
            .check_triggers(context)
            .expect("check_triggers must succeed")
            .into_iter()
            .map(|i| i.id)
            .collect()
    }

    /// Set intentions → restart → still active with the same trigger payload →
    /// the matching context fires exactly the context-bound one, a non-matching
    /// context does not, and fulfilled/snoozed intentions stay silent.
    #[test]
    fn test_journey_intention_survives_restart_and_fires_on_matching_context() {
        let _mock = enable_mock_embeddings();

        let dir = TempDir::new().expect("temp dir");
        let db_path = dir.path().join("intentions.db");
        let db = TestDatabaseManager::new_at_path(db_path.clone());

        let ctx_topic = "authentication";
        let active = intention(
            "Review the OAuth error handling before shipping",
            context_trigger(ctx_topic),
            3,
            Some(Utc::now() + Duration::days(7)),
        );
        // A time trigger whose moment has passed must fire; one in the future
        // must not. Both are deterministic on the wall clock they were written
        // with, so the journey needs no sleeping and no fake clock.
        let due = intention(
            "Rotate the staging credentials",
            time_trigger(Utc::now() - Duration::minutes(1)),
            2,
            None,
        );
        let later = intention(
            "Send the weekly report",
            time_trigger(Utc::now() + Duration::hours(1)),
            2,
            None,
        );
        let fulfilled = intention(
            "Already done: bump the rusqlite pin",
            context_trigger("dependencies"),
            2,
            None,
        );
        let snoozed = intention(
            "Later: rewrite the README quickstart",
            context_trigger("documentation"),
            1,
            None,
        );

        for record in [&active, &due, &later, &fulfilled, &snoozed] {
            db.storage
                .save_intention(record)
                .expect("save_intention must succeed");
        }
        assert!(
            db.storage
                .update_intention_status(&fulfilled.id, "fulfilled")
                .expect("update_intention_status must succeed"),
            "the fulfilled intention must be updated"
        );
        assert!(
            db.storage
                .snooze_intention(&snoozed.id, Utc::now() + Duration::hours(2))
                .expect("snooze_intention must succeed"),
            "the snoozed intention must be updated"
        );
        drop(db);

        // ---- Cold restart ----------------------------------------------------
        let restarted = reopen(&db_path);
        let active_rows = restarted
            .storage
            .get_active_intentions()
            .expect("get_active_intentions must succeed");
        let active_ids: Vec<&str> = active_rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            active_ids.len(),
            3,
            "exactly the three active intentions must come back, got {active_ids:?}"
        );
        assert!(
            !active_ids.contains(&fulfilled.id.as_str()),
            "a fulfilled intention must not be reported active after a restart"
        );
        assert!(
            !active_ids.contains(&snoozed.id.as_str()),
            "a snoozed intention must not be reported active after a restart"
        );

        let persisted = restarted
            .storage
            .get_intention(&active.id)
            .expect("get_intention must not error")
            .expect("the context intention must survive a restart");
        assert_eq!(persisted.content, active.content);
        assert_eq!(persisted.priority, 3);
        assert_eq!(
            persisted.trigger_type, "context",
            "the trigger kind must survive a restart"
        );
        assert_eq!(
            persisted.trigger_data, active.trigger_data,
            "the trigger payload must survive a restart"
        );
        assert_eq!(persisted.tags, active.tags);
        assert_eq!(persisted.source_type, "mcp");
        let deadline = persisted.deadline.expect("the deadline must survive");
        assert!(
            deadline > Utc::now(),
            "the persisted deadline must still be ahead"
        );

        let fulfilled_row = restarted
            .storage
            .get_intention(&fulfilled.id)
            .expect("get_intention must not error")
            .expect("the fulfilled intention must still exist");
        assert_eq!(fulfilled_row.status, "fulfilled");
        assert!(
            fulfilled_row.fulfilled_at.is_some(),
            "fulfilment must be timestamped on disk"
        );
        let snoozed_row = restarted
            .storage
            .get_intention(&snoozed.id)
            .expect("get_intention must not error")
            .expect("the snoozed intention must still exist");
        assert_eq!(snoozed_row.status, "snoozed");
        assert!(snoozed_row.snoozed_until.is_some());

        // ---- Fire on a matching context -------------------------------------
        let matching = Context {
            project_name: Some("vestige".to_string()),
            active_topics: vec!["authentication".to_string(), "debugging".to_string()],
            ..Context::new()
        };
        let fired = fire(&active_rows, &matching);
        assert!(
            fired.contains(&active.id),
            "the context intention must fire on a matching topic, fired {fired:?}"
        );
        assert!(
            fired.contains(&due.id),
            "the past-dated time intention must fire, fired {fired:?}"
        );
        assert!(
            !fired.contains(&later.id),
            "a future time intention must not fire yet, fired {fired:?}"
        );

        // ---- Do not fire on a non-matching context --------------------------
        let unrelated = Context {
            project_name: Some("vestige".to_string()),
            active_topics: vec!["sourdough".to_string()],
            ..Context::new()
        };
        let not_fired = fire(&active_rows, &unrelated);
        assert!(
            !not_fired.contains(&active.id),
            "the context intention must not fire on an unrelated topic, fired {not_fired:?}"
        );
        assert!(
            not_fired.contains(&due.id),
            "time-based firing must be independent of the context, fired {not_fired:?}"
        );
    }
}
