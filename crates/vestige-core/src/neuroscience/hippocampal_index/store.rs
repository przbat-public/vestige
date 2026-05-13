//! Pluggable storage backend that holds the actual memory content (the
//! "neocortical" half — files, blobs, S3, etc.).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use super::types::{ContentPointer, HippocampalIndexError, Result, StorageLocation};

// ============================================================================
// CONTENT STORE
// ============================================================================

/// Abstract content storage backend
///
/// This represents the "neocortex" - the distributed storage
/// where actual memory content lives.
pub struct ContentStore {
    /// SQLite connection (if available)
    sqlite_path: Option<PathBuf>,
    /// File storage root
    file_root: Option<PathBuf>,
    /// In-memory cache for recently accessed content
    cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    /// Maximum cache size in bytes
    max_cache_size: usize,
    /// Current cache size
    current_cache_size: Arc<RwLock<usize>>,
}

impl ContentStore {
    /// Create a new content store
    pub fn new() -> Self {
        Self {
            sqlite_path: None,
            file_root: None,
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_cache_size: 10 * 1024 * 1024, // 10 MB default
            current_cache_size: Arc::new(RwLock::new(0)),
        }
    }

    /// Configure SQLite backend
    pub fn with_sqlite(mut self, path: PathBuf) -> Self {
        self.sqlite_path = Some(path);
        self
    }

    /// Configure file storage backend
    pub fn with_file_root(mut self, path: PathBuf) -> Self {
        self.file_root = Some(path);
        self
    }

    /// Set maximum cache size
    pub fn with_max_cache(mut self, size_bytes: usize) -> Self {
        self.max_cache_size = size_bytes;
        self
    }

    /// Retrieve content from a pointer
    pub fn retrieve(&self, pointer: &ContentPointer) -> Result<Vec<u8>> {
        // Check cache first
        let cache_key = self.cache_key(pointer);
        if let Ok(cache) = self.cache.read()
            && let Some(data) = cache.get(&cache_key)
        {
            return Ok(data.clone());
        }

        // Retrieve from storage
        let data = match &pointer.storage_location {
            StorageLocation::Inline { data } => data.clone(),
            StorageLocation::SQLite { table, row_id } => {
                self.retrieve_from_sqlite(table, *row_id)?
            }
            StorageLocation::FileSystem { path } => self.retrieve_from_file(path)?,
            StorageLocation::VectorStore { index, id } => {
                self.retrieve_from_vector_store(index, *id)?
            }
            StorageLocation::Archived { archive_id, offset } => {
                self.retrieve_from_archive(archive_id, *offset)?
            }
        };

        // Apply chunk range if specified
        let data = if let Some((start, end)) = pointer.chunk_range {
            data.get(start..end).unwrap_or(&data).to_vec()
        } else {
            data
        };

        // Update cache
        self.cache_content(&cache_key, &data);

        Ok(data)
    }

    /// Generate cache key for a pointer
    fn cache_key(&self, pointer: &ContentPointer) -> String {
        match &pointer.storage_location {
            StorageLocation::Inline { .. } => "inline".to_string(),
            StorageLocation::SQLite { table, row_id } => format!("sqlite:{}:{}", table, row_id),
            StorageLocation::FileSystem { path } => format!("file:{}", path.display()),
            StorageLocation::VectorStore { index, id } => format!("vector:{}:{}", index, id),
            StorageLocation::Archived { archive_id, offset } => {
                format!("archive:{}:{}", archive_id, offset)
            }
        }
    }

    /// Add content to cache
    fn cache_content(&self, key: &str, data: &[u8]) {
        let data_size = data.len();

        // Don't cache if too large
        if data_size > self.max_cache_size / 4 {
            return;
        }

        if let Ok(mut cache) = self.cache.write()
            && let Ok(mut size) = self.current_cache_size.write()
        {
            // Evict if necessary
            while *size + data_size > self.max_cache_size && !cache.is_empty() {
                // Simple eviction: remove first entry
                if let Some(key_to_remove) = cache.keys().next().cloned() {
                    if let Some(removed) = cache.remove(&key_to_remove) {
                        *size = size.saturating_sub(removed.len());
                    }
                } else {
                    break;
                }
            }

            cache.insert(key.to_string(), data.to_vec());
            *size += data_size;
        }
    }

    /// Retrieve from SQLite (placeholder - to be integrated with Storage)
    fn retrieve_from_sqlite(&self, table: &str, row_id: i64) -> Result<Vec<u8>> {
        // This would connect to SQLite and retrieve the content
        // For now, return an error indicating it needs integration
        Err(HippocampalIndexError::ContentRetrievalFailed(format!(
            "SQLite retrieval not yet integrated: {}:{}",
            table, row_id
        )))
    }

    /// Retrieve from file system
    fn retrieve_from_file(&self, path: &PathBuf) -> Result<Vec<u8>> {
        std::fs::read(path).map_err(|e| {
            HippocampalIndexError::ContentRetrievalFailed(format!(
                "File read failed for {}: {}",
                path.display(),
                e
            ))
        })
    }

    /// Retrieve from vector store (placeholder)
    fn retrieve_from_vector_store(&self, index: &str, id: u64) -> Result<Vec<u8>> {
        Err(HippocampalIndexError::ContentRetrievalFailed(format!(
            "Vector store retrieval not yet integrated: {}:{}",
            index, id
        )))
    }

    /// Retrieve from archive (placeholder)
    fn retrieve_from_archive(&self, archive_id: &str, offset: u64) -> Result<Vec<u8>> {
        Err(HippocampalIndexError::ContentRetrievalFailed(format!(
            "Archive retrieval not yet implemented: {}:{}",
            archive_id, offset
        )))
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
        if let Ok(mut size) = self.current_cache_size.write() {
            *size = 0;
        }
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        let entries = self.cache.read().map(|c| c.len()).unwrap_or(0);
        let size = self.current_cache_size.read().map(|s| *s).unwrap_or(0);
        (entries, size)
    }
}

impl Default for ContentStore {
    fn default() -> Self {
        Self::new()
    }
}
