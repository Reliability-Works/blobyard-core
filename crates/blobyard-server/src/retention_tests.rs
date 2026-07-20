#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{RetentionQuery, SetRetentionRequest, clear_policy_at, set_policy_at};
use crate::{
    auth::Principal,
    error::ApiError,
    transfers::test_seams::{TransferFixture, fixture},
};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::IntoResponse,
};
use blobyard_contract::RepositoryError;
use tower::ServiceExt;

#[tokio::test]
async fn retention_router_covers_validation_success_update_clear_and_absence() {
    let fixture = fixture(&["retention:manage"]);
    assert_eq!(
        send(&fixture, "GET", "/v1/retention", "").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(&fixture, "PUT", "/v1/retention", "{").await,
        StatusCode::BAD_REQUEST
    );
    let body = r#"{"workspace":"fixture","project":"project","keepLatest":1}"#;
    for _attempt in 0..2 {
        assert_eq!(
            send(&fixture, "PUT", "/v1/retention", body).await,
            StatusCode::OK
        );
    }
    let query = "/v1/retention?workspace=fixture&project=project";
    assert_eq!(send(&fixture, "GET", query, "").await, StatusCode::OK);
    assert_eq!(
        send(
            &fixture,
            "GET",
            "/v1/retention/overview?workspace=fixture&project=project",
            "",
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        send(&fixture, "DELETE", "/v1/retention", "").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(send(&fixture, "DELETE", query, "").await, StatusCode::OK);
    assert_eq!(
        send(&fixture, "DELETE", query, "").await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&fixture, "GET", query, "").await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn retention_overview_serializes_the_last_run() {
    let fixture = fixture(&["retention:manage"]);
    let body = r#"{"workspace":"fixture","project":"project","keepLatest":1}"#;
    assert_eq!(
        send(&fixture, "PUT", "/v1/retention", body).await,
        StatusCode::OK
    );
    fixture
        .state
        .repository
        .begin_retention(
            &fixture.project.id,
            "run_fixture",
            "system:retention",
            "request_fixture",
            1,
        )
        .expect("retention run");
    assert_eq!(
        send(
            &fixture,
            "GET",
            "/v1/retention/overview?workspace=fixture&project=project",
            "",
        )
        .await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn retention_overview_maps_repository_failures() {
    assert_eq!(
        super::test_seams::overview_repository_failure_status().await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn retention_routes_hide_missing_projects() {
    let fixture = fixture(&["retention:manage"]);
    let query = "/v1/retention?workspace=fixture&project=missing";
    let body = r#"{"workspace":"fixture","project":"missing","keepLatest":1}"#;
    assert_eq!(
        send(&fixture, "GET", query, "").await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&fixture, "PUT", "/v1/retention", body).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&fixture, "DELETE", query, "").await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &fixture,
            "GET",
            "/v1/retention/overview?workspace=fixture&project=missing",
            "",
        )
        .await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        super::test_seams::missing_project_statuses().await,
        [StatusCode::NOT_FOUND; 4]
    );
}

#[test]
fn retention_clock_failures_do_not_change_policy_or_audit_state() {
    let fixture = fixture(&["retention:manage"]);
    let principal = Principal(fixture.principal.clone());
    let input = request();
    let response = set_policy_at(&fixture.state, &principal, input, Err(ApiError::internal()))
        .err()
        .expect("set clock failure")
        .into_response();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        fixture
            .state
            .repository
            .retention_policy(&fixture.project.id),
        Err(RepositoryError::NotFound)
    );
    assert!(audits(&fixture).is_empty());

    drop(set_policy_at(&fixture.state, &principal, request(), Ok(1)).expect("retention policy"));
    let before = fixture
        .state
        .repository
        .retention_policy(&fixture.project.id)
        .expect("persisted policy");
    let audit_count = audits(&fixture).len();
    let response = clear_policy_at(
        &fixture.state,
        &principal,
        &query(),
        Err(ApiError::internal()),
    )
    .err()
    .expect("clear clock failure")
    .into_response();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        fixture
            .state
            .repository
            .retention_policy(&fixture.project.id),
        Ok(before)
    );
    assert_eq!(audits(&fixture).len(), audit_count);

    let (set, clear) = super::test_seams::clock_failure_responses();
    assert_eq!(set.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(clear.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

fn request() -> SetRetentionRequest {
    SetRetentionRequest {
        workspace: "fixture".parse().expect("workspace slug"),
        project: "project".parse().expect("project slug"),
        keep_latest: 1,
        branch: None,
        path: None,
    }
}

fn query() -> RetentionQuery {
    RetentionQuery {
        workspace: "fixture".parse().expect("workspace slug"),
        project: "project".parse().expect("project slug"),
    }
}

fn audits(fixture: &TransferFixture) -> Vec<blobyard_contract::AuditEventRecord> {
    fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 50)
        .expect("audit query")
        .items
}

async fn send(fixture: &TransferFixture, method: &str, path: &str, body: &str) -> StatusCode {
    fixture
        .router()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::AUTHORIZATION, "Bearer secret")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_owned()))
                .expect("request"),
        )
        .await
        .expect("response")
        .status()
}
