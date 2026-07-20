use super::Fixture;
use crate::{test_support, test_support::error_status, yard_cleanup::execute_for_yard};
use axum::http::StatusCode;
use blobyard_storage_filesystem::FilesystemStorage;
use std::sync::Arc;

#[test]
fn request_path_executes_the_pending_cleanup_for_the_selected_yard() {
    let fixture = Fixture::new(true);
    let state = test_support::state(
        &fixture.root,
        fixture.root.path().join("staging"),
        Arc::new(FilesystemStorage::open(&fixture.root.path().join("objects")).expect("storage")),
    );

    execute_for_yard(&state, "yard_cleanup_1", 4).expect("request cleanup");

    assert_eq!(fixture.pending_count(), 0);
    assert!(
        fixture
            .audit_actions()
            .contains(&"yard.cleanup_completed".to_owned())
    );
}

#[test]
fn request_path_surfaces_cleanup_lookup_and_execution_failures() {
    let lookup = Fixture::new(false);
    lookup
        .repository
        .test_connection()
        .expect("connection")
        .execute("DROP TABLE deletion_operations", [])
        .expect("break lookup");
    let lookup_state = test_support::state(
        &lookup.root,
        lookup.root.path().join("staging"),
        Arc::new(FilesystemStorage::open(&lookup.root.path().join("objects")).expect("storage")),
    );
    assert_eq!(
        error_status(execute_for_yard(&lookup_state, "yard_cleanup_1", 4)),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let execution = Fixture::new(false);
    execution
        .repository
        .test_connection()
        .expect("connection")
        .execute(
            "UPDATE deletion_items SET storage_key = '../invalid' WHERE operation_id = 'yardcleanup_deploy_cleanup_1'",
            [],
        )
        .expect("corrupt key");
    let execution_state = test_support::state(
        &execution.root,
        execution.root.path().join("staging"),
        Arc::new(FilesystemStorage::open(&execution.root.path().join("objects")).expect("storage")),
    );
    assert_eq!(
        error_status(execute_for_yard(&execution_state, "yard_cleanup_1", 4)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(execution.pending_count(), 1);
}
