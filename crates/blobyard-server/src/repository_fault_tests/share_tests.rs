#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{FaultingRepository, Repository};
use crate::transfers::test_seams;
use blobyard_contract::{NewDownloadGrant, NewShare, RepositoryError, SharingRepository};
use std::sync::Arc;

fn event(action: &str) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::share_event(action, "share_fault", 1)
}

fn share() -> NewShare {
    NewShare {
        id: "share_fault".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        version_id: "upload_two".to_owned(),
        capability_hash: "e".repeat(64),
        expires_at_ms: 5_000,
        maximum_downloads: None,
        created_at_ms: 1,
    }
}

fn grant() -> NewDownloadGrant {
    NewDownloadGrant {
        version_id: "upload_two".to_owned(),
        capability_hash: "f".repeat(64),
        expires_at_ms: 2,
    }
}

#[test]
fn share_fault_wrapper_forwards_every_operation() {
    let (_temporary, inner) = super::conforming_repository();
    blobyard_testkit::sharing_conformance(&FaultingRepository::new(inner, usize::MAX))
        .expect("sharing conformance");
}

#[test]
fn share_fault_wrapper_fails_every_operation_at_the_boundary() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let fail = || FaultingRepository::new(Arc::clone(&inner), 0);
    let share = share();
    let created = event("share.created");
    let issued = event("share.download_issued");
    let revoked = event("share.revoked");
    let grant = grant();
    assert_eq!(
        fail().create_share(&share, &created),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().list_shares(&share.workspace_id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().share_by_capability(&share.capability_hash, 1),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().issue_share_download(&share.capability_hash, 1, &grant, &issued),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().revoke_share(&share.id, &share.workspace_id, 1, &revoked),
        Err(RepositoryError::Unavailable)
    );
}
