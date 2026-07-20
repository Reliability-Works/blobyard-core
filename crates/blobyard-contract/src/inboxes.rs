use crate::{
    NewAuditEvent, NewUploadReservation, ObjectVersionRecord, RepositoryError,
    UploadReservationRecord,
};

/// Persisted lifecycle state for one public upload inbox.
pub type InboxStatus = crate::repository::RevocableStatus;

/// Validated input for one bounded public upload inbox.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewInbox {
    /// Stable inbox identifier.
    pub id: String,
    /// Workspace that owns the inbox.
    pub workspace_id: String,
    /// Project that receives uploaded objects.
    pub project_id: String,
    /// Human-readable display name.
    pub name: String,
    /// Lowercase SHA-256 digest of the raw capability.
    pub capability_hash: String,
    /// Absolute capability expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Maximum number of completed files.
    pub maximum_files: u64,
    /// Maximum total completed bytes.
    pub maximum_bytes: u64,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
}

/// Redacted durable inbox metadata and capacity counters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboxRecord {
    /// Stable inbox identifier.
    pub id: String,
    /// Workspace that owns the inbox.
    pub workspace_id: String,
    /// Project that receives uploaded objects.
    pub project_id: String,
    /// Human-readable display name.
    pub name: String,
    /// Absolute capability expiry as Unix milliseconds.
    pub expires_at_ms: u64,
    /// Persisted lifecycle state.
    pub status: InboxStatus,
    /// Number of completed files.
    pub current_files: u64,
    /// Total completed bytes.
    pub current_bytes: u64,
    /// Number of files reserved by incomplete uploads.
    pub reserved_files: u64,
    /// Bytes reserved by incomplete uploads.
    pub reserved_bytes: u64,
    /// Maximum number of completed or reserved files.
    pub maximum_files: u64,
    /// Maximum completed or reserved bytes.
    pub maximum_bytes: u64,
    /// Creation time as Unix milliseconds.
    pub created_at_ms: u64,
    /// Revocation time, when revoked.
    pub revoked_at_ms: Option<u64>,
}

/// Public-capability metadata attached to one new upload reservation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewInboxUpload {
    /// Lowercase SHA-256 digest of the raw inbox capability.
    pub capability_hash: String,
    /// Non-secret digest identifying the guest request source.
    pub fingerprint_hash: String,
    /// Timestamp used for capability and capacity checks.
    pub now_ms: u64,
}

/// Durable rate-limit decision for one public inbox request class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboxRateResult {
    /// The request is within its current fixed window.
    Allowed,
    /// The current fixed window is exhausted.
    Limited {
        /// Whole seconds until a new request may be attempted.
        retry_after_seconds: u64,
    },
}

/// Durable standalone operations for bounded upload inboxes.
pub trait InboxRepository: Send + Sync {
    /// Atomically creates one inbox and its audit event.
    ///
    /// # Errors
    ///
    /// Returns validation, conflict, or persistence failures.
    fn create_inbox(
        &self,
        inbox: &NewInbox,
        event: &NewAuditEvent,
    ) -> Result<InboxRecord, RepositoryError>;

    /// Lists redacted inboxes for one project in newest-first order.
    ///
    /// # Errors
    ///
    /// Returns validation or persistence failures.
    fn list_inboxes(&self, project_id: &str) -> Result<Vec<InboxRecord>, RepositoryError>;

    /// Resolves one active, unexpired inbox capability.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or persistence failures.
    fn inbox_by_capability(
        &self,
        capability_hash: &str,
        now_ms: u64,
    ) -> Result<InboxRecord, RepositoryError>;

    /// Atomically consumes one public fixed-window rate-limit slot.
    ///
    /// # Errors
    ///
    /// Returns validation or persistence failures.
    fn consume_inbox_rate(
        &self,
        rate_key: &str,
        window_ms: u64,
        limit: u32,
        now_ms: u64,
    ) -> Result<InboxRateResult, RepositoryError>;

    /// Atomically reserves inbox capacity and one immutable upload.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or persistence failures.
    fn reserve_inbox_upload(
        &self,
        inbox_upload: &NewInboxUpload,
        reservation: &NewUploadReservation,
    ) -> Result<UploadReservationRecord, RepositoryError>;

    /// Reads one active inbox-owned upload after authenticating its capability.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or persistence failures.
    fn inbox_upload_by_id(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError>;

    /// Atomically completes an inbox upload, capacity counters, and audit event.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or persistence failures.
    fn complete_inbox_upload(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<ObjectVersionRecord, RepositoryError>;

    /// Atomically aborts an inbox upload and releases its reserved capacity.
    ///
    /// # Errors
    ///
    /// Returns not-found, conflict, validation, or persistence failures.
    fn abort_inbox_upload(
        &self,
        capability_hash: &str,
        upload_id: &str,
        now_ms: u64,
    ) -> Result<UploadReservationRecord, RepositoryError>;

    /// Atomically revokes one workspace-owned inbox and records audit.
    ///
    /// # Errors
    ///
    /// Returns not-found, validation, or persistence failures.
    fn revoke_inbox(
        &self,
        inbox_id: &str,
        workspace_id: &str,
        revoked_at_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<bool, RepositoryError>;
}

#[cfg(test)]
#[path = "inboxes_tests.rs"]
mod tests;
