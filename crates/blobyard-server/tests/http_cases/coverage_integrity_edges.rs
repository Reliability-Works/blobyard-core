use crate::support::{
    AuthorizedServer, assert_internal, authorized_server, complete_fixture_upload,
    create_fixture_project, reserve_fixture_upload, send, send_range, transfer_path,
};
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::json;

fn assert_not_found(status: StatusCode, body: &serde_json::Value) {
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    assert_eq!(
        body["error"]["message"],
        "That item couldn't be found. Check the name and try again."
    );
}

async fn completed_download(
    server: &AuthorizedServer,
    idempotency: &str,
    path: &str,
) -> (String, String) {
    let reservation = reserve_fixture_upload(server, idempotency, path).await;
    let upload_id = reservation["data"]["uploadId"].as_str().expect("upload ID");
    let completed = complete_fixture_upload(server, &reservation).await;
    let grant = send(
        &server.router,
        "POST",
        "/v1/downloads/request",
        Some(json!({ "uri": completed["data"]["uri"] })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(grant.0, StatusCode::OK);
    let storage_key = Connection::open(server.temporary.path().join("metadata.sqlite3"))
        .expect("database")
        .query_row(
            "SELECT storage_key FROM object_versions WHERE id = ?1",
            [upload_id],
            |row| row.get(0),
        )
        .expect("storage key");
    (transfer_path(&grant.1, "downloadUrl"), storage_key)
}

fn object_path(server: &AuthorizedServer, storage_key: &str) -> std::path::PathBuf {
    server
        .temporary
        .path()
        .join("objects/objects")
        .join(storage_key)
}

#[tokio::test]
async fn corrupt_abort_storage_keys_are_internal() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let reservation = reserve_fixture_upload(&server, "corrupt-storage-key", "corrupt.txt").await;
    let upload_id = reservation["data"]["uploadId"].as_str().expect("upload ID");
    Connection::open(server.temporary.path().join("metadata.sqlite3"))
        .expect("database")
        .execute(
            "UPDATE object_versions SET storage_key = '../abort-invalid' WHERE id = ?1",
            [upload_id],
        )
        .expect("corrupt storage key");
    let aborted = send(
        &server.router,
        "POST",
        "/v1/uploads/abort",
        Some(json!({ "uploadId": upload_id })),
        Some(&server.access_token),
    )
    .await;
    assert_internal(aborted.0, &aborted.1);
}

#[tokio::test]
async fn corrupt_download_storage_keys_are_internal() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let reservation = reserve_fixture_upload(&server, "corrupt-download-key", "download.txt").await;
    let upload_id = reservation["data"]["uploadId"].as_str().expect("upload ID");
    let completed = complete_fixture_upload(&server, &reservation).await;
    let grant = send(
        &server.router,
        "POST",
        "/v1/downloads/request",
        Some(json!({ "uri": completed["data"]["uri"] })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(grant.0, StatusCode::OK);
    Connection::open(server.temporary.path().join("metadata.sqlite3"))
        .expect("database")
        .execute(
            "UPDATE object_versions SET storage_key = '../download-invalid' WHERE id = ?1",
            [upload_id],
        )
        .expect("corrupt storage key");
    let downloaded = send_range(
        &server.router,
        &transfer_path(&grant.1, "downloadUrl"),
        None,
    )
    .await;
    let body: serde_json::Value =
        serde_json::from_slice(&downloaded.2).expect("download error JSON");
    assert_internal(downloaded.0, &body);
}

#[tokio::test]
async fn missing_download_bytes_are_not_found() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let (download_url, storage_key) =
        completed_download(&server, "missing-download-bytes", "missing.txt").await;
    std::fs::remove_file(object_path(&server, &storage_key)).expect("remove stored bytes");

    let downloaded = send_range(&server.router, &download_url, None).await;
    let body: serde_json::Value =
        serde_json::from_slice(&downloaded.2).expect("download error JSON");
    assert_not_found(downloaded.0, &body);
}

#[tokio::test]
async fn corrupt_download_bytes_are_internal() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let (download_url, storage_key) =
        completed_download(&server, "corrupt-download-bytes", "corrupt-bytes.txt").await;
    std::fs::write(object_path(&server, &storage_key), b"").expect("truncate stored bytes");

    let downloaded = send_range(&server.router, &download_url, None).await;
    let body: serde_json::Value =
        serde_json::from_slice(&downloaded.2).expect("download error JSON");
    assert_internal(downloaded.0, &body);
}
