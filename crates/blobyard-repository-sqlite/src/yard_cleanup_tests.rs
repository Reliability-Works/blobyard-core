#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{super::SqliteRepository, pending};
use blobyard_contract::RepositoryError;
use rusqlite::{Connection, params_from_iter, types::Value};

fn connection_with_headers(headers: Vec<Value>) -> Connection {
    let connection = Connection::open_in_memory().expect("connection");
    connection
        .execute_batch(
            "CREATE TABLE deletion_operations (id, project_id, object_path, reason, status, actor, request_id, created_at_ms);
             CREATE TABLE yard_deploys (id, yard_id, workspace_id, project_id, manifest_root);
             CREATE TABLE deletion_items (operation_id, version_id, storage_key, version);",
        )
        .expect("schema");
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
    connection
}

#[test]
fn pending_cleanup_rejects_each_malformed_header_column() {
    for invalid in 0..4 {
        let headers = (0..4)
            .map(|index| {
                if index == invalid {
                    Value::Blob(vec![1])
                } else {
                    Value::Text(format!("header-{index}"))
                }
            })
            .collect();
        assert_eq!(
            pending(&connection_with_headers(headers), None),
            Err(RepositoryError::Unavailable)
        );
    }
}

#[test]
fn pending_cleanup_maps_query_binding_failures() {
    let connection = Connection::open_in_memory().expect("connection");
    let mut statement = connection.prepare("SELECT 1").expect("statement");
    assert_eq!(
        SqliteRepository::test_yard_cleanup_query(&mut statement, Some("yard")),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn pending_cleanup_maps_statement_preparation_failures() {
    assert_eq!(
        pending(&Connection::open_in_memory().expect("connection"), None),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn pending_cleanup_propagates_a_missing_deletion_plan() {
    let connection = connection_with_headers(vec![
        Value::Text("operation".to_owned()),
        Value::Text("deploy".to_owned()),
        Value::Text("yard".to_owned()),
        Value::Text("workspace".to_owned()),
    ]);
    connection
        .execute("DROP TABLE deletion_items", [])
        .expect("remove deletion items");
    assert_eq!(
        pending(&connection, None),
        Err(RepositoryError::Unavailable)
    );
}
