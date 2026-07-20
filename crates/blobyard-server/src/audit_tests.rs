#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::record_action_at;
use crate::{error::ApiError, transfers::test_seams::fixture};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::IntoResponse,
};
use blobyard_contract::{AuditEventRecord, AuditValue, NewAuditEvent};
use tower::ServiceExt;

#[test]
fn record_action_clock_failure_does_not_persist_an_audit_event() {
    let fixture = fixture(&["audit:read"]);
    let before = fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 50)
        .expect("audit query")
        .items;
    assert!(
        record_action_at(
            &fixture.state,
            &fixture.principal,
            "fixture.action",
            "fixture",
            Vec::new(),
            Err(ApiError::internal()),
        )
        .is_err()
    );
    let after = fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 50)
        .expect("audit query")
        .items;
    assert_eq!(after, before);
    assert_eq!(
        super::test_seams::clock_failure_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn audit_values_keep_their_json_types() {
    assert_eq!(
        super::json_value(AuditValue::String("fixture".to_owned())),
        serde_json::json!("fixture")
    );
    assert_eq!(
        super::json_value(AuditValue::Number(2)),
        serde_json::json!(2)
    );
    assert_eq!(
        super::json_value(AuditValue::Boolean(true)),
        serde_json::json!(true)
    );
    assert_eq!(super::json_value(AuditValue::Null), serde_json::Value::Null);
    assert_eq!(
        super::test_seams::value_types(),
        [
            serde_json::json!("fixture"),
            serde_json::json!(2),
            serde_json::json!(true),
            serde_json::Value::Null,
        ]
    );
}

#[test]
fn audit_responses_reject_unrepresentable_timestamps() {
    let error = super::AuditResponse::try_from(AuditEventRecord {
        id: "audit_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        actor: "fixture".to_owned(),
        action: "fixture.action".to_owned(),
        request_id: "request_fixture".to_owned(),
        target_type: "fixture".to_owned(),
        metadata: Vec::new(),
        sequence: 1,
        created_at_ms: u64::try_from(i64::MAX).expect("positive timestamp"),
    })
    .err()
    .expect("timestamp outside the supported date range");
    assert_eq!(
        error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn audit_list_propagates_unrepresentable_timestamps() {
    let fixture = fixture(&["audit:read"]);
    fixture
        .state
        .repository
        .record_audit(&NewAuditEvent {
            id: "audit_invalid_time".to_owned(),
            workspace_id: fixture.principal.workspace_id.clone(),
            actor: "fixture".to_owned(),
            action: "fixture.action".to_owned(),
            request_id: "request_fixture".to_owned(),
            target_type: "fixture".to_owned(),
            metadata: Vec::new(),
            created_at_ms: u64::try_from(i64::MAX).expect("positive timestamp"),
        })
        .expect("audit fixture");
    let response = fixture
        .router()
        .oneshot(
            Request::builder()
                .uri("/v1/audit?workspace=fixture")
                .header(header::AUTHORIZATION, "Bearer secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn audit_list_maps_repository_failures() {
    assert_eq!(
        super::test_seams::list_repository_failure_status().await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
