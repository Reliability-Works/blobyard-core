//! Public standalone initialization and retention failure contracts.

#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use blobyard_core::SecretString;
use blobyard_server::{
    ServerError, StorageConfiguration, YardOidcConfiguration, enforce_retention, initialize,
    initialize_with_origin, initialize_with_origins, serve_until_with_storage_and_oidc,
};
use rusqlite::Connection;

#[test]
fn initialization_classifies_each_blocked_durable_path() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let blocked_root = temporary.path().join("blocked-root");
    std::fs::write(&blocked_root, b"file").expect("root blocker");
    assert_eq!(
        initialize(&blocked_root).err(),
        Some(ServerError::DataDirectory)
    );

    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::create_dir(temporary.path().join("metadata.sqlite3")).expect("metadata blocker");
    assert!(matches!(
        initialize(temporary.path()),
        Err(ServerError::Repository(_))
    ));

    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::write(temporary.path().join("objects"), b"file").expect("storage blocker");
    assert_eq!(
        initialize(temporary.path()).err(),
        Some(ServerError::Storage)
    );

    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::write(temporary.path().join("staging"), b"file").expect("staging blocker");
    assert_eq!(
        initialize(temporary.path()).err(),
        Some(ServerError::DataDirectory)
    );

    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::write(temporary.path().join("runtime.secret"), b"").expect("empty secret");
    assert_eq!(
        initialize(temporary.path()).err(),
        Some(ServerError::Initialization)
    );

    let temporary = tempfile::tempdir().expect("temporary directory");
    std::fs::create_dir(temporary.path().join("runtime.secret")).expect("secret blocker");
    assert_eq!(
        initialize(temporary.path()).err(),
        Some(ServerError::DataDirectory)
    );
}

#[test]
fn initialization_rejects_unsafe_transfer_origins_before_issuing_authority() {
    for origin in [
        "not a URL",
        "ftp://example.com/",
        "https://user@example.com/",
        "https://example.com/path",
        "https://example.com/?query=1",
        "https://example.com/#fragment",
    ] {
        let temporary = tempfile::tempdir().expect("temporary directory");
        assert_eq!(
            initialize_with_origin(temporary.path(), origin).err(),
            Some(ServerError::PublicOrigin)
        );
        assert!(!temporary.path().join("runtime.secret").exists());
    }
}

#[test]
fn initialization_rejects_unsafe_web_yard_origins_before_issuing_authority() {
    for origin in [
        "http://yards.example.com",
        "https://127.0.0.1",
        "https://user@yards.example.com",
        "https://yards.example.com/path",
    ] {
        let temporary = tempfile::tempdir().expect("temporary directory");
        assert_eq!(
            initialize_with_origins(temporary.path(), "http://127.0.0.1:8787", origin,).err(),
            Some(ServerError::WebYardOrigin)
        );
        assert!(!temporary.path().join("runtime.secret").exists());
    }
}

#[test]
fn retention_classifies_adapter_and_metadata_failures() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    initialize(temporary.path()).expect("initialize metadata");
    Connection::open(temporary.path().join("metadata.sqlite3"))
        .expect("database")
        .execute_batch("DROP TABLE retention_policies;")
        .expect("remove retention table");
    assert!(matches!(
        enforce_retention(temporary.path()),
        Err(ServerError::Repository(_))
    ));

    let temporary = tempfile::tempdir().expect("temporary directory");
    Connection::open(temporary.path().join("metadata.sqlite3"))
        .expect("database")
        .execute_batch("CREATE TABLE fixture (id INTEGER);")
        .expect("create metadata");
    std::fs::write(temporary.path().join("objects"), b"file").expect("storage blocker");
    assert_eq!(
        enforce_retention(temporary.path()),
        Err(ServerError::Storage)
    );
}

#[test]
fn retention_rejects_enabled_policies_without_a_project() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    initialize(temporary.path()).expect("initialize metadata");
    Connection::open(temporary.path().join("metadata.sqlite3"))
        .expect("database")
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             INSERT INTO retention_policies (project_id, keep_latest, enabled, created_at_ms, updated_at_ms) VALUES ('project_missing', 1, 1, 1, 1);",
        )
        .expect("orphaned policy");

    assert!(matches!(
        enforce_retention(temporary.path()),
        Err(ServerError::Repository(_))
    ));
}

#[tokio::test]
async fn oidc_discovery_failure_precedes_listener_binding() {
    let unused = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("unused provider listener");
    let issuer = format!(
        "http://{}/",
        unused.local_addr().expect("unused provider address")
    );
    drop(unused);
    let configuration = YardOidcConfiguration::from_optional(
        Some(issuer),
        Some("fixture-client".to_owned()),
        SecretString::new("fixture-secret").ok(),
    )
    .expect("configuration")
    .expect("enabled OIDC");
    assert!(
        serve_until_with_storage_and_oidc(
            "127.0.0.1:0".parse().expect("loopback"),
            tempfile::tempdir().expect("data directory").path(),
            Some("http://127.0.0.1:8787"),
            None,
            &StorageConfiguration::Filesystem,
            Some(&configuration),
            Box::pin(async {}),
        )
        .await
        .is_err()
    );
}
