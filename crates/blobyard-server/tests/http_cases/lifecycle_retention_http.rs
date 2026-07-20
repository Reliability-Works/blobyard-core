use crate::lifecycle::create_project;
use crate::lifecycle_support::{install_scoped_token, project_id};
use crate::support::{AuthorizedServer, authorized_server, send, send_json_bytes};
use axum::http::StatusCode;
use blobyard_contract::LifecycleRepository;
use blobyard_repository_sqlite::SqliteRepository;
use serde_json::{Value, json};

#[tokio::test]
async fn retention_routes_validate_auth_queries_bodies_and_absent_policies() {
    let server = authorized_server().await;
    create_project(&server).await;
    let uri = "/v1/retention?workspace=default&project=documentation";
    assert_retention_auth_guards(&server, uri).await;
    assert_retention_query_guards(&server).await;
    assert_retention_body_guards(&server, uri).await;
}

async fn assert_retention_auth_guards(server: &AuthorizedServer, uri: &str) {
    assert_error(
        send(&server.router, "GET", uri, None, None).await,
        StatusCode::UNAUTHORIZED,
    );
    let limited = install_scoped_token(server.temporary.path(), &["object:read"]);
    assert_error(
        send(&server.router, "GET", uri, None, Some(&limited)).await,
        StatusCode::FORBIDDEN,
    );
    for (method, path, body) in [
        (
            "PUT",
            "/v1/retention",
            Some(json!({
                "workspace": "default", "project": "documentation", "keepLatest": 1
            })),
        ),
        (
            "DELETE",
            "/v1/retention?workspace=default&project=documentation",
            None,
        ),
        (
            "GET",
            "/v1/retention/overview?workspace=default&project=documentation",
            None,
        ),
    ] {
        assert_error(
            send(&server.router, method, path, body, Some(&limited)).await,
            StatusCode::FORBIDDEN,
        );
    }
}

async fn assert_retention_query_guards(server: &AuthorizedServer) {
    assert_error(
        send(
            &server.router,
            "GET",
            "/v1/retention?workspace=default&project=documentation&extra=true",
            None,
            Some(&server.access_token),
        )
        .await,
        StatusCode::BAD_REQUEST,
    );
    for (method, path) in [
        (
            "DELETE",
            "/v1/retention?workspace=default&project=documentation&extra=true",
        ),
        (
            "GET",
            "/v1/retention/overview?workspace=default&project=documentation&extra=true",
        ),
    ] {
        assert_error(
            send(
                &server.router,
                method,
                path,
                None,
                Some(&server.access_token),
            )
            .await,
            StatusCode::BAD_REQUEST,
        );
    }
}

async fn assert_retention_body_guards(server: &AuthorizedServer, uri: &str) {
    assert_error(
        send(&server.router, "GET", uri, None, Some(&server.access_token)).await,
        StatusCode::NOT_FOUND,
    );
    assert_error(
        send_json_bytes(
            &server.router,
            "PUT",
            "/v1/retention",
            b"{".to_vec(),
            Some(&server.access_token),
        )
        .await,
        StatusCode::BAD_REQUEST,
    );
    for body in [
        json!({
            "workspace": "default", "project": "documentation", "keepLatest": 0
        }),
        json!({
            "workspace": "default", "project": "documentation", "keepLatest": 1,
            "unknown": true
        }),
    ] {
        assert_error(
            send(
                &server.router,
                "PUT",
                "/v1/retention",
                Some(body),
                Some(&server.access_token),
            )
            .await,
            StatusCode::BAD_REQUEST,
        );
    }
}

#[tokio::test]
async fn retention_routes_set_update_get_overview_and_clear() {
    let server = authorized_server().await;
    create_project(&server).await;
    for keep_latest in [1, 2] {
        let set = send(
            &server.router,
            "PUT",
            "/v1/retention",
            Some(json!({
                "workspace": "default", "project": "documentation",
                "keepLatest": keep_latest, "path": "artifacts/**", "branch": "release-*"
            })),
            Some(&server.access_token),
        )
        .await;
        assert_eq!(set.0, StatusCode::OK);
        assert_eq!(set.1["data"]["keepLatest"], keep_latest);
        assert_eq!(set.1["data"]["pathGlob"], "artifacts/**");
        assert_eq!(set.1["data"]["branchGlob"], "release-*");
    }
    let uri = "/v1/retention?workspace=default&project=documentation";
    let get = send(&server.router, "GET", uri, None, Some(&server.access_token)).await;
    assert_eq!(get.0, StatusCode::OK);
    assert_eq!(get.1["data"]["keepLatest"], 2);
    let overview = send(
        &server.router,
        "GET",
        "/v1/retention/overview?workspace=default&project=documentation",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(overview.0, StatusCode::OK);
    assert_eq!(overview.1["data"]["policy"]["keepLatest"], 2);
    assert_eq!(overview.1["data"]["lastRun"], Value::Null);
    let clear = send(
        &server.router,
        "DELETE",
        uri,
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(clear.0, StatusCode::OK);
    assert_eq!(clear.1["data"]["cleared"], true);
    assert_error(
        send(
            &server.router,
            "DELETE",
            uri,
            None,
            Some(&server.access_token),
        )
        .await,
        StatusCode::NOT_FOUND,
    );
    let overview = send(
        &server.router,
        "GET",
        "/v1/retention/overview?workspace=default&project=documentation",
        None,
        Some(&server.access_token),
    )
    .await;
    assert_eq!(overview.0, StatusCode::OK);
    assert_eq!(overview.1["data"]["policy"], Value::Null);
}

#[tokio::test]
async fn retention_policy_mutations_conflict_with_a_pending_run() {
    let server = authorized_server().await;
    create_project(&server).await;
    let body = json!({
        "workspace": "default", "project": "documentation", "keepLatest": 1
    });
    assert_eq!(
        send(
            &server.router,
            "PUT",
            "/v1/retention",
            Some(body.clone()),
            Some(&server.access_token),
        )
        .await
        .0,
        StatusCode::OK
    );
    let repository = SqliteRepository::open(&server.temporary.path().join("metadata.sqlite3"))
        .expect("repository");
    repository
        .begin_retention(
            &project_id(server.temporary.path()),
            "retention_pending",
            "system:retention",
            "request_pending",
            1,
        )
        .expect("pending run");
    assert_error(
        send(
            &server.router,
            "PUT",
            "/v1/retention",
            Some(body),
            Some(&server.access_token),
        )
        .await,
        StatusCode::CONFLICT,
    );
    assert_error(
        send(
            &server.router,
            "DELETE",
            "/v1/retention?workspace=default&project=documentation",
            None,
            Some(&server.access_token),
        )
        .await,
        StatusCode::CONFLICT,
    );
}

fn assert_error(response: (StatusCode, Value), status: StatusCode) {
    let (actual_status, body) = response;
    assert_eq!(actual_status, status);
    assert_eq!(body["ok"], false);
}
