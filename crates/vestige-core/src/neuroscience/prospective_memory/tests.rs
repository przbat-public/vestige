use super::*;
use chrono::{Duration, Utc};

#[test]
fn test_priority_ordering() {
    assert!(Priority::Critical > Priority::High);
    assert!(Priority::High > Priority::Normal);
    assert!(Priority::Normal > Priority::Low);
}

#[test]
fn test_priority_escalation() {
    assert_eq!(Priority::Low.escalate(), Priority::Normal);
    assert_eq!(Priority::Normal.escalate(), Priority::High);
    assert_eq!(Priority::High.escalate(), Priority::Critical);
    assert_eq!(Priority::Critical.escalate(), Priority::Critical);
}

#[test]
fn test_trigger_pattern_matches() {
    let pattern = TriggerPattern::contains("john");
    assert!(pattern.matches("Meeting with John"));
    assert!(pattern.matches("john's project"));
    assert!(!pattern.matches("Meeting with Jane"));
}

#[test]
fn test_trigger_pattern_any_of() {
    let pattern = TriggerPattern::AnyOf(vec![
        TriggerPattern::contains("john"),
        TriggerPattern::contains("jane"),
    ]);

    assert!(pattern.matches("Meeting with John"));
    assert!(pattern.matches("Meeting with Jane"));
    assert!(!pattern.matches("Meeting with Bob"));
}

#[test]
fn test_context_pattern_matches() {
    let context = Context::new()
        .with_project("payments-service", "/code/payments")
        .with_file("/code/payments/src/auth.rs")
        .with_topic("authentication");

    let pattern = ContextPattern::in_codebase("payments");
    assert!(pattern.matches(&context));

    let pattern = ContextPattern::topic_active("auth");
    assert!(pattern.matches(&context));
}

#[test]
fn test_intention_creation() {
    let intention = Intention::new(
        "Review code",
        IntentionTrigger::after_duration(Duration::hours(1)),
    )
    .with_priority(Priority::High)
    .with_tags(vec!["code-review".to_string()]);

    assert_eq!(intention.priority, Priority::High);
    assert!(!intention.tags.is_empty());
    assert_eq!(intention.status, IntentionStatus::Active);
}

#[test]
fn test_time_trigger() {
    let trigger = IntentionTrigger::at_time(Utc::now() - Duration::hours(1));
    let context = Context::new();

    assert!(trigger.is_triggered(&context, &[]));
}

#[test]
fn test_duration_trigger() {
    let trigger = IntentionTrigger::after_duration(Duration::seconds(-1));
    let context = Context::new();

    assert!(trigger.is_triggered(&context, &[]));
}

#[test]
fn test_event_trigger() {
    let trigger = IntentionTrigger::on_event("Meeting with John", TriggerPattern::contains("john"));
    let context = Context::new();

    assert!(!trigger.is_triggered(&context, &[]));
    assert!(trigger.is_triggered(&context, &["Scheduled meeting with John".to_string()]));
}

#[test]
fn test_prospective_memory_create() {
    let pm = ProspectiveMemory::new();

    let intention = Intention::new(
        "Test intention",
        IntentionTrigger::after_duration(Duration::hours(1)),
    );

    let id = pm.create_intention(intention).unwrap();
    assert!(!id.is_empty());

    let retrieved = pm.get_intention(&id).unwrap();
    assert!(retrieved.is_some());
}

#[test]
fn test_parse_natural_language() {
    let parser = IntentionParser::new();

    // Test "remind me to X in Y" pattern
    let result = parser.parse("remind me to check email in 30 minutes");
    assert!(result.is_ok());

    // Test "when" pattern
    let result = parser.parse("remind me to ask about API when meeting with John");
    assert!(result.is_ok());

    // Test implicit intention
    let result = parser.parse("I should tell Sarah about the bug");
    assert!(result.is_ok());
}

#[test]
fn test_intention_snooze() {
    let pm = ProspectiveMemory::new();

    let intention = Intention::new(
        "Test",
        IntentionTrigger::after_duration(Duration::seconds(-1)),
    );

    let id = pm.create_intention(intention).unwrap();

    pm.snooze(&id, Duration::hours(1)).unwrap();

    let intention = pm.get_intention(&id).unwrap().unwrap();
    assert_eq!(intention.status, IntentionStatus::Snoozed);
}

#[test]
fn test_intention_fulfill() {
    let pm = ProspectiveMemory::new();

    let intention = Intention::new("Test", IntentionTrigger::after_duration(Duration::hours(1)));

    let id = pm.create_intention(intention).unwrap();

    pm.fulfill(&id).unwrap();

    let intention = pm.get_intention(&id).unwrap().unwrap();
    assert_eq!(intention.status, IntentionStatus::Fulfilled);
}

#[test]
fn test_recurrence_pattern() {
    let now = Utc::now();

    let pattern = RecurrencePattern::EveryHours(2);
    let next = pattern.next_occurrence(now);
    assert!(next > now);
    assert!((next - now) == Duration::hours(2));
}

#[test]
fn test_stats() {
    let pm = ProspectiveMemory::new();

    // Create some intentions
    for i in 0..5 {
        let intention = Intention::new(
            format!("Intention {}", i),
            IntentionTrigger::after_duration(Duration::hours(1)),
        );
        pm.create_intention(intention).unwrap();
    }

    let stats = pm.stats().unwrap();
    assert_eq!(stats.total_active, 5);
}
