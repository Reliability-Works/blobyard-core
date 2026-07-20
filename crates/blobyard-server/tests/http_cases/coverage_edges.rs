use crate::support::{
    authorized_server, create_fixture_project, reserve_fixture_upload, send, send_bytes,
    send_idempotent, send_range,
};
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::json;

#[tokio::test]
async fn namespace_and_authentication_edges_fail_closed() {
    let server = authorized_server().await;
    for (path, expected) in [
        ("/missing", StatusCode::NOT_FOUND),
        ("/v1/workspaces?cursor=next", StatusCode::BAD_REQUEST),
        (
            "/v1/projects?workspace=default&cursor=next",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/v1/projects?workspace=Invalid%20Slug",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = send(
            &server.router,
            "GET",
            path,
            None,
            Some(&server.access_token),
        )
        .await;
        assert_eq!(response.0, expected, "{path}");
    }

    for body in [json!({ "name": "" }), json!({ "name": "line\nbreak" })] {
        let response = send(
            &server.router,
            "POST",
            "/v1/workspaces",
            Some(body),
            Some(&server.access_token),
        )
        .await;
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
    }
    let missing_workspace = send(
        &server.router,
        "POST",
        "/v1/projects",
        Some(json!({ "workspace": "missing", "name": "Fixture" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(missing_workspace.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn object_and_download_edges_are_concealed() {
    let server = authorized_server().await;
    for path in [
        "/v1/objects?workspace=default&project=missing&versions=false",
        "/v1/objects?workspace=default&project=missing&versions=false&cursor=next",
        "/v1/objects?workspace=default&project=missing&versions=false&prefix=../",
    ] {
        let response = send(
            &server.router,
            "GET",
            path,
            None,
            Some(&server.access_token),
        )
        .await;
        assert!(matches!(
            response.0,
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
        ));
    }

    for body in [
        json!({ "uri": "not-a-uri" }),
        json!({ "uri": "blobyard://default/missing/file.txt" }),
    ] {
        let response = send(
            &server.router,
            "POST",
            "/v1/downloads/request",
            Some(body),
            Some(&server.access_token),
        )
        .await;
        assert!(matches!(
            response.0,
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
        ));
    }
    let unknown = send_range(
        &server.router,
        "/transfers/downloads/unknown-capability",
        None,
    )
    .await;
    assert_eq!(unknown.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn upload_edges_reject_invalid_authority_and_state() {
    let server = authorized_server().await;
    let project = send(
        &server.router,
        "POST",
        "/v1/projects",
        Some(json!({ "workspace": "default", "name": "Fixture" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(project.0, StatusCode::OK);
    let request = json!({
        "workspace": "default",
        "project": "fixture",
        "path": "file.txt",
        "filename": "file.txt",
        "sizeBytes": 1,
        "checksumSha256": "00".repeat(32),
        "contentType": "text/plain"
    });
    assert_upload_request_edges(&server, &request).await;
    assert_upload_state_edges(&server).await;
}

async fn assert_upload_request_edges(
    server: &crate::support::AuthorizedServer,
    request: &serde_json::Value,
) {
    let missing_key = send(
        &server.router,
        "POST",
        "/v1/uploads/request",
        Some(request.clone()),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(missing_key.0, StatusCode::BAD_REQUEST);

    let mut unknown_request = request.clone();
    unknown_request["project"] = json!("missing");
    let missing_project = send_idempotent(
        &server.router,
        "POST",
        "/v1/uploads/request",
        Some(unknown_request),
        Some(&server.access_token),
        Some("fixture"),
    )
    .await;
    assert_eq!(missing_project.0, StatusCode::NOT_FOUND);
}

async fn assert_upload_state_edges(server: &crate::support::AuthorizedServer) {
    for body in [
        json!({ "uploadId": "missing", "parts": [{ "partNumber": 1, "etag": "x" }] }),
        json!({ "uploadId": "missing", "parts": [] }),
    ] {
        let response = send(
            &server.router,
            "POST",
            "/v1/uploads/complete",
            Some(body),
            Some(&server.access_token),
        )
        .await;
        assert!(matches!(
            response.0,
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
        ));
    }
    let abort = send(
        &server.router,
        "POST",
        "/v1/uploads/abort",
        Some(json!({ "uploadId": "missing" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(abort.0, StatusCode::NOT_FOUND);
    let status = send(
        &server.router,
        "GET",
        "/v1/uploads/status?uploadId=missing",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(status.0, StatusCode::NOT_FOUND);
    let put = send_bytes(
        &server.router,
        "PUT",
        "/transfers/uploads/unknown-capability",
        Vec::new(),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(put.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn requested_upload_status_covers_live_and_expired_reservations() {
    let server = authorized_server().await;
    create_fixture_project(&server).await;
    let reservation = reserve_fixture_upload(&server, "status-fixture", "status.txt").await;
    let upload_id = reservation["data"]["uploadId"].as_str().expect("upload ID");
    let status = send(
        &server.router,
        "GET",
        &format!("/v1/uploads/status?uploadId={upload_id}"),
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(status.1["data"]["state"], "requested");

    Connection::open(server.temporary.path().join("metadata.sqlite3"))
        .expect("connection")
        .execute(
            "UPDATE upload_reservations SET expires_at_ms = 0 WHERE id = ?1",
            [upload_id],
        )
        .expect("expire reservation");
    let status = send(
        &server.router,
        "GET",
        &format!("/v1/uploads/status?uploadId={upload_id}"),
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(status.1["data"]["state"], "expired");
}
