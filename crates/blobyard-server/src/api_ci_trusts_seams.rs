#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{CreateRequest, RevokeRequest, create_at, revoke_at};
use crate::{auth::Principal, error::ApiError, transfers::test_seams};
use axum::{Json, http::StatusCode, response::IntoResponse};
use blobyard_core::Slug;

fn slug(value: &str) -> Slug {
    Slug::new(value).expect("slug")
}

fn request() -> CreateRequest {
    CreateRequest {
        allowed_actions: vec!["upload".to_owned()],
        allowed_ref_glob: "refs/heads/main".to_owned(),
        environment: None,
        project: Some(slug("project")),
        repository: "reliability-works/blobyard-core".to_owned(),
        workflow_path: ".github/workflows/release.yml".to_owned(),
        workflow_ref: "refs/heads/main".to_owned(),
        workspace: slug("fixture"),
    }
}

/// Exercises the normal-library trust failure boundaries.
#[must_use]
pub fn failure_statuses() -> [StatusCode; 3] {
    let fixture = test_seams::fixture(&["ci:manage"]);
    let principal = Principal(fixture.principal.clone());
    let create_status = create_at(
        &fixture.state,
        &principal,
        Ok(Json(request())),
        Err(ApiError::internal()),
    )
    .err()
    .expect("create clock failure")
    .into_response()
    .status();

    let _created =
        create_at(&fixture.state, &principal, Ok(Json(request())), Ok(1)).expect("create trust");
    let trust_id = fixture
        .state
        .repository
        .list_ci_trusts(&fixture.principal.workspace_id)
        .expect("list trusts")
        .into_iter()
        .next()
        .expect("created trust")
        .id;
    let revoke_status = revoke_at(
        &fixture.state,
        &principal,
        Ok(Json(RevokeRequest {
            trust_id: trust_id.clone(),
        })),
        Err(ApiError::internal()),
    )
    .err()
    .expect("revoke clock failure")
    .into_response()
    .status();
    fixture.break_ci_revoke();
    let repository_status = revoke_at(
        &fixture.state,
        &principal,
        Ok(Json(RevokeRequest { trust_id })),
        Ok(2),
    )
    .err()
    .expect("revoke repository failure")
    .into_response()
    .status();
    [create_status, revoke_status, repository_status]
}
