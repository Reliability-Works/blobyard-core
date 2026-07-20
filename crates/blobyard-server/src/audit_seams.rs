#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{AuditQuery, Principal};
use crate::{error::ApiError, transfers::test_seams::fixture};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blobyard_contract::AuditValue;
use blobyard_core::Slug;

/// Exercises audit recording with a failed clock before durable mutation.
#[must_use]
pub fn clock_failure_response() -> Response {
    let fixture = fixture(&["audit:read"]);
    super::record_action_at(
        &fixture.state,
        &fixture.principal,
        "fixture.recorded",
        "fixture",
        Vec::new(),
        Err(ApiError::internal()),
    )
    .expect_err("audit clock failure")
    .into_response()
}

/// Converts every supported audit value to its public JSON type.
#[must_use]
pub fn value_types() -> [serde_json::Value; 4] {
    [
        super::json_value(AuditValue::String("fixture".to_owned())),
        super::json_value(AuditValue::Number(2)),
        super::json_value(AuditValue::Boolean(true)),
        super::json_value(AuditValue::Null),
    ]
}

/// Exercises an audit-listing provider failure after successful workspace resolution.
pub async fn list_repository_failure_status() -> StatusCode {
    let fixture = fixture(&["audit:read"]);
    fixture.break_audit_listing();
    super::list(
        State(fixture.state),
        Principal(fixture.principal),
        Ok(Query(AuditQuery {
            workspace: Slug::new("fixture").expect("workspace slug"),
            cursor: None,
        })),
    )
    .await
    .err()
    .expect("audit repository failure")
    .into_response()
    .status()
}
