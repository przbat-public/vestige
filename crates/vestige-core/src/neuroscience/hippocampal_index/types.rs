//! Type vocabulary used by the hippocampal index — errors, barcodes, content
//! pointers and link metadata.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Errors for hippocampal index operations
#[derive(Debug, Clone)]
pub enum HippocampalIndexError {
    /// Memory not found in index
    NotFound(String),
    /// Content retrieval failed
    ContentRetrievalFailed(String),
    /// Invalid barcode
    InvalidBarcode(String),
    /// Storage error
    StorageError(String),
    /// Index corruption detected
    IndexCorruption(String),
    /// Lock acquisition failed
    LockError(String),
    /// Migration error
    MigrationError(String),
    /// Embedding error
    EmbeddingError(String),
}

impl std::fmt::Display for HippocampalIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HippocampalIndexError::NotFound(id) => write!(f, "Memory not found: {}", id),
            HippocampalIndexError::ContentRetrievalFailed(e) => {
                write!(f, "Content retrieval failed: {}", e)
            }
            HippocampalIndexError::InvalidBarcode(e) => write!(f, "Invalid barcode: {}", e),
            HippocampalIndexError::StorageError(e) => write!(f, "Storage error: {}", e),
            HippocampalIndexError::IndexCorruption(e) => write!(f, "Index corruption: {}", e),
            HippocampalIndexError::LockError(e) => write!(f, "Lock error: {}", e),
            HippocampalIndexError::MigrationError(e) => write!(f, "Migration error: {}", e),
            HippocampalIndexError::EmbeddingError(e) => write!(f, "Embedding error: {}", e),
        }
    }
}

impl std::error::Error for HippocampalIndexError {}

pub type Result<T> = std::result::Result<T, HippocampalIndexError>;

// ============================================================================
// MEMORY BARCODE
// ============================================================================

/// Unique barcode for each memory (inspired by chickadee hippocampus)
///
/// The barcode provides:
/// - Unique identification across the entire memory system
/// - Temporal information (when created)
/// - Content fingerprint (what it represents)
///
/// This is analogous to how hippocampal neurons create sparse,
/// orthogonal patterns for different memories.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct MemoryBarcode {
    /// Sequential unique identifier
    pub id: u64,
    /// Hash of creation timestamp (temporal signature)
    pub creation_hash: u32,
    /// Hash of content (content fingerprint)
    pub content_fingerprint: u32,
}

impl MemoryBarcode {
    /// Create a new barcode
    pub fn new(id: u64, creation_hash: u32, content_fingerprint: u32) -> Self {
        Self {
            id,
            creation_hash,
            content_fingerprint,
        }
    }

    /// Convert to a compact string representation
    pub fn to_compact_string(&self) -> String {
        format!(
            "{:016x}-{:08x}-{:08x}",
            self.id, self.creation_hash, self.content_fingerprint
        )
    }

    /// Parse from string representation
    pub fn from_string(s: &str) -> std::result::Result<Self, HippocampalIndexError> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 {
            return Err(HippocampalIndexError::InvalidBarcode(
                "Expected 3 parts separated by '-'".to_string(),
            ));
        }

        let id = u64::from_str_radix(parts[0], 16)
            .map_err(|e| HippocampalIndexError::InvalidBarcode(format!("Invalid id: {}", e)))?;
        let creation_hash = u32::from_str_radix(parts[1], 16).map_err(|e| {
            HippocampalIndexError::InvalidBarcode(format!("Invalid creation_hash: {}", e))
        })?;
        let content_fingerprint = u32::from_str_radix(parts[2], 16).map_err(|e| {
            HippocampalIndexError::InvalidBarcode(format!("Invalid content_fingerprint: {}", e))
        })?;

        Ok(Self {
            id,
            creation_hash,
            content_fingerprint,
        })
    }

    /// Check if two barcodes have the same content (ignoring temporal info)
    pub fn same_content(&self, other: &Self) -> bool {
        self.content_fingerprint == other.content_fingerprint
    }

    /// Check if created around the same time (within hash collision probability)
    pub fn similar_time(&self, other: &Self) -> bool {
        self.creation_hash == other.creation_hash
    }
}

impl std::fmt::Display for MemoryBarcode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:016x}-{:08x}-{:08x}",
            self.id, self.creation_hash, self.content_fingerprint
        )
    }
}

// ============================================================================
// BARCODE GENERATOR
// ============================================================================

/// Generator for unique memory barcodes
///
/// Creates barcodes that encode:
/// - Sequential ID (uniqueness)
/// - Temporal signature (when)
/// - Content fingerprint (what)
pub struct BarcodeGenerator {
    /// Next sequential ID
    next_id: u64,
    /// Salt for hashing (instance-specific)
    hash_salt: u64,
}

impl BarcodeGenerator {
    /// Create a new barcode generator
    pub fn new() -> Self {
        Self {
            next_id: 0,
            hash_salt: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
        }
    }

    /// Create a generator starting from a specific ID
    pub fn with_starting_id(starting_id: u64) -> Self {
        Self {
            next_id: starting_id,
            hash_salt: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
        }
    }

    /// Generate a unique barcode for new memory
    pub fn generate(&mut self, content: &str, timestamp: DateTime<Utc>) -> MemoryBarcode {
        let id = self.next_id;
        self.next_id += 1;

        let creation_hash = self.hash_timestamp(timestamp);
        let content_fingerprint = self.hash_content(content);

        MemoryBarcode::new(id, creation_hash, content_fingerprint)
    }

    /// Generate barcode for existing memory with known ID
    pub fn generate_with_id(
        &self,
        id: u64,
        content: &str,
        timestamp: DateTime<Utc>,
    ) -> MemoryBarcode {
        let creation_hash = self.hash_timestamp(timestamp);
        let content_fingerprint = self.hash_content(content);

        MemoryBarcode::new(id, creation_hash, content_fingerprint)
    }

    /// Hash timestamp to 32-bit signature
    fn hash_timestamp(&self, timestamp: DateTime<Utc>) -> u32 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        timestamp
            .timestamp_nanos_opt()
            .unwrap_or(0)
            .hash(&mut hasher);
        self.hash_salt.hash(&mut hasher);
        (hasher.finish() & 0xFFFFFFFF) as u32
    }

    /// Hash content to 32-bit fingerprint
    fn hash_content(&self, content: &str) -> u32 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        (hasher.finish() & 0xFFFFFFFF) as u32
    }

    /// Get the current ID counter (for persistence)
    pub fn current_id(&self) -> u64 {
        self.next_id
    }
}

impl Default for BarcodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TEMPORAL MARKER
// ============================================================================

/// Temporal information for a memory index
///
/// Encodes when the memory was created and when it's valid,
/// enabling temporal queries without accessing full content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalMarker {
    /// When the memory was created
    pub created_at: DateTime<Utc>,
    /// When the memory was last accessed
    pub last_accessed: DateTime<Utc>,
    /// When the memory becomes valid (optional)
    pub valid_from: Option<DateTime<Utc>>,
    /// When the memory expires (optional)
    pub valid_until: Option<DateTime<Utc>>,
    /// Access count for frequency-based retrieval
    pub access_count: u32,
}

impl TemporalMarker {
    /// Create a new temporal marker
    pub fn new(created_at: DateTime<Utc>) -> Self {
        Self {
            created_at,
            last_accessed: created_at,
            valid_from: None,
            valid_until: None,
            access_count: 0,
        }
    }

    /// Check if valid at a specific time
    pub fn is_valid_at(&self, time: DateTime<Utc>) -> bool {
        let after_start = self.valid_from.map(|t| time >= t).unwrap_or(true);
        let before_end = self.valid_until.map(|t| time <= t).unwrap_or(true);
        after_start && before_end
    }

    /// Check if currently valid
    pub fn is_currently_valid(&self) -> bool {
        self.is_valid_at(Utc::now())
    }

    /// Record an access
    pub fn record_access(&mut self) {
        self.last_accessed = Utc::now();
        self.access_count = self.access_count.saturating_add(1);
    }

    /// Get age in days since creation
    pub fn age_days(&self) -> f64 {
        (Utc::now() - self.created_at).num_seconds() as f64 / 86400.0
    }

    /// Get recency (days since last access)
    pub fn recency_days(&self) -> f64 {
        (Utc::now() - self.last_accessed).num_seconds() as f64 / 86400.0
    }
}

// ============================================================================
// IMPORTANCE FLAGS
// ============================================================================

/// Importance flags for a memory (compact, bit-packed)
///
/// These flags enable fast filtering without content access.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportanceFlags {
    bits: u32,
}

impl ImportanceFlags {
    // Flag bit positions
    const EMOTIONAL: u32 = 1 << 0;
    const FREQUENTLY_ACCESSED: u32 = 1 << 1;
    const RECENTLY_CREATED: u32 = 1 << 2;
    const HAS_ASSOCIATIONS: u32 = 1 << 3;
    const USER_STARRED: u32 = 1 << 4;
    const HIGH_RETENTION: u32 = 1 << 5;
    const CONSOLIDATED: u32 = 1 << 6;
    const COMPRESSED: u32 = 1 << 7;

    /// Create empty flags
    pub fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Create with all flags set
    pub fn all() -> Self {
        Self {
            bits: Self::EMOTIONAL
                | Self::FREQUENTLY_ACCESSED
                | Self::RECENTLY_CREATED
                | Self::HAS_ASSOCIATIONS
                | Self::USER_STARRED
                | Self::HIGH_RETENTION
                | Self::CONSOLIDATED
                | Self::COMPRESSED,
        }
    }

    /// Set emotional flag
    pub fn set_emotional(&mut self, value: bool) {
        if value {
            self.bits |= Self::EMOTIONAL;
        } else {
            self.bits &= !Self::EMOTIONAL;
        }
    }

    /// Check emotional flag
    pub fn is_emotional(&self) -> bool {
        self.bits & Self::EMOTIONAL != 0
    }

    /// Set frequently accessed flag
    pub fn set_frequently_accessed(&mut self, value: bool) {
        if value {
            self.bits |= Self::FREQUENTLY_ACCESSED;
        } else {
            self.bits &= !Self::FREQUENTLY_ACCESSED;
        }
    }

    /// Check frequently accessed flag
    pub fn is_frequently_accessed(&self) -> bool {
        self.bits & Self::FREQUENTLY_ACCESSED != 0
    }

    /// Set recently created flag
    pub fn set_recently_created(&mut self, value: bool) {
        if value {
            self.bits |= Self::RECENTLY_CREATED;
        } else {
            self.bits &= !Self::RECENTLY_CREATED;
        }
    }

    /// Check recently created flag
    pub fn is_recently_created(&self) -> bool {
        self.bits & Self::RECENTLY_CREATED != 0
    }

    /// Set has associations flag
    pub fn set_has_associations(&mut self, value: bool) {
        if value {
            self.bits |= Self::HAS_ASSOCIATIONS;
        } else {
            self.bits &= !Self::HAS_ASSOCIATIONS;
        }
    }

    /// Check has associations flag
    pub fn has_associations(&self) -> bool {
        self.bits & Self::HAS_ASSOCIATIONS != 0
    }

    /// Set user starred flag
    pub fn set_user_starred(&mut self, value: bool) {
        if value {
            self.bits |= Self::USER_STARRED;
        } else {
            self.bits &= !Self::USER_STARRED;
        }
    }

    /// Check user starred flag
    pub fn is_user_starred(&self) -> bool {
        self.bits & Self::USER_STARRED != 0
    }

    /// Set high retention flag
    pub fn set_high_retention(&mut self, value: bool) {
        if value {
            self.bits |= Self::HIGH_RETENTION;
        } else {
            self.bits &= !Self::HIGH_RETENTION;
        }
    }

    /// Check high retention flag
    pub fn has_high_retention(&self) -> bool {
        self.bits & Self::HIGH_RETENTION != 0
    }

    /// Set consolidated flag
    pub fn set_consolidated(&mut self, value: bool) {
        if value {
            self.bits |= Self::CONSOLIDATED;
        } else {
            self.bits &= !Self::CONSOLIDATED;
        }
    }

    /// Check consolidated flag
    pub fn is_consolidated(&self) -> bool {
        self.bits & Self::CONSOLIDATED != 0
    }

    /// Set compressed flag
    pub fn set_compressed(&mut self, value: bool) {
        if value {
            self.bits |= Self::COMPRESSED;
        } else {
            self.bits &= !Self::COMPRESSED;
        }
    }

    /// Check compressed flag
    pub fn is_compressed(&self) -> bool {
        self.bits & Self::COMPRESSED != 0
    }

    /// Get raw bits (for persistence)
    pub fn to_bits(&self) -> u32 {
        self.bits
    }

    /// Create from raw bits
    pub fn from_bits(bits: u32) -> Self {
        Self { bits }
    }

    /// Count number of flags set
    pub fn count_set(&self) -> u32 {
        self.bits.count_ones()
    }
}

impl Default for ImportanceFlags {
    fn default() -> Self {
        Self::empty()
    }
}

// ============================================================================
// CONTENT TYPES AND STORAGE LOCATIONS
// ============================================================================

/// Type of content stored
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContentType {
    /// Plain text content
    Text,
    /// Source code
    Code,
    /// Structured data (JSON, etc.)
    StructuredData,
    /// Embedding vector
    Embedding,
    /// Metadata only
    Metadata,
    /// Binary data
    Binary,
    /// Reference to external resource
    ExternalReference,
}

/// Location where content is stored
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageLocation {
    /// SQLite database
    SQLite {
        /// Table name
        table: String,
        /// Row ID
        row_id: i64,
    },
    /// Vector store
    VectorStore {
        /// Index name
        index: String,
        /// Vector ID
        id: u64,
    },
    /// File system
    FileSystem {
        /// File path
        path: PathBuf,
    },
    /// Inline (stored directly in the pointer)
    Inline {
        /// Raw data
        data: Vec<u8>,
    },
    /// Content was compressed/archived
    Archived {
        /// Archive identifier
        archive_id: String,
        /// Offset in archive
        offset: u64,
    },
}

// ============================================================================
// CONTENT POINTER
// ============================================================================

/// Pointer to actual content in distributed storage
///
/// This is the "neocortical" reference - pointing to where
/// the actual memory content lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPointer {
    /// Type of content at this location
    pub content_type: ContentType,
    /// Where the content is stored
    pub storage_location: StorageLocation,
    /// Byte range within the content (for chunked storage)
    pub chunk_range: Option<(usize, usize)>,
    /// Size in bytes (for pre-allocation)
    pub size_bytes: Option<usize>,
    /// Content hash for integrity verification
    pub content_hash: Option<u64>,
}

impl ContentPointer {
    /// Create a pointer to SQLite storage
    pub fn sqlite(table: &str, row_id: i64, content_type: ContentType) -> Self {
        Self {
            content_type,
            storage_location: StorageLocation::SQLite {
                table: table.to_string(),
                row_id,
            },
            chunk_range: None,
            size_bytes: None,
            content_hash: None,
        }
    }

    /// Create a pointer to vector store
    pub fn vector_store(index: &str, id: u64) -> Self {
        Self {
            content_type: ContentType::Embedding,
            storage_location: StorageLocation::VectorStore {
                index: index.to_string(),
                id,
            },
            chunk_range: None,
            size_bytes: None,
            content_hash: None,
        }
    }

    /// Create a pointer to file system
    pub fn file_system(path: PathBuf, content_type: ContentType) -> Self {
        Self {
            content_type,
            storage_location: StorageLocation::FileSystem { path },
            chunk_range: None,
            size_bytes: None,
            content_hash: None,
        }
    }

    /// Create an inline pointer (for small data)
    pub fn inline(data: Vec<u8>, content_type: ContentType) -> Self {
        let size = data.len();
        Self {
            content_type,
            storage_location: StorageLocation::Inline { data },
            chunk_range: None,
            size_bytes: Some(size),
            content_hash: None,
        }
    }

    /// Set chunk range
    pub fn with_chunk_range(mut self, start: usize, end: usize) -> Self {
        self.chunk_range = Some((start, end));
        self
    }

    /// Set size
    pub fn with_size(mut self, size: usize) -> Self {
        self.size_bytes = Some(size);
        self
    }

    /// Set content hash
    pub fn with_hash(mut self, hash: u64) -> Self {
        self.content_hash = Some(hash);
        self
    }

    /// Check if this is inline storage
    pub fn is_inline(&self) -> bool {
        matches!(self.storage_location, StorageLocation::Inline { .. })
    }
}

// ============================================================================
// INDEX LINK
// ============================================================================

/// Link between memory indices (associations)
///
/// These links form the "web" of memory associations,
/// enabling pattern completion and spreading activation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexLink {
    /// Target barcode
    pub target_barcode: MemoryBarcode,
    /// Link strength (0.0 to 1.0)
    pub strength: f32,
    /// Type of association
    pub link_type: AssociationLinkType,
    /// When the link was created
    pub created_at: DateTime<Utc>,
    /// Number of times the link was activated
    pub activation_count: u32,
}

/// Type of association between memories (in hippocampal index)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AssociationLinkType {
    /// Temporal co-occurrence
    Temporal,
    /// Semantic similarity
    Semantic,
    /// Causal relationship
    Causal,
    /// Part-of relationship
    PartOf,
    /// User-defined association
    UserDefined,
    /// Derived from same source
    SameSource,
}

impl IndexLink {
    /// Create a new link
    pub fn new(target: MemoryBarcode, strength: f32, link_type: AssociationLinkType) -> Self {
        Self {
            target_barcode: target,
            strength: strength.clamp(0.0, 1.0),
            link_type,
            created_at: Utc::now(),
            activation_count: 0,
        }
    }

    /// Strengthen the link (Hebbian learning)
    pub fn strengthen(&mut self, amount: f32) {
        self.strength = (self.strength + amount).clamp(0.0, 1.0);
        self.activation_count = self.activation_count.saturating_add(1);
    }

    /// Decay the link strength
    pub fn decay(&mut self, factor: f32) {
        self.strength *= factor.clamp(0.0, 1.0);
    }
}
