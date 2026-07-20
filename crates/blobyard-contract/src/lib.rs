//! Provider-independent service contracts for Blob Yard runtimes.

mod auth;
mod ci;
mod inboxes;
mod lifecycle;
mod migration;
mod previews;
mod repository;
mod sharing;
mod storage;
mod transfers;
mod yard_repository;
mod yards;

fn is_valid_relative_path(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
}

pub use auth::{CredentialRepository, LocalApiTokenRecord, LocalCliSessionRecord};
pub use ci::{
    CiAction, CiRepository, GithubOidcIdentity, LocalCiTrustRecord, LocalMachineSessionRecord,
    MachineSessionMintResult, NewCiAuditEvent, NewMachineSession, ci_audit_event,
    valid_github_ref_tail, valid_github_repository_part, valid_github_workflow_path,
};
pub use inboxes::{
    InboxRateResult, InboxRecord, InboxRepository, InboxStatus, NewInbox, NewInboxUpload,
};
pub use lifecycle::{
    AuditEventRecord, AuditPage, AuditValue, DeletionItem, DeletionPlan, LifecycleRepository,
    NewAuditEvent, NewObjectDeletion, ObjectDeletionTarget, RetentionOverview,
    RetentionPolicyRecord, RetentionRunRecord,
};
pub use migration::{
    MigrationObjectRecord, MigrationRepository, MigrationRetentionRecord, MigrationShareRecord,
    MigrationSnapshot,
};
pub use previews::{
    MAXIMUM_PREVIEW_PATH_BYTES, NewPreview, NewPreviewFile, PreviewRecord, PreviewRepository,
    PreviewStatus, PreviewTarget, is_valid_preview_path,
};
pub use repository::{
    MetadataRepository, MetadataRepositoryInventory, NewObjectVersion, ObjectSource,
    ObjectVersionRecord, ProjectRecord, RepositoryError, RevocableStatus, UploadState,
    WorkspaceRecord,
};
pub use sharing::{NewShare, ShareRecord, ShareStatus, ShareTarget, SharingRepository};
pub use storage::{
    ByteRange, MultipartId, MultipartPart, ObjectChecksum, ObjectStorage, ObjectStorageInventory,
    StorageError, StorageKey, StorageMetadata, StorageRead,
};
pub use transfers::{
    NewDownloadGrant, NewUploadPartGrant, NewUploadReservation, ReservationState,
    ReservationStrategy, StoredObjectRecord, TransferRepository, UploadPartRecord,
    UploadReservationRecord,
};
pub use yard_repository::{WebYardRepository, YardCleanupPlan};
pub use yards::{
    MAXIMUM_YARD_PATH_BYTES, NewWebYard, NewYardDeploy, NewYardFile, WebYardRecord, WebYardStatus,
    YardDeployRecord, YardDeployStatus, YardDeploymentRecord, YardFileTarget, YardStartRecord,
    is_valid_yard_path, is_valid_yard_request_path,
};
