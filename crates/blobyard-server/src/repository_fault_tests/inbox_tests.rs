#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{FaultingRepository, Repository};
use crate::transfers::test_seams;
use blobyard_contract::{
    InboxRepository, NewInbox, NewInboxUpload, NewUploadReservation, ObjectSource, RepositoryError,
};
use std::sync::Arc;

fn inbox() -> NewInbox {
    NewInbox {
        id: "inbox_fault".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: "Fault inbox".to_owned(),
        capability_hash: "a".repeat(64),
        expires_at_ms: 5_000,
        maximum_files: 2,
        maximum_bytes: 10,
        created_at_ms: 1_000,
    }
}

fn upload() -> NewUploadReservation {
    NewUploadReservation {
        id: "inbox_upload_fault".to_owned(),
        project_id: "project_fixture".to_owned(),
        object_path: "inbox/fault.bin".to_owned(),
        filename: "fault.bin".to_owned(),
        content_type: "application/octet-stream".to_owned(),
        expected_size: 5,
        expected_checksum: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
            .to_owned(),
        storage_key: "objects/inbox_upload_fault".to_owned(),
        capability_hash: "b".repeat(64),
        expires_at_ms: 4_000,
        created_at_ms: 1_100,
        source: ObjectSource::Inbox,
        git_repository: None,
        git_commit: None,
        git_branch: None,
        strategy: blobyard_contract::ReservationStrategy::Single,
        part_size: None,
        part_count: None,
    }
}

#[test]
fn inbox_fault_wrapper_forwards_every_operation() {
    let (_temporary, inner) = super::conforming_repository();
    blobyard_testkit::inbox_conformance(&FaultingRepository::new(inner, usize::MAX))
        .expect("inbox conformance");
}

#[test]
fn inbox_fault_wrapper_fails_every_operation_at_the_boundary() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let fail = || FaultingRepository::new(Arc::clone(&inner), 0);
    let inbox = inbox();
    let upload = upload();
    let principal = NewInboxUpload {
        capability_hash: inbox.capability_hash.clone(),
        fingerprint_hash: "c".repeat(64),
        now_ms: 1_100,
    };
    let created = blobyard_testkit::inbox_event("inbox.created", &inbox.id, 1_000);
    let completed = blobyard_testkit::inbox_upload_event(&inbox.id, 1_101);
    let revoked = blobyard_testkit::inbox_event("inbox.revoked", &inbox.id, 1_200);
    assert_eq!(
        fail().create_inbox(&inbox, &created),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().list_inboxes(&inbox.project_id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().inbox_by_capability(&inbox.capability_hash, 1_001),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().consume_inbox_rate(&"d".repeat(64), 1_000, 1, 1_000),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().reserve_inbox_upload(&principal, &upload),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().inbox_upload_by_id(&inbox.capability_hash, &upload.id, 1_101),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().complete_inbox_upload(&inbox.capability_hash, &upload.id, 1_101, &completed,),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().abort_inbox_upload(&inbox.capability_hash, &upload.id, 1_101),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().revoke_inbox(&inbox.id, &inbox.workspace_id, 1_200, &revoked),
        Err(RepositoryError::Unavailable)
    );
}
