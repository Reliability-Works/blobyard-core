#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Yard-cleanup query failure coverage through the integration-built adapter.

use blobyard_contract::{RepositoryError, WebYardRepository};
use blobyard_repository_sqlite::SqliteRepository;
use rusqlite::{Connection, params_from_iter, types::Value};

fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let repository =
        SqliteRepository::open(&temporary.path().join("metadata.sqlite3")).expect("repository");
    (temporary, repository)
}

fn install_header_tables(repository: &SqliteRepository, headers: Vec<Value>) {
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch(
            "CREATE TEMP TABLE deletion_operations (id, project_id, object_path, reason, status, actor, request_id, created_at_ms);
             CREATE TEMP TABLE yard_deploys (id, yard_id, workspace_id, project_id, manifest_root);",
        )
        .expect("temporary schema");
    connection
        .execute(
            "INSERT INTO deletion_operations VALUES (?1, 'project', 'manifest/', 'yard_cleanup', 'pending', 'actor', 'request', 1)",
            [&headers[0]],
        )
        .expect("operation");
    connection
        .execute(
            "INSERT INTO yard_deploys VALUES (?1, ?2, ?3, 'project', 'manifest/')",
            params_from_iter(headers.into_iter().skip(1)),
        )
        .expect("deploy");
}

#[test]
fn cleanup_query_binding_and_prepare_failures_are_unavailable() {
    let connection = Connection::open_in_memory().expect("connection");
    let mut invalid_binding = connection.prepare("SELECT 1").expect("statement");
    assert_eq!(
        SqliteRepository::test_yard_cleanup_query(&mut invalid_binding, Some("yard")),
        Err(RepositoryError::Unavailable)
    );

    let (_temporary, repository) = repository();
    repository
        .test_connection()
        .expect("connection")
        .execute("DROP TABLE deletion_operations", [])
        .expect("break query preparation");
    assert_eq!(
        repository.pending_yard_cleanups(None),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn cleanup_header_and_plan_failures_are_unavailable() {
    for invalid in 0..4 {
        let (_temporary, repository) = repository();
        let headers = (0..4)
            .map(|index| {
                if index == invalid {
                    Value::Blob(vec![1])
                } else {
                    Value::Text(format!("header-{index}"))
                }
            })
            .collect();
        install_header_tables(&repository, headers);
        assert_eq!(
            repository.pending_yard_cleanups(None),
            Err(RepositoryError::Unavailable)
        );
    }

    let (_temporary, repository) = repository();
    install_header_tables(
        &repository,
        vec![
            Value::Text("operation".to_owned()),
            Value::Text("deploy".to_owned()),
            Value::Text("yard".to_owned()),
            Value::Text("workspace".to_owned()),
        ],
    );
    repository
        .test_connection()
        .expect("connection")
        .execute("DROP TABLE deletion_items", [])
        .expect("break deletion plan");
    assert_eq!(
        repository.pending_yard_cleanups(None),
        Err(RepositoryError::Unavailable)
    );
}
