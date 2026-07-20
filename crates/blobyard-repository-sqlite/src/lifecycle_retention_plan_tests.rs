#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{Candidate, candidate_row, glob_at, glob_matches, policy_matches, protected_path};
use blobyard_contract::{RepositoryError, RetentionPolicyRecord};
use rusqlite::Connection;
use std::collections::HashMap;

#[test]
fn glob_matching_is_segment_aware_and_covers_recursive_patterns() {
    assert!(glob_matches("artifacts/**", "artifacts/release/app.zip"));
    assert!(glob_matches("artifacts/*.zip", "artifacts/app.zip"));
    assert!(!glob_matches(
        "artifacts/*.zip",
        "artifacts/release/app.zip"
    ));
    assert!(glob_matches("release-?", "release-a"));
    assert!(!glob_matches("release-?", "release/a"));
    assert!(!glob_matches("release-?", "release-ab"));
    assert!(!glob_matches("artifacts/**/app.zip", "other/app.zip"));
    assert!(glob_matches("**/**", "nested/path"));
    assert!(!glob_matches("exact", "exactly"));

    let mut memo = HashMap::from([((0, 0), true)]);
    assert!(glob_at(&['x'], &['y'], 0, 0, &mut memo));
}

#[test]
fn policy_matching_requires_branch_provenance_and_protects_internal_paths() {
    let policy = RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: Some("artifacts/**".to_owned()),
        branch_glob: Some("release-*".to_owned()),
        created_at_ms: 1,
        updated_at_ms: 1,
    };
    let mut candidate = Candidate {
        id: "version_fixture".to_owned(),
        path: "artifacts/app.zip".to_owned(),
        storage_key: "objects/version_fixture".to_owned(),
        version: 1,
        git_branch: None,
    };
    assert!(!policy_matches(&policy, &candidate));
    candidate.git_branch = Some("release-main".to_owned());
    assert!(policy_matches(&policy, &candidate));
    candidate.path = "logs/app.zip".to_owned();
    assert!(!policy_matches(&policy, &candidate));
    assert!(protected_path(".blobyard-preview/index.html"));
    assert!(protected_path(".blobyard-yard/index.html"));
    assert!(!protected_path("artifacts/index.html"));
}

#[test]
fn retention_candidate_rows_reject_each_malformed_provider_field() {
    let connection = Connection::open_in_memory().expect("connection");
    for query in [
        "SELECT 1, 'path', 'key', 1, NULL",
        "SELECT 'id', 1, 'key', 1, NULL",
        "SELECT 'id', 'path', 1, 1, NULL",
        "SELECT 'id', 'path', 'key', 'bad', NULL",
        "SELECT 'id', 'path', 'key', 1, 2",
        "SELECT 'id', 'path', 'key', -1, NULL",
    ] {
        assert!(connection.query_row(query, [], candidate_row).is_err());
    }
}

#[test]
fn matching_versions_propagates_query_and_row_failures() {
    let connection = candidate_connection();
    let mut statement = connection.prepare("SELECT 1").expect("wrong statement");
    assert_eq!(
        super::query_candidates(&mut statement, "project").err(),
        Some(RepositoryError::Unavailable)
    );

    let connection = candidate_connection();
    let mut statement = candidate_statement(&connection);
    connection
        .progress_handler(1, Some(|| true))
        .expect("progress handler");
    assert_eq!(
        super::query_candidates(&mut statement, "project").err(),
        Some(RepositoryError::Unavailable)
    );

    let connection = candidate_connection();
    connection
        .execute(
            "INSERT INTO object_versions (id, project_id, object_path, storage_key, version, git_branch, state, created_at_ms) VALUES ('version', 'project', 'path', 'key', -1, NULL, 'complete', 1)",
            [],
            )
            .expect("corrupt candidate");
    let mut statement = candidate_statement(&connection);
    assert_eq!(
        super::query_candidates(&mut statement, "project").err(),
        Some(RepositoryError::Unavailable)
    );
}

fn candidate_connection() -> Connection {
    let connection = Connection::open_in_memory().expect("connection");
    connection
        .execute_batch(
            "CREATE TABLE object_versions (id TEXT, project_id TEXT, object_path TEXT, storage_key TEXT, version INTEGER, git_branch TEXT, state TEXT, created_at_ms INTEGER)",
        )
        .expect("candidate schema");
    connection
}

fn candidate_statement(connection: &Connection) -> rusqlite::Statement<'_> {
    connection
        .prepare(
            "SELECT id, object_path, storage_key, version, git_branch FROM object_versions WHERE project_id = ?1 AND state = 'complete' ORDER BY created_at_ms DESC, id DESC",
        )
        .expect("candidate statement")
}
