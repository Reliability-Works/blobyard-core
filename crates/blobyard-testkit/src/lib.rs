//! Reusable, non-secret fixtures for Blobyard tests.

use blobyard_contract::{AuditValue, NewAuditEvent};
use blobyard_core::{BlobyardUri, BlobyardUriError};

mod ci;
mod credentials;
mod fault_forwarders;
mod groups;
mod lifecycle;
mod local_users;
mod repository;
mod storage;

pub use ci::{CI_REPOSITORY, ci_trust, github_oidc_identity};
pub use credentials::{cli_session_record, cli_session_revoked_event, credential_conformance};
pub use fault_forwarders::FailureCounter;
pub use groups::{GroupConformanceRepository, group_conformance, group_event};
pub use lifecycle::lifecycle_conformance;
pub use local_users::{local_user, local_user_conformance, local_user_event, login_key};
pub use repository::{
    InboxConformanceRepository, PreviewConformanceRepository, YardConformanceFixture,
    YardConformanceRepository, granted_event, inbox_conformance, inbox_event, inbox_upload_event,
    new_grant, preview_conformance, preview_event, repository_conformance, revoked_event,
    share_event, sharing_conformance, transfer_conformance, visibility_event, yard_conformance,
    yard_event,
};
pub use storage::storage_conformance;

pub(crate) fn ensure_equal<T: Eq>(
    actual: &T,
    expected: &T,
) -> Result<(), blobyard_contract::RepositoryError> {
    if actual == expected {
        Ok(())
    } else {
        Err(blobyard_contract::RepositoryError::Unavailable)
    }
}

pub(crate) fn hash(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

/// A stable valid URI suitable for tests that do not care about object identity.
pub const SAMPLE_BLOBYARD_URI: &str = "blobyard://sample/default/builds/app.zip?version=1";

/// Parses [`SAMPLE_BLOBYARD_URI`] without hiding parse failures.
///
/// # Errors
///
/// Returns a URI validation error if the shared fixture stops satisfying the
/// canonical Blobyard URI contract.
pub fn sample_blobyard_uri() -> Result<BlobyardUri, BlobyardUriError> {
    SAMPLE_BLOBYARD_URI.parse()
}

/// Builds the canonical non-secret audit fixture for a workspace rename.
#[must_use]
pub fn workspace_renamed_event(
    workspace_id: &str,
    previous_slug: &str,
    created_at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: "audit_workspace_renamed".to_owned(),
        workspace_id: workspace_id.to_owned(),
        actor: "fixture".to_owned(),
        action: "workspace.renamed".to_owned(),
        request_id: "request_workspace_renamed".to_owned(),
        target_type: "workspace".to_owned(),
        metadata: vec![(
            "previousSlug".to_owned(),
            AuditValue::String(previous_slug.to_owned()),
        )],
        created_at_ms,
    }
}
