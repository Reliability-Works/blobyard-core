#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{Corruption, FaultingRepository, Repository};
use crate::transfers::test_seams;
use blobyard_contract::{NewPreview, NewPreviewFile, PreviewRepository, RepositoryError};
use std::sync::Arc;

fn preview() -> NewPreview {
    NewPreview {
        id: "preview_fault".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        capability_hash: "a".repeat(64),
        expires_at_ms: 5_000,
        created_at_ms: 1_000,
        files: vec![NewPreviewFile {
            normalized_path: "index.html".to_owned(),
            version_id: "upload_two".to_owned(),
        }],
    }
}

#[test]
fn preview_fault_wrapper_forwards_every_operation() {
    let (_temporary, inner) = super::conforming_repository();
    let repository = FaultingRepository::new(inner, usize::MAX);
    blobyard_testkit::preview_conformance(&repository).expect("preview conformance");
    assert_eq!(
        repository
            .preview_by_id("preview_fixture")
            .expect("preview")
            .id,
        "preview_fixture"
    );
}

#[test]
fn preview_fault_wrapper_fails_every_operation_at_the_boundary() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let fail = || FaultingRepository::new(Arc::clone(&inner), 0);
    let preview = preview();
    let created = blobyard_testkit::preview_event("preview.created", &preview.id, 1_000);
    let revoked = blobyard_testkit::preview_event("preview.revoked", &preview.id, 1_100);
    assert_eq!(
        fail().create_preview(&preview, &created),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().list_previews(&preview.project_id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().preview_by_id(&preview.id),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().preview_file_by_capability(&preview.capability_hash, "index.html", 1_001),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        fail().revoke_preview(
            &preview.id,
            &preview.workspace_id,
            &preview.project_id,
            1_100,
            &revoked,
        ),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn preview_corruption_wrapper_preserves_inner_list_failure() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let inner: Arc<dyn Repository> = Arc::new(FaultingRepository::new(
        Arc::clone(&fixture.state.repository),
        0,
    ));
    let repository = FaultingRepository::corrupting(inner, Corruption::PreviewCreatedAt);
    assert_eq!(
        repository.list_previews("project_fixture"),
        Err(RepositoryError::Unavailable)
    );
}
