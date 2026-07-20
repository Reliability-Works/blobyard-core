use crate::lifecycle::{create_project, list_objects, store_version};
use crate::lifecycle_support::install_scoped_token;
use crate::support::{authorized_server, send, send_json_bytes};
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::{Value, json};

#[tokio::test]
async fn delete_route_enforces_auth_and_body_validation() {
    let server = authorized_server().await;
    create_project(&server).await;
    let body = json!({ "uri": "blobyard://default/documentation/missing.txt" });
    assert_error(
        send(
            &server.router,
            "DELETE",
            "/v1/objects",
            Some(body.clone()),
            None,
        )
        .await,
        StatusCode::UNAUTHORIZED,
    );
    let limited = install_scoped_token(server.temporary.path(), &["object:read"]);
    assert_error(
        send(
            &server.router,
            "DELETE",
            "/v1/objects",
            Some(body.clone()),
            Some(&limited),
        )
        .await,
        StatusCode::FORBIDDEN,
    );
    assert_error(
        send_json_bytes(
            &server.router,
            "DELETE",
            "/v1/objects",
            b"{".to_vec(),
            Some(&server.access_token),
        )
        .await,
        StatusCode::BAD_REQUEST,
    );
    assert_error(
        send(
            &server.router,
            "DELETE",
            "/v1/objects",
            Some(json!({ "uri": "not-a-blobyard-uri" })),
            Some(&server.access_token),
        )
        .await,
        StatusCode::BAD_REQUEST,
    );
}

#[tokio::test]
async fn delete_route_conceals_object_ownership_and_existence() {
    let server = authorized_server().await;
    create_project(&server).await;
    let body = json!({ "uri": "blobyard://default/documentation/missing.txt" });
    assert_error(
        send(
            &server.router,
            "DELETE",
            "/v1/objects",
            Some(body),
            Some(&server.access_token),
        )
        .await,
        StatusCode::NOT_FOUND,
    );
    assert_error(
        send(
            &server.router,
            "DELETE",
            "/v1/objects",
            Some(json!({ "uri": "blobyard://default/missing/object.txt" })),
            Some(&server.access_token),
        )
        .await,
        StatusCode::NOT_FOUND,
    );
}

#[tokio::test]
async fn versioned_delete_preserves_other_versions_and_replays_as_success() {
    let server = authorized_server().await;
    create_project(&server).await;
    let first = store_version(&server, "versioned/object.txt", "versioned-one").await;
    let second = store_version(&server, "versioned/object.txt", "versioned-two").await;
    for _attempt in 0..2 {
        let deleted = send(
            &server.router,
            "DELETE",
            "/v1/objects",
            Some(json!({ "uri": first })),
            Some(&server.access_token),
        )
        .await;
        assert_eq!(deleted.0, StatusCode::OK);
        assert_eq!(deleted.1["data"]["deleted"], true);
    }
    let listed = list_objects(&server.router, &server.access_token, true).await;
    let items = listed.1["data"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["uri"], second);
}

#[tokio::test]
async fn corrupt_persisted_deletion_key_is_internal_and_preserves_metadata() {
    let server = authorized_server().await;
    create_project(&server).await;
    let uri = store_version(&server, "corrupt/key.txt", "corrupt-key").await;
    let connection =
        Connection::open(server.temporary.path().join("metadata.sqlite3")).expect("database");
    connection
        .execute(
            "UPDATE object_versions SET storage_key = '../invalid' WHERE object_path = 'corrupt/key.txt'",
            [],
        )
        .expect("corrupt storage key");
    let audits_before: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("audit count");

    let response = send(
        &server.router,
        "DELETE",
        "/v1/objects",
        Some(json!({ "uri": uri })),
        Some(&server.access_token),
    )
    .await;
    assert_error(response, StatusCode::INTERNAL_SERVER_ERROR);
    let versions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM object_versions WHERE object_path = 'corrupt/key.txt'",
            [],
            |row| row.get(0),
        )
        .expect("version count");
    let audits_after: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("final audit count");
    assert_eq!(versions, 1);
    assert_eq!(audits_after, audits_before);
}

fn assert_error(response: (StatusCode, Value), status: StatusCode) {
    let (actual_status, body) = response;
    assert_eq!(actual_status, status);
    assert_eq!(body["ok"], false);
}
