use crate::{ObjectSource, ObjectVersionRecord, RepositoryError};

/// Input for one durable single-upload reservation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewUploadReservation {
    /// Stable upload identifier.
    pub id: String,
    /// Parent project identifier.
    pub project_id: String,
    /// Logical object path.
    pub object_path: String,
    /// Original safe filename.
    pub filename: String,
    /// Client content type hint.
    pub content_type: String,
    /// Exact expected byte size.
    pub expected_size: u64,
    /// Expected lowercase SHA-256 digest.
    pub expected_checksum: String,
    /// Provider-independent storage key.
    pub storage_key: String,
    /// Digest of the upload capability.
    pub capability_hash: String,
    /// Capability expiration as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Durable reservation creation timestamp as Unix milliseconds.
    pub created_at_ms: u64,
    /// Authenticated ingestion surface creating the immutable object version.
    pub source: ObjectSource,
    /// Optional normalized source repository.
    pub git_repository: Option<String>,
    /// Optional source commit identifier.
    pub git_commit: Option<String>,
    /// Optional source branch provenance.
    pub git_branch: Option<String>,
    /// Selected provider-independent transfer strategy.
    pub strategy: ReservationStrategy,
    /// Fixed multipart part size, when multipart is selected.
    pub part_size: Option<u64>,
    /// Exact multipart part count, when multipart is selected.
    pub part_count: Option<u32>,
}

/// Provider-independent upload reservation strategy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservationStrategy {
    /// One bounded complete-object PUT.
    Single,
    /// Ordered staged parts followed by one atomic completion.
    Multipart,
}

impl ReservationStrategy {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multipart => "multipart",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "single" => Some(Self::Single),
            "multipart" => Some(Self::Multipart),
            _ => None,
        }
    }
}

/// Input for one retry-stable multipart part capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewUploadPartGrant {
    /// Parent upload reservation.
    pub upload_id: String,
    /// One-based part number.
    pub part_number: u32,
    /// Exact expected part byte size.
    pub expected_size: u64,
    /// Digest of the raw part capability.
    pub capability_hash: String,
    /// Capability expiration as Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Persisted multipart part lifecycle and integrity metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UploadPartRecord {
    /// Parent upload reservation.
    pub upload_id: String,
    /// One-based part number.
    pub part_number: u32,
    /// Exact expected part byte size.
    pub expected_size: u64,
    /// Capability expiration as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Received byte size, when uploaded.
    pub received_size: Option<u64>,
    /// Received lowercase SHA-256 checksum, when uploaded.
    pub received_checksum: Option<String>,
    /// Opaque storage-provider tag needed to complete the uploaded part.
    pub provider_tag: Option<String>,
}

/// Input for one short-lived object download capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewDownloadGrant {
    /// Immutable object-version identifier.
    pub version_id: String,
    /// SHA-256 digest of the raw capability.
    pub capability_hash: String,
    /// Capability expiration as Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Durable upload lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservationState {
    /// Capability exists but bytes have not been committed.
    Requested,
    /// Bytes passed integrity checks and await metadata completion.
    Uploaded,
    /// Metadata and bytes are committed.
    Complete,
    /// The reservation was explicitly abandoned.
    Aborted,
}

impl ReservationState {
    /// Returns the stable persisted representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Uploaded => "uploaded",
            Self::Complete => "complete",
            Self::Aborted => "aborted",
        }
    }

    /// Parses the stable persisted representation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "uploaded" => Some(Self::Uploaded),
            "complete" => Some(Self::Complete),
            "aborted" => Some(Self::Aborted),
            _ => None,
        }
    }
}

/// Persisted upload reservation and immutable version allocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UploadReservationRecord {
    /// Stable upload identifier.
    pub id: String,
    /// Reserved immutable object version.
    pub version: ObjectVersionRecord,
    /// Original safe filename.
    pub filename: String,
    /// Client content type hint.
    pub content_type: String,
    /// Exact expected byte size.
    pub expected_size: u64,
    /// Expected lowercase SHA-256 digest.
    pub expected_checksum: String,
    /// Capability expiration as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Current reservation lifecycle state.
    pub state: ReservationState,
    /// Selected provider-independent transfer strategy.
    pub strategy: ReservationStrategy,
    /// Fixed multipart part size, when multipart is selected.
    pub part_size: Option<u64>,
    /// Exact multipart part count, when multipart is selected.
    pub part_count: Option<u32>,
    /// Opaque storage-provider multipart upload identifier.
    pub provider_upload_id: Option<String>,
}

/// Completed object metadata needed by list and download journeys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredObjectRecord {
    /// Immutable version metadata.
    pub version: ObjectVersionRecord,
    /// Original safe filename.
    pub filename: String,
    /// Client content type hint.
    pub content_type: String,
}

/// Durable metadata operations for byte transfer orchestration.
pub trait TransferRepository: Send + Sync {
    /// Atomically allocates the next object version and its upload capability.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn reserve_upload(
        &self,
        reservation: &NewUploadReservation,
    ) -> Result<UploadReservationRecord, RepositoryError>;

    /// Resolves an active, unexpired upload capability by digest.
    ///
    /// # Errors
    ///
    /// Returns not-found for unknown, expired, or consumed capabilities.
    fn upload_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError>;

    /// Reads an upload reservation by stable identifier.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failures.
    fn upload_by_id(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError>;

    /// Renews an expired requested capability without reallocating its object version.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn renew_upload(&self, id: &str, expires_at_ms: u64) -> Result<(), RepositoryError>;

    /// Attaches the one storage-provider multipart identifier to a reservation.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn attach_multipart(
        &self,
        id: &str,
        provider_upload_id: &str,
    ) -> Result<UploadReservationRecord, RepositoryError>;

    /// Atomically issues or renews one bounded batch of multipart part capabilities.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn issue_upload_parts(
        &self,
        parts: &[NewUploadPartGrant],
    ) -> Result<Vec<UploadPartRecord>, RepositoryError>;

    /// Resolves an active multipart part capability by digest.
    ///
    /// # Errors
    ///
    /// Returns not-found for unknown, expired, consumed, or inactive capabilities.
    fn upload_part_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<(UploadReservationRecord, UploadPartRecord), RepositoryError>;

    /// Records storage-computed integrity metadata for one uploaded part.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn record_uploaded_part(
        &self,
        upload_id: &str,
        part_number: u32,
        size: u64,
        checksum: &str,
        provider_tag: Option<&str>,
    ) -> Result<(), RepositoryError>;

    /// Lists every requested multipart part in ascending number order.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or provider failures.
    fn list_upload_parts(&self, upload_id: &str) -> Result<Vec<UploadPartRecord>, RepositoryError>;

    /// Marks integrity-checked bytes as durably stored.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn record_uploaded_bytes(
        &self,
        id: &str,
        size: u64,
        checksum: &str,
    ) -> Result<(), RepositoryError>;

    /// Atomically completes uploaded metadata and consumes transfer authority.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn complete_upload(&self, id: &str) -> Result<ObjectVersionRecord, RepositoryError>;

    /// Aborts a requested or uploaded reservation and its pending object version.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn abort_upload(&self, id: &str) -> Result<UploadReservationRecord, RepositoryError>;

    /// Lists completed object versions for one project in stable path and version order.
    ///
    /// # Errors
    ///
    /// Returns validation or provider failures.
    fn list_stored_objects(
        &self,
        project_id: &str,
        prefix: Option<&str>,
        include_versions: bool,
    ) -> Result<Vec<StoredObjectRecord>, RepositoryError>;

    /// Persists a hashed short-lived download capability.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or provider failures.
    fn issue_download(&self, grant: &NewDownloadGrant) -> Result<(), RepositoryError>;

    /// Resolves an active download capability to one completed object version.
    ///
    /// # Errors
    ///
    /// Returns not-found for unknown, expired, or unavailable capabilities.
    fn download_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<StoredObjectRecord, RepositoryError>;
}

#[cfg(test)]
#[path = "transfers_tests.rs"]
mod tests;
