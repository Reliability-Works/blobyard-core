use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::Read;

/// Stable storage failure classes shared by every adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageError {
    /// The requested object or multipart upload does not exist.
    NotFound,
    /// The target already exists or multipart state conflicts.
    Conflict,
    /// A key, range, part, or expected checksum was invalid.
    InvalidInput,
    /// Stored bytes did not match required integrity metadata.
    IntegrityMismatch,
    /// The storage provider failed.
    Unavailable,
}

impl Display for StorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "stored object not found",
            Self::Conflict => "storage conflict",
            Self::InvalidInput => "invalid storage input",
            Self::IntegrityMismatch => "stored object integrity mismatch",
            Self::Unavailable => "storage provider unavailable",
        })
    }
}

impl Error for StorageError {}

/// A provider-independent object key that cannot escape an adapter root.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageKey(String);

impl StorageKey {
    /// Validates a relative object key.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidInput`] for empty, absolute, dot-segment, oversized, or
    /// control-character-bearing keys.
    pub fn new(value: impl Into<String>) -> Result<Self, StorageError> {
        let value = value.into();
        let valid = crate::is_valid_relative_path(&value, 512);
        if valid {
            Ok(Self(value))
        } else {
            Err(StorageError::InvalidInput)
        }
    }

    /// Returns the validated key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A lowercase SHA-256 checksum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectChecksum(String);

impl ObjectChecksum {
    /// Validates a lowercase SHA-256 checksum.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidInput`] unless the value is exactly 64 lowercase hex bytes.
    pub fn new(value: impl Into<String>) -> Result<Self, StorageError> {
        let value = value.into();
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value))
        } else {
            Err(StorageError::InvalidInput)
        }
    }

    /// Builds a checksum from one exact SHA-256 digest.
    #[must_use]
    pub fn from_sha256_digest(digest: [u8; 32]) -> Self {
        Self(blobyard_core::hex_digest(&digest))
    }

    /// Returns the validated checksum.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A half-open byte range. Empty ranges represent complete zero-byte objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    /// Inclusive first byte.
    pub start: u64,
    /// Exclusive final byte.
    pub end: u64,
}

impl ByteRange {
    /// Validates a non-inverted half-open range.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidInput`] when `start > end`.
    pub const fn new(start: u64, end: u64) -> Result<Self, StorageError> {
        if start <= end {
            Ok(Self { start, end })
        } else {
            Err(StorageError::InvalidInput)
        }
    }
}

/// Integrity metadata for one committed object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageMetadata {
    /// Stored byte size.
    pub size: u64,
    /// SHA-256 of the complete object.
    pub checksum: ObjectChecksum,
}

/// A bounded storage read plus complete-object integrity metadata.
pub struct StorageRead {
    /// Reader limited to the requested range.
    pub reader: Box<dyn Read + Send>,
    /// Complete-object metadata.
    pub metadata: StorageMetadata,
    /// Actual returned half-open range.
    pub range: ByteRange,
}

/// Opaque multipart upload identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultipartId(pub String);

/// Integrity metadata for one staged multipart part.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultipartPart {
    /// One-based part number.
    pub number: u32,
    /// Part byte size.
    pub size: u64,
    /// SHA-256 of the part bytes.
    pub checksum: ObjectChecksum,
    /// Opaque storage-provider tag required to complete this part, when applicable.
    pub provider_tag: Option<String>,
}

/// Provider-independent byte storage used by Blob Yard services.
pub trait ObjectStorage: Send + Sync {
    /// Stores a complete object atomically and returns computed integrity metadata.
    ///
    /// # Errors
    ///
    /// Returns a stable validation, conflict, integrity, or provider failure.
    fn put(
        &self,
        key: &StorageKey,
        source: &mut dyn Read,
        expected: Option<&ObjectChecksum>,
    ) -> Result<StorageMetadata, StorageError>;

    /// Opens a complete object or a validated byte range.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, integrity, or provider failure.
    fn get(&self, key: &StorageKey, range: Option<ByteRange>) -> Result<StorageRead, StorageError>;

    /// Reads complete-object integrity metadata without opening bytes.
    ///
    /// # Errors
    ///
    /// Returns not-found, integrity, or provider failure.
    fn head(&self, key: &StorageKey) -> Result<StorageMetadata, StorageError>;

    /// Deletes an object. Missing objects are reported explicitly.
    ///
    /// # Errors
    ///
    /// Returns not-found or provider failure.
    fn delete(&self, key: &StorageKey) -> Result<(), StorageError>;

    /// Starts a multipart upload for a key that is not yet committed.
    ///
    /// # Errors
    ///
    /// Returns a validation, conflict, or provider failure.
    fn begin_multipart(
        &self,
        key: &StorageKey,
        expected: &StorageMetadata,
    ) -> Result<MultipartId, StorageError>;

    /// Replaces one numbered multipart part and returns computed integrity metadata.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failure.
    fn put_part(
        &self,
        upload: &MultipartId,
        number: u32,
        source: &mut dyn Read,
    ) -> Result<MultipartPart, StorageError>;

    /// Commits ordered multipart parts atomically as one complete object.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, integrity, or provider failure.
    fn complete_multipart(
        &self,
        upload: &MultipartId,
        parts: &[MultipartPart],
    ) -> Result<StorageMetadata, StorageError>;

    /// Removes staged multipart state without touching committed bytes.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failure.
    fn abort_multipart(&self, upload: &MultipartId) -> Result<(), StorageError>;
}

/// Read-only physical object inventory used by standalone operator commands.
///
/// This contract is deliberately separate from [`ObjectStorage`] so request-path adapters and
/// test doubles do not gain an operator-only responsibility.
pub trait ObjectStorageInventory: Send + Sync {
    /// Lists every committed physical object key in stable ascending order.
    ///
    /// Multipart state, local staging files, and provider-specific metadata are excluded. An
    /// adapter must fail closed when it encounters a key it cannot represent safely.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures rather than omitting an unsafe or unreadable key.
    fn list_object_keys(&self) -> Result<Vec<StorageKey>, StorageError>;
}
