#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::tests::{repository, snapshot};
use super::*;
use blobyard_contract::{MetadataRepository, MigrationRepository};

#[test]
fn import_maps_each_sqlite_stage_failure_and_rolls_back() {
    for table in [
        "workspaces",
        "projects",
        "object_versions",
        "shares",
        "retention_policies",
    ] {
        let (_temporary, repository) = repository();
        repository
            .test_connection()
            .expect("connection")
            .execute_batch(&format!(
                "CREATE TRIGGER reject_migration BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT, 'blocked'); END;"
            ))
            .expect("failure trigger");

        assert_eq!(
            repository.import_migration(&snapshot()),
            Err(RepositoryError::Conflict),
            "table {table}"
        );
        assert!(repository.list_workspaces().expect("workspaces").is_empty());
    }
}

#[test]
fn import_maps_repository_occupancy_query_failure() {
    let (_temporary, repository) = repository();
    repository
        .test_connection()
        .expect("connection")
        .execute_batch("DROP TABLE workspaces")
        .expect("drop table");

    assert_eq!(
        repository.import_migration(&snapshot()),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn snapshot_validation_rejects_each_nested_text_and_storage_key() {
    let mutations: [fn(&mut MigrationSnapshot); 7] = [
        |value| value.workspaces[0].name.clear(),
        |value| value.projects[0].name.clear(),
        |value| value.objects[0].filename.clear(),
        |value| value.objects[0].storage_key.clear(),
        |value| value.shares[0].id.clear(),
        |value| value.retention[0].project_id.clear(),
        |value| value.retention[0].path_glob = Some("invalid\npath".to_owned()),
    ];
    for mutate in mutations {
        let mut invalid = snapshot();
        mutate(&mut invalid);
        assert_eq!(
            validate_snapshot(&invalid),
            Err(RepositoryError::InvalidInput)
        );
    }
}

#[test]
fn share_and_retention_insertions_reject_each_unrepresentable_time() {
    assert_insert_rejections(
        &[
            |value: &mut MigrationSnapshot| value.shares[0].created_at_ms = u64::MAX,
            |value: &mut MigrationSnapshot| value.shares[0].consumed_count = u64::MAX,
        ],
        insert_shares,
    );
    assert_insert_rejections(
        &[
            |value: &mut MigrationSnapshot| value.retention[0].created_at_ms = u64::MAX,
            |value: &mut MigrationSnapshot| value.retention[0].updated_at_ms = u64::MAX,
        ],
        insert_retention,
    );
}

fn assert_insert_rejections(
    mutations: &[fn(&mut MigrationSnapshot)],
    insert: for<'a> fn(
        &rusqlite::Transaction<'a>,
        &MigrationSnapshot,
    ) -> Result<(), RepositoryError>,
) {
    for mutate in mutations {
        let (_temporary, repository) = repository();
        let mut invalid = snapshot();
        mutate(&mut invalid);
        let mut connection = repository.test_connection().expect("connection");
        let transaction = connection.transaction().expect("transaction");
        let result = insert(&transaction, &invalid);
        drop(transaction);
        drop(connection);
        assert_eq!(result, Err(RepositoryError::InvalidInput));
    }
}
