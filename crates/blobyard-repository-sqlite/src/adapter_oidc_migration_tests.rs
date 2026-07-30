#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{SqliteRepository, assert_tables};
use blobyard_contract::RepositoryError;
use rusqlite::Connection;
use std::path::Path;

#[test]
fn oidc_migration_normalizes_local_user_emails_and_adds_constrained_tables() {
    use blobyard_contract::{LocalUserRepository, MetadataRepository};

    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    version_twenty_three_local_users(
        &path,
        "('user_one', 'workspace', 'User One', ' Mixed@Example.test ', 'active', 2, NULL),
         ('user_two', 'workspace', 'User Two', 'second@example.test', 'active', 1, NULL),
         ('user_gone', 'workspace', 'User Gone', ' MIXED@example.TEST ', 'deactivated', 1, 2)",
    );

    let repository = SqliteRepository::open(&path).expect("migrated repository");
    assert_eq!(repository.schema_version().expect("schema version"), 24);
    let users = repository.list_local_users("workspace").expect("users");
    let emails = users
        .iter()
        .map(|listing| (listing.user.id.as_str(), listing.user.email.as_deref()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(emails.get("user_one"), Some(&Some("mixed@example.test")));
    assert_eq!(emails.get("user_two"), Some(&Some("second@example.test")));
    assert_eq!(emails.get("user_gone"), Some(&Some("mixed@example.test")));
    assert_tables(&repository, &["yard_oidc_identities", "yard_oidc_attempts"]);
    assert_oidc_constraints(&repository);
}

fn version_twenty_three_local_users(path: &Path, values: &str) {
    let mut connection = Connection::open(path).expect("version twenty-three connection");
    super::super::migrations::apply_through(&mut connection, 23)
        .expect("version twenty-three schema");
    connection
        .execute_batch(&format!(
            "INSERT INTO workspaces (id, name, slug) VALUES ('workspace', 'Workspace', 'workspace');
             INSERT INTO local_users
               (id, workspace_id, display_name, email, status, created_at_ms, deactivated_at_ms)
             VALUES {values};"
        ))
        .expect("version twenty-three fixture");
    drop(connection);
}

fn assert_oidc_constraints(repository: &SqliteRepository) {
    let connection = repository.test_connection().expect("connection");
    connection
        .execute_batch(
            "INSERT INTO yard_subjects
               (id, kind, workspace_id, local_user_id, invitation_id, created_at_ms, revoked_at_ms)
             VALUES ('user_one', 'member', 'workspace', 'user_one', NULL, 2, NULL);",
        )
        .expect("member subject");
    connection
        .execute(
            "INSERT INTO yard_oidc_identities VALUES
               ('https://identity.example.test/', 'subject', 'workspace', 'user_one',
                'mixed@example.test', 1, 1)",
            [],
        )
        .expect("valid identity");
    for invalid in [
        "INSERT INTO yard_oidc_identities VALUES
           ('https://identity.example.test/', 'subject-two', 'workspace', 'user_one',
            'Mixed@example.test', 1, 1)",
        "INSERT INTO yard_oidc_identities VALUES
           ('https://identity.example.test/', 'subject-three', 'workspace', 'user_one',
            'second@example.test', 2, 1)",
        "INSERT INTO yard_oidc_identities VALUES
           ('https://identity.example.test/', 'subject-four', 'foreign', 'user_one',
            'second@example.test', 1, 1)",
        "INSERT INTO yard_oidc_attempts
           (state_hash, continuation_hash, host_label, return_path, created_at_ms, expires_at_ms)
         VALUES
           ('short', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
            'host', '/', 1, 2)",
    ] {
        assert!(connection.execute(invalid, []).is_err(), "{invalid}");
    }
    drop(connection);
}

#[test]
fn oidc_migration_rejects_active_local_users_with_colliding_normalized_emails() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let path = temporary.path().join("metadata.sqlite3");
    version_twenty_three_local_users(
        &path,
        "('user_one', 'workspace', 'User One', 'Person@example.test', 'active', 1, NULL),
         ('user_two', 'workspace', 'User Two', ' person@example.test ', 'active', 2, NULL)",
    );

    assert_eq!(
        SqliteRepository::open(&path).err(),
        Some(RepositoryError::Unavailable),
        "ambiguous normalized emails must fail the upgrade closed"
    );
    let connection = Connection::open(&path).expect("rolled back connection");
    assert_eq!(
        super::super::migrations::schema_version(&connection).expect("schema version"),
        23,
        "a failed migration leaves the prior schema version untouched"
    );
}
