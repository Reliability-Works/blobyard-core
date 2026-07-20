#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    DatabaseInspection, current_schema_version, finish_inspection, inspect_connection,
    inspect_database, oldest_supported_schema_version, snapshot_database, validate_copied_snapshot,
    validate_integrity, validate_snapshot,
};
use blobyard_contract::RepositoryError;
use rusqlite::Connection;

#[test]
fn inspection_and_online_snapshot_preserve_the_exact_schema() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let source = temporary.path().join("source.sqlite3");
    let destination = temporary.path().join("snapshot.sqlite3");
    drop(crate::SqliteRepository::open(&source).expect("repository"));

    let expected = DatabaseInspection {
        schema_version: current_schema_version(),
    };
    assert_eq!(oldest_supported_schema_version(), 1);
    assert_eq!(inspect_database(&source), Ok(expected));
    assert_eq!(snapshot_database(&source, &destination), Ok(expected));
    assert_eq!(inspect_database(&destination), Ok(expected));
}

#[test]
fn inspection_and_snapshot_fail_closed_for_invalid_inputs() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let missing = temporary.path().join("missing.sqlite3");
    assert_eq!(
        inspect_database(&missing),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        snapshot_database(&missing, &temporary.path().join("missing-copy.sqlite3")),
        Err(RepositoryError::Unavailable)
    );

    let corrupt = temporary.path().join("corrupt.sqlite3");
    std::fs::write(&corrupt, b"not sqlite").expect("corrupt fixture");
    assert_eq!(
        inspect_database(&corrupt),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        snapshot_database(&corrupt, &temporary.path().join("corrupt-copy.sqlite3")),
        Err(RepositoryError::Unavailable)
    );

    let source = temporary.path().join("future.sqlite3");
    let connection = Connection::open(&source).expect("future database");
    connection
        .pragma_update(None, "user_version", current_schema_version() + 1)
        .expect("future version");
    drop(connection);
    assert_eq!(
        snapshot_database(&source, &temporary.path().join("future-copy.sqlite3")),
        Err(RepositoryError::SchemaTooNew)
    );

    let valid = temporary.path().join("valid.sqlite3");
    drop(crate::SqliteRepository::open(&valid).expect("repository"));
    let blocked = temporary.path().join("blocked");
    std::fs::create_dir(&blocked).expect("blocked destination");
    assert_eq!(
        snapshot_database(&valid, &blocked),
        Err(RepositoryError::Unavailable)
    );
}

#[test]
fn integrity_results_fail_closed() {
    let connection = Connection::open_in_memory().expect("database");
    assert_eq!(
        inspect_connection(&connection),
        Ok(DatabaseInspection { schema_version: 0 })
    );
    assert_eq!(validate_integrity("ok"), Ok(()));
    assert_eq!(
        validate_integrity("malformed page"),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        validate_snapshot(
            DatabaseInspection { schema_version: 1 },
            DatabaseInspection { schema_version: 2 },
        ),
        Err(RepositoryError::Unavailable)
    );

    assert_eq!(
        finish_inspection("malformed page", Ok(1)),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        finish_inspection("ok", Err(RepositoryError::Unavailable)),
        Err(RepositoryError::Unavailable)
    );

    let temporary = tempfile::tempdir().expect("temporary directory");
    let corrupt = temporary.path().join("corrupt-copy.sqlite3");
    std::fs::write(&corrupt, b"not sqlite").expect("corrupt fixture");
    assert_eq!(
        validate_copied_snapshot(&corrupt, DatabaseInspection { schema_version: 1 }),
        Err(RepositoryError::Unavailable)
    );
}
