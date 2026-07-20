#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    deletion_header_row, item_row, query_deletion_items, query_selected_versions, selected_row,
};
use blobyard_contract::RepositoryError;
use rusqlite::Connection;

#[test]
fn deletion_rows_reject_each_malformed_provider_field() {
    let connection = Connection::open_in_memory().expect("connection");
    for query in [
        "SELECT 'bad', 'actor', 'request'",
        "SELECT 0, 1, 'request'",
        "SELECT 0, 'actor', 1",
    ] {
        assert!(
            connection
                .query_row(query, [], deletion_header_row)
                .is_err()
        );
    }
    for query in [
        "SELECT 1, 'key', 1, 'complete'",
        "SELECT 'id', 1, 1, 'complete'",
        "SELECT 'id', 'key', 'bad', 'complete'",
        "SELECT 'id', 'key', 1, 2",
        "SELECT 'id', 'key', -1, 'complete'",
    ] {
        assert!(connection.query_row(query, [], selected_row).is_err());
    }
    for query in [
        "SELECT 1, 'key', 1",
        "SELECT 'id', 1, 1",
        "SELECT 'id', 'key', 'bad'",
        "SELECT 'id', 'key', -1",
    ] {
        assert!(connection.query_row(query, [], item_row).is_err());
    }
}

#[test]
fn deletion_queries_map_parameter_failures() {
    let connection = Connection::open_in_memory().expect("connection");
    let mut statement = connection.prepare("SELECT 1").expect("wrong statement");
    assert_eq!(
        query_deletion_items(&mut statement, "operation").err(),
        Some(RepositoryError::Unavailable)
    );
    assert_eq!(
        query_selected_versions(&mut statement, "project", "path", None).err(),
        Some(RepositoryError::Unavailable)
    );
}
