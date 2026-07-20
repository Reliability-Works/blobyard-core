use crate::lifecycle_support::{insert_audit_events, install_scoped_token};
use crate::support::{AuthorizedServer, authorized_server, send};
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::{Value, json};

#[tokio::test]
async fn audit_enforces_auth_ownership_cursor_and_stable_paging() {
    let server = authorized_server().await;
    assert_audit_authorization_edges(&server).await;
    assert_audit_query_edges(&server).await;
    assert_stable_audit_paging(&server).await;
}

#[tokio::test]
async fn audit_rejects_an_out_of_range_persisted_timestamp_without_mutation() {
    let server = authorized_server().await;
    let connection =
        Connection::open(server.temporary.path().join("metadata.sqlite3")).expect("connection");
    connection
        .execute(
            "INSERT INTO audit_events (id, workspace_id, actor, action, request_id, target_type, metadata_json, created_at_ms) VALUES ('audit_corrupt_time', (SELECT id FROM workspaces WHERE slug = 'default'), 'fixture', 'fixture.action', 'request_fixture', 'fixture', '{}', ?1)",
            [i64::MAX],
        )
        .expect("corrupt audit event");
    let before: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("audit count");
    assert_error(
        send(
            &server.router,
            "GET",
            "/v1/audit?workspace=default",
            None,
            Some(&server.access_token),
        )
        .await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    );
    let after: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("final audit count");
    assert_eq!(after, before);
}

async fn assert_audit_authorization_edges(server: &AuthorizedServer) {
    assert_error(
        send(
            &server.router,
            "GET",
            "/v1/audit?workspace=default",
            None,
            None,
        )
        .await,
        StatusCode::UNAUTHORIZED,
        "AUTH_REQUIRED",
    );
    let limited = install_scoped_token(server.temporary.path(), &["object:read"]);
    assert_error(
        send(
            &server.router,
            "GET",
            "/v1/audit?workspace=default",
            None,
            Some(&limited),
        )
        .await,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
    );
}

async fn assert_audit_query_edges(server: &AuthorizedServer) {
    for uri in [
        "/v1/audit?workspace=default&unknown=true",
        "/v1/audit?workspace=default&cursor=not-a-number",
    ] {
        assert_error(
            send(&server.router, "GET", uri, None, Some(&server.access_token)).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        );
    }
    assert_error(
        send(
            &server.router,
            "GET",
            "/v1/audit?workspace=missing",
            None,
            Some(&server.access_token),
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    );

    let other = send(
        &server.router,
        "POST",
        "/v1/workspaces",
        Some(json!({ "name": "Other" })),
        Some(&server.access_token),
    )
    .await;
    assert_eq!(other.0, StatusCode::OK);
    assert_error(
        send(
            &server.router,
            "GET",
            "/v1/audit?workspace=other",
            None,
            Some(&server.access_token),
        )
        .await,
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
    );
}

async fn assert_stable_audit_paging(server: &AuthorizedServer) {
    insert_audit_events(server.temporary.path(), 55);
    let first = send(
        &server.router,
        "GET",
        "/v1/audit?workspace=default",
        None,
        Some(&server.access_token),
    )
    .await;
    let cursor = assert_first_audit_page(&first);
    let second = send(
        &server.router,
        "GET",
        &format!("/v1/audit?workspace=default&cursor={cursor}"),
        None,
        Some(&server.access_token),
    )
    .await;
    assert_second_audit_page(&second);
    let encoded = serde_json::to_string(&first.1).expect("audit JSON");
    assert!(!encoded.contains(&server.access_token));
    assert!(!encoded.contains(&server.bootstrap_token));
}

fn assert_first_audit_page(response: &(StatusCode, Value)) -> String {
    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(
        response.1["data"]["items"].as_array().expect("items").len(),
        50
    );
    let newest = &response.1["data"]["items"][0];
    assert_eq!(newest["metadata"]["bool"], true);
    assert_eq!(newest["metadata"]["null"], Value::Null);
    assert_eq!(newest["metadata"]["number"], 54);
    assert_eq!(newest["metadata"]["text"], "safe");
    response.1["data"]["nextCursor"]
        .as_str()
        .expect("next cursor")
        .to_owned()
}

fn assert_second_audit_page(response: &(StatusCode, Value)) {
    assert_eq!(response.0, StatusCode::OK);
    assert!(
        !response.1["data"]["items"]
            .as_array()
            .expect("items")
            .is_empty()
    );
    assert_eq!(response.1["data"]["nextCursor"], Value::Null);
}

fn assert_error(response: (StatusCode, Value), status: StatusCode, code: &str) {
    let (actual_status, body) = response;
    assert_eq!(actual_status, status);
    assert_eq!(body["error"]["code"], code);
}
