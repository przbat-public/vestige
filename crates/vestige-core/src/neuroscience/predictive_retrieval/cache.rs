//! Tiny LRU-style cache holding recent prediction batches.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};

use super::types::PredictedMemory;

// ============================================================================
// PREDICTION CACHE (LRU-like)
// ============================================================================

/// Simple LRU-like cache for predictions
#[derive(Debug)]
pub(crate) struct PredictionCache {
    /// Cache entries (key -> (predictions, timestamp))
    entries: HashMap<String, (Vec<PredictedMemory>, DateTime<Utc>)>,
    /// Access order for LRU eviction
    access_order: VecDeque<String>,
    /// Maximum cache size
    max_size: usize,
}

impl PredictionCache {
    pub(crate) fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            access_order: VecDeque::new(),
            max_size,
        }
    }

    pub(crate) fn get(&mut self, key: &str) -> Option<&Vec<PredictedMemory>> {
        if self.entries.contains_key(key) {
            // Move to front of access order
            self.access_order.retain(|k| k != key);
            self.access_order.push_front(key.to_string());
            self.entries.get(key).map(|(v, _)| v)
        } else {
            None
        }
    }

    pub(crate) fn insert(&mut self, key: String, predictions: Vec<PredictedMemory>) {
        // Evict if necessary
        while self.entries.len() >= self.max_size {
            if let Some(old_key) = self.access_order.pop_back() {
                self.entries.remove(&old_key);
            }
        }

        self.entries.insert(key.clone(), (predictions, Utc::now()));
        self.access_order.push_front(key);
    }

    pub(crate) fn invalidate(&mut self, key: &str) {
        self.entries.remove(key);
        self.access_order.retain(|k| k != key);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.access_order.clear();
    }
}
