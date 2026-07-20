#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{CreateRequest, ListQuery, RevokeRequest, create, create_at, list, revoke, revoke_at};
use crate::{auth::Principal, error::ApiError, transfers::test_seams};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, extract::State};
use blobyard_core::Slug;

fn slug(value: &str) -> Slug {
    Slug::new(value.to_owned()).expect("slug")
}

fn create_request() -> CreateRequest {
    CreateRequest {
        allowed_actions: vec!["upload".to_owned(), "yard:manage".to_owned()],
        allowed_ref_glob: "refs/heads/main".to_owned(),
        environment: Some("release".to_owned()),
        project: Some(slug("project")),
        repository: "Reliability-Works/Blobyard-Core".to_owned(),
        workflow_path: ".github/workflows/release.yml".to_owned(),
        workflow_ref: "refs/heads/main".to_owned(),
        workspace: slug("fixture"),
    }
}

#[tokio::test]
async fn create_list_and_revoke_return_the_exact_redacted_contract() {
    let fixture = test_seams::fixture(&["ci:manage"]);
    let principal = Principal(fixture.principal.clone());
    let created = create(
        State(fixture.state.clone()),
        principal.clone(),
        Ok(Json(create_request())),
    )
    .await
    .expect("create trust");
    let created = serde_json::to_value(&created.0).expect("created JSON");
    assert_eq!(
        created["data"]["repository"],
        "reliability-works/blobyard-core"
    );
    assert_eq!(created["data"]["projectId"], "project_fixture");
    assert_eq!(
        created["data"]["allowedActions"],
        serde_json::json!(["upload", "yard:manage"])
    );
    assert!(created["data"].get("audience").is_none());
    let trust_id = created["data"]["id"].as_str().expect("trust id").to_owned();

    let listed = list(
        State(fixture.state.clone()),
        principal.clone(),
        axum::extract::Query(ListQuery {
            workspace: slug("fixture"),
        }),
    )
    .await
    .expect("list trusts");
    let listed = serde_json::to_value(&listed.0).expect("listed JSON");
    assert_eq!(listed["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed["data"][0]["id"], trust_id);

    let revoked = revoke(
        State(fixture.state.clone()),
        principal.clone(),
        Ok(Json(RevokeRequest {
            trust_id: trust_id.clone(),
        })),
    )
    .await
    .expect("revoke trust");
    assert_eq!(
        serde_json::to_value(&revoked.0).expect("revoke JSON")["data"],
        "revoked"
    );
    let replay = revoke(
        State(fixture.state),
        principal,
        Ok(Json(RevokeRequest { trust_id })),
    )
    .await
    .expect("revoke replay");
    assert_eq!(
        serde_json::to_value(&replay.0).expect("replay JSON")["data"],
        "already_revoked"
    );
}

#[tokio::test]
async fn trust_management_rejects_missing_scope_and_invalid_actions() {
    let no_scope = test_seams::fixture(&["object:read"]);
    assert!(
        create(
            State(no_scope.state),
            Principal(no_scope.principal),
            Ok(Json(create_request())),
        )
        .await
        .is_err()
    );

    let fixture = test_seams::fixture(&["ci:manage"]);
    for actions in [
        Vec::new(),
        vec!["upload".to_owned(), "upload".to_owned()],
        vec!["unknown".to_owned()],
    ] {
        let mut request = create_request();
        request.allowed_actions = actions;
        assert!(
            create(
                State(fixture.state.clone()),
                Principal(fixture.principal.clone()),
                Ok(Json(request)),
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn trust_management_rejects_invalid_targets_and_normalizes_optionals() {
    let fixture = test_seams::fixture(&["ci:manage"]);
    let mut invalid_repository = create_request();
    invalid_repository.repository = "missing-slash".to_owned();
    assert!(
        create(
            State(fixture.state.clone()),
            Principal(fixture.principal.clone()),
            Ok(Json(invalid_repository)),
        )
        .await
        .is_err()
    );
    let mut missing_project = create_request();
    missing_project.project = Some(slug("missing"));
    let missing_project_error = create(
        State(fixture.state.clone()),
        Principal(fixture.principal.clone()),
        Ok(Json(missing_project)),
    )
    .await
    .err()
    .expect("missing project must fail");
    assert_eq!(
        missing_project_error.into_response().status(),
        StatusCode::NOT_FOUND
    );
    assert!(
        revoke(
            State(fixture.state),
            Principal(fixture.principal),
            Ok(Json(RevokeRequest {
                trust_id: "trust_missing".to_owned(),
            })),
        )
        .await
        .is_err()
    );

    let empty_environment_fixture = test_seams::fixture(&["ci:manage"]);
    let mut empty_environment = create_request();
    empty_environment.environment = Some(String::new());
    empty_environment.project = None;
    let created = create(
        State(empty_environment_fixture.state),
        Principal(empty_environment_fixture.principal),
        Ok(Json(empty_environment)),
    )
    .await
    .expect("empty environment is omitted");
    let created = serde_json::to_value(created.0).expect("created JSON");
    assert!(created["data"]["environment"].is_null());
    assert!(created["data"]["projectId"].is_null());
}

#[tokio::test]
async fn trust_clock_failures_stop_before_mutation() {
    let fixture = test_seams::fixture(&["ci:manage"]);
    let principal = Principal(fixture.principal.clone());
    let create_error = create_at(
        &fixture.state,
        &principal,
        Ok(Json(create_request())),
        Err(ApiError::internal()),
    )
    .err()
    .expect("create clock failure");
    assert_eq!(
        create_error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let created = create(
        State(fixture.state.clone()),
        principal.clone(),
        Ok(Json(create_request())),
    )
    .await
    .expect("create trust");
    let trust_id = serde_json::to_value(created.0).expect("created JSON")["data"]["id"]
        .as_str()
        .expect("trust ID")
        .to_owned();
    let revoke_error = revoke_at(
        &fixture.state,
        &principal,
        Ok(Json(RevokeRequest {
            trust_id: trust_id.clone(),
        })),
        Err(ApiError::internal()),
    )
    .err()
    .expect("revoke clock failure");
    assert_eq!(
        revoke_error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    fixture.break_ci_revoke();
    let repository_error = revoke(
        State(fixture.state),
        Principal(fixture.principal),
        Ok(Json(RevokeRequest { trust_id })),
    )
    .await
    .err()
    .expect("repository failure");
    assert_eq!(
        repository_error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
