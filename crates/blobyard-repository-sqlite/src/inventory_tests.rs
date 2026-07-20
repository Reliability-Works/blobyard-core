use super::{stable_behavior::repository, version};
use crate::adapter::inventory::query_versions;
use blobyard_contract::{
    MetadataRepository, MetadataRepositoryInventory, NewObjectVersion, ObjectSource, UploadState,
};

#[test]
fn inventory_lists_every_state_in_stable_storage_key_order() {
    let (_temporary, repository) = repository();

    let mut complete = record("complete", "z-complete", 1);
    repository
        .reserve_object_version(&complete)
        .expect("reserve complete");
    repository
        .complete_object_version(&complete.id, 3, &"a".repeat(64))
        .expect("complete version");
    let pending = record("pending", "a-pending", 2);
    repository
        .reserve_object_version(&pending)
        .expect("reserve pending");
    let aborted = record("aborted", "m-aborted", 3);
    repository
        .reserve_object_version(&aborted)
        .expect("reserve aborted");
    repository
        .abort_object_version(&aborted.id)
        .expect("abort version");

    let records = repository.list_object_versions().expect("inventory");
    assert_eq!(
        records
            .iter()
            .map(|record| (record.storage_key.as_str(), record.state))
            .collect::<Vec<_>>(),
        [
            ("a-pending", UploadState::Pending),
            ("m-aborted", UploadState::Aborted),
            ("z-complete", UploadState::Complete),
        ]
    );

    complete.storage_key = "unused".to_owned();
    assert_eq!(complete.source, ObjectSource::Cli);
}

#[test]
fn inventory_maps_query_and_row_failures() {
    let (_temporary, repository) = repository();
    repository
        .reserve_object_version(&version())
        .expect("reserve version");
    {
        let connection = repository.test_connection().expect("connection");
        connection
            .execute_batch(
                "PRAGMA foreign_keys = OFF; DELETE FROM object_versions; DROP TABLE object_versions;",
            )
            .expect("drop table");
    }
    assert_eq!(
        repository.list_object_versions().err(),
        Some(blobyard_contract::RepositoryError::Unavailable)
    );
}

#[test]
fn inventory_query_rejects_statements_that_require_parameters() {
    let connection = rusqlite::Connection::open_in_memory().expect("connection");
    let mut statement = connection.prepare("SELECT ?1").expect("statement");
    assert_eq!(
        query_versions(&mut statement),
        Err(blobyard_contract::RepositoryError::Unavailable)
    );
}

#[test]
fn inventory_maps_connection_failures() {
    let (_temporary, repository) = repository();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _connection = repository.test_connection().expect("connection");
        std::panic::resume_unwind(Box::new(()));
    }));
    assert!(unwind.is_err());
    assert_eq!(
        repository.list_object_versions().err(),
        Some(blobyard_contract::RepositoryError::Unavailable)
    );
}

#[test]
fn inventory_maps_corrupt_rows_without_returning_partial_results() {
    let (_temporary, repository) = repository();
    repository
        .reserve_object_version(&version())
        .expect("reserve version");
    repository
        .test_connection()
        .expect("connection")
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON; UPDATE object_versions SET source = 'corrupt';",
        )
        .expect("corrupt source");

    assert_eq!(
        repository.list_object_versions().err(),
        Some(blobyard_contract::RepositoryError::Unavailable)
    );

    repository
        .test_connection()
        .expect("connection")
        .execute_batch("UPDATE object_versions SET source = 'cli', state = 'corrupt';")
        .expect("corrupt state");
    assert_eq!(
        repository.list_object_versions().err(),
        Some(blobyard_contract::RepositoryError::Unavailable)
    );
}

fn record(id: &str, storage_key: &str, number: u64) -> NewObjectVersion {
    NewObjectVersion {
        id: id.to_owned(),
        project_id: "project_fixture".to_owned(),
        object_path: format!("{id}.bin"),
        version: number,
        storage_key: storage_key.to_owned(),
        source: ObjectSource::Cli,
        git_repository: None,
        git_commit: None,
        git_branch: None,
    }
}
