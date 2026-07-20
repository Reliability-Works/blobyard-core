#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{Principal, SetRetentionRequest, clear_policy_at, set_policy_at};
use crate::{error::ApiError, transfers::test_seams::fixture};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use blobyard_core::Slug;

/// Exercises both retention mutations with a failed clock before durable mutation.
#[must_use]
pub fn clock_failure_responses() -> (Response, Response) {
    let fixture = fixture(&["retention:manage"]);
    let principal = Principal(fixture.principal.clone());
    let request = SetRetentionRequest {
        workspace: Slug::new("fixture").expect("workspace slug"),
        project: Slug::new("project").expect("project slug"),
        keep_latest: 1,
        branch: None,
        path: None,
    };
    let set = set_policy_at(
        &fixture.state,
        &principal,
        request,
        Err(ApiError::internal()),
    )
    .err()
    .expect("set clock failure")
    .into_response();
    let query = super::RetentionQuery {
        workspace: Slug::new("fixture").expect("workspace slug"),
        project: Slug::new("project").expect("project slug"),
    };
    let clear = clear_policy_at(
        &fixture.state,
        &principal,
        &query,
        Err(ApiError::internal()),
    )
    .err()
    .expect("clear clock failure")
    .into_response();
    (set, clear)
}

/// Exercises project concealment on each retention operation.
pub async fn missing_project_statuses() -> [StatusCode; 4] {
    let fixture = fixture(&["retention:manage"]);
    let principal = Principal(fixture.principal.clone());
    let query = super::RetentionQuery {
        workspace: Slug::new("fixture").expect("workspace slug"),
        project: Slug::new("missing").expect("project slug"),
    };
    let get = super::get_policy(
        State(fixture.state.clone()),
        principal.clone(),
        Ok(Query(super::RetentionQuery {
            workspace: query.workspace.clone(),
            project: query.project.clone(),
        })),
    )
    .await
    .err()
    .expect("missing get project")
    .into_response()
    .status();
    let set = set_policy_at(
        &fixture.state,
        &principal,
        SetRetentionRequest {
            workspace: query.workspace.clone(),
            project: query.project.clone(),
            keep_latest: 1,
            branch: None,
            path: None,
        },
        Ok(1),
    )
    .err()
    .expect("missing set project")
    .into_response()
    .status();
    let clear = clear_policy_at(&fixture.state, &principal, &query, Ok(1))
        .err()
        .expect("missing clear project")
        .into_response()
        .status();
    let overview = super::overview(State(fixture.state), principal, Ok(Query(query)))
        .await
        .err()
        .expect("missing overview project")
        .into_response()
        .status();
    [get, set, clear, overview]
}

/// Exercises a repository failure after successful retention project resolution.
pub async fn overview_repository_failure_status() -> StatusCode {
    let fixture = fixture(&["retention:manage"]);
    fixture.break_retention_overview();
    super::overview(
        State(fixture.state),
        Principal(fixture.principal),
        Ok(Query(super::RetentionQuery {
            workspace: Slug::new("fixture").expect("workspace slug"),
            project: Slug::new("project").expect("project slug"),
        })),
    )
    .await
    .err()
    .expect("overview repository failure")
    .into_response()
    .status()
}
