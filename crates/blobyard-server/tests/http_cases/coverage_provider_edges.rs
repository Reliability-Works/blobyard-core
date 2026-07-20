use crate::support::{
    assert_internal, authorized_server, complete_fixture_upload, create_fixture_project,
    reserve_fixture_upload, send, send_idempotent, upload_fixture_bytes,
};
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::json;

#[tokio::test]
async fn upload_completion_and_abort_audit_failures_are_internal() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let complete = reserve_fixture_upload(&server, "complete-audit", "complete.txt").await;
    let abort = reserve_fixture_upload(&server, "abort-audit", "abort.txt").await;
    upload_fixture_bytes(&server, &complete).await;
    Connection::open(server.temporary.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute_batch("DROP TABLE audit_events;")
        .expect("drop audit table");
    let complete_id = complete["data"]["uploadId"].as_str().expect("upload ID");
    let completed = send(
        &server.router,
        "POST",
        "/v1/uploads/complete",
        Some(json!({ "uploadId": complete_id, "parts": [] })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(completed.0, StatusCode::INTERNAL_SERVER_ERROR);
    let abort_id = abort["data"]["uploadId"].as_str().expect("upload ID");
    let aborted = send(
        &server.router,
        "POST",
        "/v1/uploads/abort",
        Some(json!({ "uploadId": abort_id })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(aborted.0, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn upload_abort_storage_failure_is_internal() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let blocked = reserve_fixture_upload(&server, "abort-storage", "blocked.txt").await;
    let upload_id = blocked["data"]["uploadId"].as_str().expect("upload ID");
    let connection =
        Connection::open(server.temporary.path().join("metadata.sqlite3")).expect("connection");
    let storage_key: String = connection
        .query_row(
            "SELECT v.storage_key FROM object_versions v JOIN upload_reservations r ON r.version_id = v.id WHERE r.id = ?1",
            [upload_id],
            |row| row.get(0),
        )
        .expect("storage key");
    std::fs::create_dir_all(
        server
            .temporary
            .path()
            .join("objects/objects")
            .join(storage_key),
    )
    .expect("storage blocker");
    let aborted = send(
        &server.router,
        "POST",
        "/v1/uploads/abort",
        Some(json!({ "uploadId": upload_id })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(aborted.0, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn corrupt_completion_metadata_is_internal() {
    for (index, corruption) in [
        "UPDATE object_versions SET version = 0 WHERE id = ?1",
        "UPDATE object_versions SET object_path = '/absolute' WHERE id = ?1",
    ]
    .into_iter()
    .enumerate()
    {
        let server = authorized_server().await;
        create_fixture_project(&server).await;
        let reservation =
            reserve_fixture_upload(&server, &format!("corrupt-complete-{index}"), "corrupt.txt")
                .await;
        let upload_id = reservation["data"]["uploadId"].as_str().expect("upload ID");
        upload_fixture_bytes(&server, &reservation).await;
        let connection =
            Connection::open(server.temporary.path().join("metadata.sqlite3")).expect("database");
        connection
            .execute_batch("PRAGMA ignore_check_constraints = ON")
            .expect("disable fixture checks");
        connection
            .execute(corruption, [upload_id])
            .expect("corrupt object version");
        let response = send(
            &server.router,
            "POST",
            "/v1/uploads/complete",
            Some(json!({ "uploadId": upload_id, "parts": [] })),
            Some(&server.access_token),
        )
        .await;
        assert_eq!(
            response.0,
            StatusCode::INTERNAL_SERVER_ERROR,
            "corruption case {index}"
        );
        assert_internal(response.0, &response.1);
    }
}

#[tokio::test]
async fn repository_failures_during_upload_audit_and_retention_are_internal() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let database = server.temporary.path().join("metadata.sqlite3");
    Connection::open(&database)
        .expect("connection")
        .execute_batch("DROP TABLE audit_events;")
        .expect("drop audit table");
    let upload = send_idempotent(
        &server.router,
        "POST",
        "/v1/uploads/request",
        Some(json!({
            "workspace": "default", "project": "fixture", "path": "audit.txt",
            "filename": "audit.txt", "sizeBytes": 0, "checksumSha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "contentType": "text/plain"
        })),
        Some(&server.access_token),
        Some("audit-failure"),
    )
    .await;
    assert_eq!(upload.0, StatusCode::INTERNAL_SERVER_ERROR);

    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let connection =
        Connection::open(server.temporary.path().join("metadata.sqlite3")).expect("connection");
    let audit_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("audit count");
    connection
        .execute_batch("DROP TABLE retention_policies;")
        .expect("drop retention table");
    let lookup = send(
        &server.router,
        "GET",
        "/v1/retention?workspace=default&project=fixture",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_internal(lookup.0, &lookup.1);
    let retention = send(
        &server.router,
        "PUT",
        "/v1/retention",
        Some(json!({
            "workspace": "default", "project": "fixture", "keepLatest": 1
        })),
        Some(&server.access_token),
    )
    .await;
    assert_internal(retention.0, &retention.1);
    let final_audit_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("final audit count");
    assert_eq!(final_audit_count, audit_count);
}

#[tokio::test]
async fn download_audit_provider_failure_is_internal() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let reservation = reserve_fixture_upload(&server, "download-audit", "download.txt").await;
    let completed = complete_fixture_upload(&server, &reservation).await;
    Connection::open(server.temporary.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute_batch("DROP TABLE audit_events;")
        .expect("drop audit table");
    let grant = send(
        &server.router,
        "POST",
        "/v1/downloads/request",
        Some(json!({ "uri": completed["data"]["uri"] })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(grant.0, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn retention_enforcement_provider_failures_are_internal() {
    for failure in ["begin", "finish"] {
        let server = authorized_server().await;
        create_fixture_project(&server).await;
        let set = send(
            &server.router,
            "PUT",
            "/v1/retention",
            Some(json!({
                "workspace": "default", "project": "fixture", "keepLatest": 1
            })),
            Some(&server.access_token),
        )
        .await;
        assert_eq!(set.0, StatusCode::OK);
        let connection =
            Connection::open(server.temporary.path().join("metadata.sqlite3")).expect("connection");
        if failure == "begin" {
            connection
                .execute_batch("DROP TABLE deletion_operations;")
                .expect("drop operations");
        } else {
            connection
                .execute_batch(
                    "CREATE TRIGGER fail_finish BEFORE UPDATE OF status ON deletion_operations WHEN NEW.status = 'complete' BEGIN SELECT RAISE(ABORT, 'fixture'); END;",
                )
                .expect("failure trigger");
        }
        assert!(blobyard_server::enforce_retention(server.temporary.path()).is_err());
    }
}
