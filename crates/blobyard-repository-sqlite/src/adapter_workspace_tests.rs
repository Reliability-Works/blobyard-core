#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::stable_behavior::repository;
use blobyard_contract::{AuditValue, MetadataRepository, RepositoryError, WorkspaceRecord};
use blobyard_core::Slug;

#[test]
fn workspace_rename_is_atomic_with_its_audit_event() {
    let (_temporary, repository) = repository();
    let renamed = WorkspaceRecord {
        id: "workspace_fixture".to_owned(),
        name: "Renamed".to_owned(),
        slug: Slug::new("renamed").expect("renamed slug"),
    };
    let mut wrong_event = workspace_rename_event("wrong");
    assert_eq!(
        repository.rename_workspace(&renamed, &wrong_event),
        Err(RepositoryError::InvalidInput)
    );
    assert!(
        repository
            .workspace_by_slug(&Slug::new("fixture").expect("original slug"))
            .is_ok()
    );

    wrong_event.metadata = vec![(
        "previousSlug".to_owned(),
        AuditValue::String("fixture".to_owned()),
    )];
    repository
        .test_connection()
        .expect("connection")
        .execute_batch("DROP TABLE audit_events")
        .expect("drop audit table");
    assert_eq!(
        repository.rename_workspace(&renamed, &wrong_event),
        Err(RepositoryError::Unavailable)
    );
    assert!(
        repository
            .workspace_by_slug(&Slug::new("fixture").expect("original slug"))
            .is_ok()
    );
}

#[test]
fn workspace_rename_rejects_a_suppressed_update_without_writing_audit() {
    let (_temporary, repository) = repository();
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER suppress_workspace_rename BEFORE UPDATE ON workspaces BEGIN SELECT RAISE(IGNORE); END;",
        )
        .expect("suppressing trigger");
    let renamed = WorkspaceRecord {
        id: "workspace_fixture".to_owned(),
        name: "Renamed".to_owned(),
        slug: Slug::new("renamed").expect("renamed slug"),
    };
    assert_eq!(
        repository.rename_workspace(&renamed, &workspace_rename_event("fixture")),
        Err(RepositoryError::NotFound)
    );
    assert!(
        repository
            .workspace_by_slug(&Slug::new("fixture").expect("original slug"))
            .is_ok()
    );
}

fn workspace_rename_event(previous_slug: &str) -> blobyard_contract::NewAuditEvent {
    blobyard_testkit::workspace_renamed_event("workspace_fixture", previous_slug, 3)
}
