#![cfg(feature = "test-seams")]
#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Deterministic clock-failure coverage for feature-gated server test seams.

use axum::http::StatusCode;
use std::net::SocketAddr;

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().expect("loopback address")
}

#[test]
fn deterministic_clock_failures_are_internal() {
    let audit = blobyard_server::audit::test_seams::clock_failure_response();
    assert_eq!(audit.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let (set, clear) = blobyard_server::retention::test_seams::clock_failure_responses();
    assert_eq!(set.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(clear.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn private_value_and_credential_contracts_execute_in_the_library_copy() {
    assert!(blobyard_server::server_result(Ok(())).is_ok());
    assert!(blobyard_server::server_result(Err(std::io::Error::other("fixture"))).is_err());
    assert_eq!(
        blobyard_server::audit::test_seams::value_types(),
        [
            serde_json::json!("fixture"),
            serde_json::json!(2),
            serde_json::json!(true),
            serde_json::Value::Null,
        ]
    );
    assert_eq!(
        blobyard_server::auth::test_seams::credential_failure_statuses(),
        [
            StatusCode::UNAUTHORIZED,
            StatusCode::UNAUTHORIZED,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::INTERNAL_SERVER_ERROR,
        ]
    );
}

#[tokio::test]
async fn repository_failures_and_concealment_execute_in_the_library_copy() {
    assert_eq!(
        blobyard_server::audit::test_seams::list_repository_failure_status().await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        blobyard_server::retention::test_seams::missing_project_statuses().await,
        [StatusCode::NOT_FOUND; 4]
    );
    assert_eq!(
        blobyard_server::retention::test_seams::overview_repository_failure_status().await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn standalone_serve_executes_success_and_listener_failure_in_the_library_copy() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    for _attempt in 0..2 {
        blobyard_server::serve_until(loopback(), temporary.path(), None, None, Box::pin(async {}))
            .await
            .expect("graceful server shutdown");
    }

    let blocked = tempfile::tempdir().expect("blocked directory");
    let listener = tokio::net::TcpListener::bind(loopback())
        .await
        .expect("occupied listener");
    let occupied = listener.local_addr().expect("occupied address");
    assert!(
        blobyard_server::serve_until(
            occupied,
            blocked.path(),
            Some("http://127.0.0.1:8787"),
            None,
            Box::pin(async {}),
        )
        .await
        .is_err()
    );
}
