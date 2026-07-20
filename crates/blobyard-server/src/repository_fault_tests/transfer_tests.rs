#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{Corruption, FaultingRepository, Repository};
use crate::test_support::multipart_upload;
use crate::transfers::test_seams;
use blobyard_contract::{
    NewUploadPartGrant, NewUploadReservation, RepositoryError, TransferRepository,
};
use std::sync::Arc;

fn multipart(project_id: &str) -> NewUploadReservation {
    let mut upload = multipart_upload::record("upload_fault_multipart", 5, 5, 1, None);
    upload.version.project_id = project_id.to_owned();
    multipart_upload::reservation(&upload, 'b', 10)
}

fn part() -> NewUploadPartGrant {
    NewUploadPartGrant {
        upload_id: "upload_fault_multipart".to_owned(),
        part_number: 1,
        expected_size: 5,
        capability_hash: "c".repeat(64),
        expires_at_ms: 10,
    }
}

#[test]
fn transfer_fault_wrapper_forwards_every_multipart_operation() {
    let fixture = test_seams::fixture(&["object:write"]);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let upload = multipart(&fixture.project.id);
    inner
        .reserve_upload(&upload)
        .expect("multipart reservation");

    FaultingRepository::new(Arc::clone(&inner), 1)
        .attach_multipart(&upload.id, "provider")
        .expect("forward provider attachment");
    let grant = part();
    FaultingRepository::new(Arc::clone(&inner), 1)
        .issue_upload_parts(std::slice::from_ref(&grant))
        .expect("forward part grant");
    FaultingRepository::new(Arc::clone(&inner), 1)
        .upload_part_by_capability(&grant.capability_hash, 1)
        .expect("forward capability lookup");
    FaultingRepository::new(Arc::clone(&inner), 1)
        .record_uploaded_part(&upload.id, 1, 5, &"d".repeat(64), Some("provider-tag"))
        .expect("forward uploaded part");
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 1)
            .list_upload_parts(&upload.id)
            .expect("forward part listing")
            .len(),
        1
    );

    let corrupted = FaultingRepository::corrupting(inner, Corruption::AbortedStorageKey)
        .upload_by_id(&upload.id)
        .expect("corrupt upload lookup");
    assert_eq!(corrupted.version.storage_key, "../invalid");

    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let aborted = FaultingRepository::corrupting(inner, Corruption::AbortedStorageKey)
        .abort_upload(&upload.id)
        .expect("corrupt aborted upload");
    assert_eq!(aborted.version.storage_key, "../invalid");
}

#[test]
fn transfer_fault_wrapper_fails_every_multipart_operation_at_the_boundary() {
    let fixture = test_seams::fixture(&["object:write"]);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let grant = part();
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0).attach_multipart("upload", "provider"),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0)
            .issue_upload_parts(std::slice::from_ref(&grant)),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0)
            .upload_part_by_capability(&grant.capability_hash, 1),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(Arc::clone(&inner), 0).record_uploaded_part(
            "upload",
            1,
            5,
            &"d".repeat(64),
            Some("provider-tag")
        ),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        FaultingRepository::new(inner, 0).list_upload_parts("upload"),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn transfer_fault_wrapper_propagates_inner_record_failures() {
    let fixture = test_seams::fixture(&["object:write"]);
    let nested = || {
        let inner: Arc<dyn Repository> = Arc::new(FaultingRepository::new(
            Arc::clone(&fixture.state.repository),
            0,
        ));
        FaultingRepository::new(inner, 1)
    };
    assert_eq!(
        nested().upload_by_id("upload"),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        nested().complete_upload("upload"),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        nested().abort_upload("upload"),
        Err(RepositoryError::Unavailable)
    );
}
