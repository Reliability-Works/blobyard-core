//! Reusable, non-secret fixtures for Blobyard tests.

use blobyard_contract::{AuditValue, NewAuditEvent};
use blobyard_core::{BlobyardUri, BlobyardUriError};

mod ci;
mod credentials;
mod fault_forwarders;
mod lifecycle;
mod repository;
mod storage;

pub use ci::{CI_REPOSITORY, ci_trust, github_oidc_identity};
pub use credentials::{cli_session_record, cli_session_revoked_event, credential_conformance};
pub use fault_forwarders::FailureCounter;
pub use lifecycle::lifecycle_conformance;
pub use repository::{
    InboxConformanceRepository, PreviewConformanceRepository, YardConformanceFixture,
    YardConformanceRepository, inbox_conformance, inbox_event, inbox_upload_event,
    preview_conformance, preview_event, repository_conformance, share_event, sharing_conformance,
    transfer_conformance, yard_conformance, yard_event,
};
pub use storage::storage_conformance;

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
