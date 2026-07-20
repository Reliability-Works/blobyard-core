#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    SqliteRepository,
    auth::{query_api_tokens, query_token_revocation},
    metadata::query_projects,
    tests::install_denial,
};
use blobyard_contract::RepositoryError;
use rusqlite::Connection;

#[test]
fn repository_initialization_and_metadata_queries_map_adapter_failures() {
    let connection = Connection::open_in_memory().expect("connection");
    install_denial(&connection, 0);
    assert_eq!(
        SqliteRepository::initialize_connection(connection).err(),
        Some(RepositoryError::Unavailable)
    );

    let connection = Connection::open_in_memory().expect("connection");
    let mut statement = connection
        .prepare("SELECT ?1, ?2")
        .expect("wrong statement");
    assert_eq!(
        query_projects(&mut statement, "workspace").err(),
        Some(RepositoryError::Unavailable)
    );

    let mut statement = connection.prepare("SELECT ?1").expect("wrong statement");
    assert_eq!(
        query_api_tokens(&mut statement).err(),
        Some(RepositoryError::Unavailable)
    );

    for sql in [
        "SELECT 1, 0 WHERE ?1 IS NOT NULL",
        "SELECT 'workspace_fixture', 'invalid' WHERE ?1 IS NOT NULL",
    ] {
        let mut statement = connection.prepare(sql).expect("wrong token row");
        assert_eq!(
            query_token_revocation(&mut statement, "token_fixture").err(),
            Some(RepositoryError::Unavailable)
        );
    }
}
