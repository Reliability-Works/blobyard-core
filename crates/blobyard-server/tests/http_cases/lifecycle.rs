use crate::support::{
    AuthorizedServer, authorized_server, complete_single_upload, send, send_bytes, send_idempotent,
    transfer_path,
};
use axum::http::StatusCode;
use blobyard_server::{enforce_retention, initialize};
use serde_json::{Value, json};

const HELLO_CHECKSUM: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

#[tokio::test]
async fn delete_removes_bytes_and_metadata_and_persists_redacted_audit() {
    let server = authorized_server().await;
    create_project(&server).await;
    let uri = store_version(&server, "delete/me.txt", "delete-one").await;
    let deleted = send(
        &server.router,
        "DELETE",
        "/v1/objects",
        Some(json!({ "uri": uri })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(deleted.0, StatusCode::OK);
    assert_eq!(deleted.1["data"]["deleted"], true);

    let mut reopened = initialize(server.temporary.path()).expect("reopened server");
    assert!(reopened.take_bootstrap_token().is_none());
    let router = reopened.router();
    let list = list_objects(&router, &server.access_token, true).await;
    assert_eq!(list.0, StatusCode::OK);
    assert_eq!(list.1["data"]["items"], json!([]));
    let audit = send(
        &router,
        "GET",
        "/v1/audit?workspace=default",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(audit.0, StatusCode::OK);
    assert_eq!(audit.1["data"]["items"][0]["action"], "object.deleted");
    let encoded = serde_json::to_string(&audit.1).expect("audit JSON");
    assert!(!encoded.contains(&server.access_token));
    assert!(!encoded.contains(&server.bootstrap_token));
}

#[tokio::test]
async fn delete_rejects_pending_and_retention_enforces_only_eligible_versions() {
    let server = authorized_server().await;
    create_project(&server).await;
    let pending = reserve(&server, "pending.txt", "pending-delete").await;
    let blocked = send(
        &server.router,
        "DELETE",
        "/v1/objects",
        Some(json!({ "uri": "blobyard://default/documentation/pending.txt" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(blocked.0, StatusCode::CONFLICT);
    assert_eq!(blocked.1["error"]["code"], "CONFLICT");
    assert!(pending.1["data"]["uploadUrl"].is_string());

    store_version(&server, "retained/build.txt", "retention-one").await;
    store_version(&server, "retained/build.txt", "retention-two").await;
    let set = send(
        &server.router,
        "PUT",
        "/v1/retention",
        Some(json!({
            "workspace": "default",
            "project": "documentation",
            "keepLatest": 1,
            "path": "retained/**"
        })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(set.0, StatusCode::OK);
    assert_eq!(set.1["data"]["keepLatest"], 1);
    enforce_retention(server.temporary.path()).expect("retention enforcement");

    let mut reopened = initialize(server.temporary.path()).expect("reopened server");
    assert!(reopened.take_bootstrap_token().is_none());
    let list = list_objects(&reopened.router(), &server.access_token, true).await;
    assert_eq!(list.0, StatusCode::OK);
    let retained = list.1["data"]["items"]
        .as_array()
        .expect("object list")
        .iter()
        .filter(|item| {
            item["uri"]
                .as_str()
                .is_some_and(|uri| uri.contains("retained/build.txt"))
        })
        .collect::<Vec<_>>();
    assert_eq!(retained.len(), 1);
    assert!(
        retained[0]["uri"]
            .as_str()
            .is_some_and(|uri| uri.ends_with("version=2"))
    );
    let overview = send(
        &reopened.router(),
        "GET",
        "/v1/retention/overview?workspace=default&project=documentation",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(overview.0, StatusCode::OK);
    assert_eq!(overview.1["data"]["lastRun"]["status"], "complete");
    assert_eq!(overview.1["data"]["lastRun"]["deletedCount"], 1);
}

pub(crate) async fn create_project(server: &AuthorizedServer) {
    let response = send(
        &server.router,
        "POST",
        "/v1/projects",
        Some(json!({ "workspace": "default", "name": "Documentation" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(response.0, StatusCode::OK);
}

pub(crate) async fn store_version(server: &AuthorizedServer, path: &str, key: &str) -> String {
    let reservation = reserve(server, path, key).await;
    assert_eq!(reservation.0, StatusCode::OK);
    let upload = send_bytes(
        &server.router,
        "PUT",
        &transfer_path(&reservation.1, "uploadUrl"),
        b"hello".to_vec(),
        None,
        None,
        Some("text/plain"),
    )
    .await;
    assert_eq!(upload.0, StatusCode::NO_CONTENT);
    let upload_id = reservation.1["data"]["uploadId"]
        .as_str()
        .expect("upload ID");
    let complete = complete_single_upload(&server.router, &server.access_token, upload_id).await;
    assert_eq!(complete.0, StatusCode::OK);
    complete.1["data"]["uri"].as_str().expect("URI").to_owned()
}

async fn reserve(server: &AuthorizedServer, path: &str, key: &str) -> (StatusCode, Value) {
    send_idempotent(
        &server.router,
        "POST",
        "/v1/uploads/request",
        Some(json!({
            "workspace": "default",
            "project": "documentation",
            "path": path,
            "filename": "hello.txt",
            "sizeBytes": 5,
            "checksumSha256": HELLO_CHECKSUM,
            "contentType": "text/plain"
        })),
        Some(&server.access_token),
        Some(key),
    )
    .await
}

pub(crate) async fn list_objects(
    router: &axum::Router,
    token: &str,
    versions: bool,
) -> (StatusCode, Value) {
    send(
        router,
        "GET",
        &format!("/v1/objects?workspace=default&project=documentation&versions={versions}"),
        None,
        Some(token),
    )
    .await
}
