#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{inspect, rollback, upgrade};
use crate::recovery::{RecoveryError, test_support};
use rusqlite::Connection;
use std::fs;
use std::os::unix::fs::symlink;

#[test]
fn upgrade_reports_current_and_supported_older_schemas_without_mutation() {
    let source = test_support::installation();
    let current = upgrade(source.path()).expect("current upgrade preflight");
    assert!(!current.migration_required);
    assert!(current.backup_required);
    assert_eq!(
        inspect(source.path()),
        Ok(blobyard_repository_sqlite::current_schema_version())
    );
    let rollback_report = rollback(source.path()).expect("current rollback preflight");
    assert!(rollback_report.code_only_rollback_allowed);

    Connection::open(source.path().join("metadata.sqlite3"))
        .expect("database")
        .pragma_update(
            None,
            "user_version",
            blobyard_repository_sqlite::oldest_supported_schema_version(),
        )
        .expect("older supported schema");
    let older = upgrade(source.path()).expect("older upgrade preflight");
    assert!(older.migration_required);
    assert_eq!(
        rollback(source.path()).expect_err("older schema must block rollback"),
        RecoveryError::RollbackUnsafe
    );
}

#[test]
fn upgrade_rejects_old_new_corrupt_and_unsafe_installations() {
    let source = test_support::installation();
    let database = source.path().join("metadata.sqlite3");
    Connection::open(&database)
        .expect("database")
        .pragma_update(None, "user_version", 0)
        .expect("old schema");
    assert_eq!(
        upgrade(source.path()).expect_err("old schema must fail"),
        RecoveryError::SchemaTooOld
    );

    Connection::open(&database)
        .expect("database")
        .pragma_update(
            None,
            "user_version",
            blobyard_repository_sqlite::current_schema_version() + 1,
        )
        .expect("new schema");
    assert_eq!(
        upgrade(source.path()).expect_err("new schema must fail"),
        RecoveryError::SchemaTooNew
    );

    fs::write(&database, b"not sqlite").expect("corrupt database");
    assert_eq!(
        upgrade(source.path()).expect_err("corrupt database must fail"),
        RecoveryError::Database
    );
    assert_eq!(
        rollback(source.path()).expect_err("corrupt database must block rollback"),
        RecoveryError::Database
    );

    let missing = tempfile::tempdir().expect("missing parent");
    assert_eq!(
        inspect(&missing.path().join("absent")),
        Err(RecoveryError::InstallationUnavailable)
    );
    let linked = missing.path().join("linked");
    symlink(source.path(), &linked).expect("linked installation");
    assert_eq!(
        inspect(&linked),
        Err(RecoveryError::InstallationUnavailable)
    );
}

#[test]
fn upgrade_requires_one_valid_runtime_secret() {
    for bytes in [b"".as_slice(), &[0xff][..]] {
        let source = test_support::installation();
        fs::write(source.path().join("runtime.secret"), bytes).expect("invalid runtime secret");
        assert_eq!(
            inspect(source.path()),
            Err(RecoveryError::InstallationUnavailable)
        );
    }

    let source = test_support::installation();
    fs::remove_file(source.path().join("runtime.secret")).expect("remove runtime secret");
    assert_eq!(
        inspect(source.path()),
        Err(RecoveryError::InstallationUnavailable)
    );
}
