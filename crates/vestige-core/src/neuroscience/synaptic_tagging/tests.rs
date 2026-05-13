use super::*;
use chrono::{Duration, Utc};

#[test]
fn test_decay_function_exponential() {
    let decay = DecayFunction::Exponential;

    // At t=0, full strength
    assert!((decay.apply(1.0, 0.0, 12.0) - 1.0).abs() < 0.01);

    // At halfway, significant decay
    let mid = decay.apply(1.0, 6.0, 12.0);
    assert!(mid > 0.0 && mid < 0.5);

    // At lifetime, near zero
    let end = decay.apply(1.0, 12.0, 12.0);
    assert!(end < 0.02);
}

#[test]
fn test_decay_function_linear() {
    let decay = DecayFunction::Linear;

    assert!((decay.apply(1.0, 0.0, 10.0) - 1.0).abs() < 0.01);
    assert!((decay.apply(1.0, 5.0, 10.0) - 0.5).abs() < 0.01);
    assert!((decay.apply(1.0, 10.0, 10.0) - 0.0).abs() < 0.01);
}

#[test]
fn test_synaptic_tag_creation() {
    let tag = SynapticTag::new("mem-123");

    assert_eq!(tag.memory_id, "mem-123");
    assert_eq!(tag.tag_strength, 1.0);
    assert!(!tag.captured);
    assert!(tag.capture_event.is_none());
}

#[test]
fn test_synaptic_tag_capture() {
    let mut tag = SynapticTag::new("mem-123");
    tag.capture("event-456");

    assert!(tag.captured);
    assert_eq!(tag.capture_event.as_deref(), Some("event-456"));
    assert!(tag.captured_at.is_some());
}

#[test]
fn test_capture_window_probability() {
    let window = CaptureWindow::new(9.0, 2.0);
    let event_time = Utc::now();

    // Memory just before event - high probability
    let before = event_time - Duration::hours(1);
    let prob_before = window.capture_probability(before, event_time).unwrap();
    assert!(prob_before > 0.5);

    // Memory long before event - lower probability
    let long_before = event_time - Duration::hours(8);
    let prob_long_before = window.capture_probability(long_before, event_time).unwrap();
    assert!(prob_long_before < prob_before);

    // Memory outside window - None
    let outside = event_time - Duration::hours(10);
    assert!(window.capture_probability(outside, event_time).is_none());
}

#[test]
fn test_importance_event_types() {
    assert!(
        ImportanceEventType::UserFlag.base_strength()
            > ImportanceEventType::TemporalProximity.base_strength()
    );
    assert!(ImportanceEventType::EmotionalContent.capture_radius_multiplier() > 1.0);
    assert!(ImportanceEventType::NoveltySpike.capture_radius_multiplier() < 1.0);
}

#[test]
fn test_synaptic_tagging_system_basic() {
    let mut stc = SynapticTaggingSystem::new();

    // Tag a memory
    let tag = stc.tag_memory("mem-123");
    assert_eq!(tag.memory_id, "mem-123");

    // Check it's active
    assert!(stc.has_active_tag("mem-123"));
    assert!(!stc.is_captured("mem-123"));
}

#[test]
fn test_prp_trigger_captures_tagged_memory() {
    let mut stc = SynapticTaggingSystem::new();

    // Tag a memory
    stc.tag_memory("mem-123");

    // Trigger importance event
    let event = ImportanceEvent::user_flag("mem-456", Some("Remember this!"));
    let result = stc.trigger_prp(event);

    // Should capture the tagged memory
    assert!(result.has_captures());
    assert_eq!(result.captured_memories[0].memory_id, "mem-123");
    assert!(stc.is_captured("mem-123"));
}

#[test]
fn test_weak_event_does_not_trigger_prp() {
    let mut stc = SynapticTaggingSystem::new();
    stc.tag_memory("mem-123");

    // Weak event below threshold
    let event = ImportanceEvent::with_strength(ImportanceEventType::TemporalProximity, 0.3);
    let result = stc.trigger_prp(event);

    assert!(!result.has_captures());
}

#[test]
fn test_clustering() {
    let mut stc = SynapticTaggingSystem::new();

    // Tag multiple memories
    stc.tag_memory("mem-1");
    stc.tag_memory("mem-2");
    stc.tag_memory("mem-3");

    // Trigger event
    let event = ImportanceEvent::user_flag("mem-trigger", None);
    let result = stc.trigger_prp(event);

    // Should create cluster
    assert!(result.cluster.is_some());
    let cluster = result.cluster.unwrap();
    assert!(cluster.size() >= 3);
}

#[test]
fn test_decay_cleans_old_tags() {
    let mut stc = SynapticTaggingSystem::with_config(SynapticTaggingConfig {
        // 0.000003 hours = ~10.8ms (so 100ms sleep should be enough)
        tag_lifetime_hours: 0.000003,
        min_tag_strength: 0.01,
        ..Default::default()
    });

    stc.tag_memory("mem-123");

    // Simulate time passing - sleep much longer than tag lifetime
    std::thread::sleep(std::time::Duration::from_millis(100));

    stc.decay_tags();

    // Check the tag state for debugging
    let tag = stc.get_tag("mem-123");
    let has_active = stc.has_active_tag("mem-123");

    // After sufficient time, tag should be removed OR marked inactive
    // decay_tags removes tags, so get_tag should return None
    assert!(
        tag.is_none() || !has_active,
        "Tag should be cleaned up or inactive. tag={:?}, has_active={}",
        tag.map(|t| t.tag_strength),
        has_active
    );
}

#[test]
fn test_stats_tracking() {
    let mut stc = SynapticTaggingSystem::new();

    stc.tag_memory("mem-1");
    stc.tag_memory("mem-2");

    let event = ImportanceEvent::user_flag("trigger", None);
    let _ = stc.trigger_prp(event);

    let stats = stc.stats();
    assert_eq!(stats.total_tags_created, 2);
    assert_eq!(stats.total_events, 1);
    assert!(stats.total_captures >= 2);
}

#[test]
fn test_captured_memory_direction() {
    let captured = CapturedMemory {
        memory_id: "test".to_string(),
        encoded_at: Utc::now() - Duration::hours(2),
        capture_event_id: "event".to_string(),
        capture_event_type: ImportanceEventType::UserFlag,
        captured_at: Utc::now(),
        capture_probability: 0.8,
        tag_strength_at_capture: 0.9,
        consolidated_importance: 0.85,
        temporal_distance_hours: 2.0,
    };

    assert!(captured.is_backward_capture());
    assert!(!captured.is_forward_capture());
}

#[test]
fn test_importance_cluster_creation() {
    let event = ImportanceEvent::user_flag("trigger", None);
    let captured = vec![
        CapturedMemory {
            memory_id: "mem-1".to_string(),
            encoded_at: Utc::now() - Duration::hours(1),
            capture_event_id: "event".to_string(),
            capture_event_type: ImportanceEventType::UserFlag,
            captured_at: Utc::now(),
            capture_probability: 0.8,
            tag_strength_at_capture: 0.9,
            consolidated_importance: 0.85,
            temporal_distance_hours: 1.0,
        },
        CapturedMemory {
            memory_id: "mem-2".to_string(),
            encoded_at: Utc::now() - Duration::hours(2),
            capture_event_id: "event".to_string(),
            capture_event_type: ImportanceEventType::UserFlag,
            captured_at: Utc::now(),
            capture_probability: 0.7,
            tag_strength_at_capture: 0.8,
            consolidated_importance: 0.75,
            temporal_distance_hours: 2.0,
        },
    ];

    let cluster = ImportanceCluster::new(&event, &captured);

    assert_eq!(cluster.size(), 2);
    assert!(cluster.contains("mem-1"));
    assert!(cluster.contains("mem-2"));
    assert!(!cluster.contains("mem-3"));
    assert!(cluster.average_importance > 0.0);
}

#[test]
fn test_batch_operations() {
    let mut stc = SynapticTaggingSystem::new();

    // Bulk tag
    let tags = stc.tag_memories(&["mem-1", "mem-2", "mem-3"]);
    assert_eq!(tags.len(), 3);

    // Batch trigger
    let events = vec![
        ImportanceEvent::user_flag("trigger-1", None),
        ImportanceEvent::emotional("trigger-2", 0.9),
    ];
    let results = stc.trigger_prp_batch(events);
    assert_eq!(results.len(), 2);
}

#[test]
fn test_get_capture_candidates() {
    let mut stc = SynapticTaggingSystem::new();

    stc.tag_memory("mem-1");
    stc.tag_memory("mem-2");

    let start = Utc::now() - Duration::hours(1);
    let end = Utc::now() + Duration::hours(1);

    let candidates = stc.get_capture_candidates(start, end);
    assert_eq!(candidates.len(), 2);
}
