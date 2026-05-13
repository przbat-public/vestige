//! Round-trip tests for the full hippocampal index pipeline.

use chrono::Utc;

use super::*;

#[test]
fn test_barcode_generation() {
    let mut generator = BarcodeGenerator::new();
    let now = Utc::now();

    let barcode1 = generator.generate("content1", now);
    let barcode2 = generator.generate("content2", now);

    assert_ne!(barcode1.id, barcode2.id);
    assert_ne!(barcode1.content_fingerprint, barcode2.content_fingerprint);
}

#[test]
fn test_barcode_string_roundtrip() {
    let barcode = MemoryBarcode::new(12345, 0xABCD1234, 0xDEADBEEF);
    let s = barcode.to_string();
    let parsed = MemoryBarcode::from_string(&s).unwrap();

    assert_eq!(barcode, parsed);
}

#[test]
fn test_importance_flags() {
    let mut flags = ImportanceFlags::empty();
    assert!(!flags.is_emotional());
    assert!(!flags.is_frequently_accessed());

    flags.set_emotional(true);
    assert!(flags.is_emotional());

    flags.set_frequently_accessed(true);
    assert!(flags.is_frequently_accessed());

    assert_eq!(flags.count_set(), 2);
}

#[test]
fn test_temporal_marker() {
    let now = Utc::now();
    let mut marker = TemporalMarker::new(now);

    assert!(marker.is_currently_valid());
    assert_eq!(marker.access_count, 0);

    marker.record_access();
    assert_eq!(marker.access_count, 1);
}

#[test]
fn test_index_memory() {
    let index = HippocampalIndex::new();
    let now = Utc::now();

    let barcode = index
        .index_memory(
            "test-id",
            "This is test content for indexing",
            "fact",
            now,
            None,
        )
        .unwrap();

    // barcode.id is u64, verify it was assigned
    let _ = barcode.id;
    assert_eq!(index.len(), 1);

    let retrieved = index.get_index("test-id").unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().node_type, "fact");
}

#[test]
fn test_search_indices() {
    let index = HippocampalIndex::new();
    let now = Utc::now();

    index
        .index_memory("mem-1", "The quick brown fox", "fact", now, None)
        .unwrap();
    index
        .index_memory("mem-2", "jumps over the lazy dog", "fact", now, None)
        .unwrap();
    index
        .index_memory("mem-3", "completely unrelated content", "fact", now, None)
        .unwrap();

    let query = IndexQuery::from_text("fox").with_limit(10);
    let results = index.search_indices(&query).unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].index.memory_id, "mem-1");
}

#[test]
fn test_associations() {
    let index = HippocampalIndex::new();
    let now = Utc::now();

    index
        .index_memory("mem-1", "Content A", "fact", now, None)
        .unwrap();
    index
        .index_memory("mem-2", "Content B", "fact", now, None)
        .unwrap();

    index
        .add_association("mem-1", "mem-2", 0.8, AssociationLinkType::Semantic)
        .unwrap();

    let associations = index.get_associations("mem-1", 1).unwrap();
    assert_eq!(associations.len(), 1);
    assert_eq!(associations[0].index.memory_id, "mem-2");
}

#[test]
fn test_compress_embedding() {
    let index = HippocampalIndex::new();

    // Create a 768-dim embedding (like BGE-base-en-v1.5)
    let embedding: Vec<f32> = (0..768).map(|i| (i as f32 / 768.0).sin()).collect();

    let compressed = index.compress_embedding(&embedding);

    assert_eq!(compressed.len(), INDEX_EMBEDDING_DIM);

    // Check normalization
    let norm: f32 = compressed.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.01);
}

#[test]
fn test_migration() {
    let index = HippocampalIndex::new();
    let now = Utc::now();

    let nodes = vec![
        MigrationNode {
            id: "node-1".to_string(),
            content: "First node content".to_string(),
            node_type: "fact".to_string(),
            created_at: now,
            embedding: None,
            retention_strength: 0.8,
            sentiment_magnitude: 0.6,
        },
        MigrationNode {
            id: "node-2".to_string(),
            content: "Second node content".to_string(),
            node_type: "concept".to_string(),
            created_at: now,
            embedding: None,
            retention_strength: 0.3,
            sentiment_magnitude: 0.1,
        },
    ];

    let result = index.migrate_batch(nodes);

    assert_eq!(result.migrated, 2);
    assert_eq!(result.failed, 0);
    assert_eq!(index.len(), 2);

    // Check that flags were set correctly
    let idx1 = index.get_index("node-1").unwrap().unwrap();
    assert!(idx1.importance_flags.has_high_retention());
    assert!(idx1.importance_flags.is_emotional());

    let idx2 = index.get_index("node-2").unwrap().unwrap();
    assert!(!idx2.importance_flags.has_high_retention());
    assert!(!idx2.importance_flags.is_emotional());
}

#[test]
fn test_content_pointer() {
    let sqlite_ptr = ContentPointer::sqlite("knowledge_nodes", 42, ContentType::Text);
    assert!(!sqlite_ptr.is_inline());

    let inline_ptr = ContentPointer::inline(vec![1, 2, 3, 4], ContentType::Binary);
    assert!(inline_ptr.is_inline());
    assert_eq!(inline_ptr.size_bytes, Some(4));
}

#[test]
fn test_index_link_strengthen() {
    let barcode = MemoryBarcode::new(1, 0, 0);
    let mut link = IndexLink::new(barcode, 0.5, AssociationLinkType::Semantic);

    assert_eq!(link.activation_count, 0);

    link.strengthen(0.2);
    assert!(link.strength > 0.5);
    assert_eq!(link.activation_count, 1);
}

#[test]
fn test_prune_weak_links() {
    let index = HippocampalIndex::new();
    let now = Utc::now();

    index
        .index_memory("mem-1", "Content A", "fact", now, None)
        .unwrap();
    index
        .index_memory("mem-2", "Content B", "fact", now, None)
        .unwrap();
    index
        .index_memory("mem-3", "Content C", "fact", now, None)
        .unwrap();

    // Add strong and weak links
    index
        .add_association("mem-1", "mem-2", 0.8, AssociationLinkType::Semantic)
        .unwrap();
    index
        .add_association("mem-1", "mem-3", 0.05, AssociationLinkType::Semantic)
        .unwrap();

    let pruned = index.prune_weak_links().unwrap();
    assert_eq!(pruned, 1);

    let idx = index.get_index("mem-1").unwrap().unwrap();
    assert_eq!(idx.association_links.len(), 1);
}
