use super::super::operations;
use super::{MANIFEST_ID, upload};
use crate::{
    Repository,
    api::AppState,
    auth::Principal,
    error::ApiError,
    repository_fault_tests::{Corruption, FaultingRepository},
    test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_api_client::{CreatePreviewRequest, ListPreviewsQuery, RevokePreviewRequest};
use blobyard_contract::{PreviewRecord, PreviewStatus};
use std::sync::Arc;

#[path = "operations_late.rs"]
mod late_failures;

fn faulted_state(mut state: AppState, failure_index: usize) -> AppState {
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::new(inner, failure_index));
    state
}

fn assert_create_failure(
    state: &AppState,
    principal: &Principal,
    request: &CreatePreviewRequest,
    now: Result<u64, ApiError>,
    expected: StatusCode,
) {
    assert_eq!(
        error_status(operations::create_at(state, principal, request, now)),
        expected
    );
}

fn create_request() -> CreatePreviewRequest {
    CreatePreviewRequest {
        workspace: "fixture".parse().expect("workspace"),
        project: "project".parse().expect("project"),
        manifest_id: MANIFEST_ID.to_owned(),
        expires: Some("1h".to_owned()),
    }
}

fn list_query() -> ListPreviewsQuery {
    ListPreviewsQuery {
        workspace: "fixture".parse().expect("workspace"),
        project: "project".parse().expect("project"),
    }
}

async fn prepared_fixture() -> (test_seams::TransferFixture, Principal) {
    let fixture = test_seams::fixture(&["object:write", "share:manage"]);
    let root = format!(".blobyard-preview/{MANIFEST_ID}/index.html");
    upload(&fixture, &root, "text/html", b"preview").await;
    let principal = Principal(fixture.principal.clone());
    (fixture, principal)
}

async fn created_preview_fixture() -> (test_seams::TransferFixture, Principal, PreviewRecord) {
    let (fixture, principal) = prepared_fixture().await;
    let _created = operations::create_at(&fixture.state, &principal, &create_request(), Ok(1_000))
        .expect("preview creation");
    let preview = fixture
        .state
        .repository
        .list_previews(&fixture.project.id)
        .expect("preview list")
        .into_iter()
        .next()
        .expect("created preview");
    (fixture, principal, preview)
}

#[test]
fn project_resolution_and_binding_failures_are_concealed() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let principal = Principal(fixture.principal.clone());
    let mut create = create_request();
    create.workspace = "missing".parse().expect("workspace");
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &create,
            Ok(1)
        )),
        StatusCode::NOT_FOUND
    );
    let mut query = list_query();
    query.project = "missing".parse().expect("project");
    assert_eq!(
        error_status(operations::list_at(
            &fixture.state,
            &principal,
            &query,
            Ok(1)
        )),
        StatusCode::NOT_FOUND
    );
    let mut bound = principal;
    bound.0.project_id = Some("project_foreign".to_owned());
    assert_eq!(
        error_status(operations::list_at(
            &fixture.state,
            &bound,
            &list_query(),
            Ok(1)
        )),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &bound,
            &create_request(),
            Ok(1)
        )),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn create_rejects_an_empty_snapshot_before_generating_a_capability() {
    let fixture = test_seams::fixture(&["share:manage"]);
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &Principal(fixture.principal.clone()),
            &create_request(),
            Ok(1),
        )),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn list_propagates_clock_and_repository_failures() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let principal = Principal(fixture.principal.clone());
    assert_eq!(
        error_status(operations::list_at(
            &fixture.state,
            &principal,
            &list_query(),
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let failed = faulted_state(fixture.state, 2);
    assert_eq!(
        error_status(operations::list_at(
            &failed,
            &principal,
            &list_query(),
            Ok(1)
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn preview_authorization_and_bound_revoke_fail_closed() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let principal = Principal(fixture.principal.clone());
    let preview = blobyard_contract::NewPreview {
        id: "preview_operation".to_owned(),
        workspace_id: fixture.principal.workspace_id.clone(),
        project_id: fixture.project.id.clone(),
        capability_hash: "c".repeat(64),
        expires_at_ms: 5_000,
        created_at_ms: 1_000,
        files: vec![blobyard_contract::NewPreviewFile {
            normalized_path: "index.html".to_owned(),
            version_id: "missing".to_owned(),
        }],
    };
    let mut foreign = PreviewRecord {
        id: preview.id.clone(),
        workspace_id: "workspace_foreign".to_owned(),
        project_id: preview.project_id,
        expires_at_ms: preview.expires_at_ms,
        status: PreviewStatus::Active,
        created_at_ms: preview.created_at_ms,
        revoked_at_ms: None,
    };
    assert_eq!(
        error_status(operations::authorize_preview(&principal, &foreign)),
        StatusCode::NOT_FOUND
    );
    foreign.workspace_id = fixture.principal.workspace_id;
    let mut project_bound = principal;
    project_bound.0.project_id = Some("project_foreign".to_owned());
    assert_eq!(
        error_status(operations::authorize_preview(&project_bound, &foreign)),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(operations::revoke_at(
            &fixture.state,
            &project_bound,
            &RevokePreviewRequest {
                preview_id: "missing".to_owned()
            },
            Err(ApiError::internal()),
        )),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn create_rejects_clock_origin_time_and_expiry_failures_after_snapshotting() {
    let (fixture, principal) = prepared_fixture().await;
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &create_request(),
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut invalid_origin = fixture.state.clone();
    invalid_origin.web_yard_origin = "bad\norigin".to_owned();
    assert_eq!(
        error_status(operations::create_at(
            &invalid_origin,
            &principal,
            &create_request(),
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &create_request(),
            Ok(253_402_300_799_001),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut invalid_expiry = create_request();
    invalid_expiry.expires = Some("31d".to_owned());
    assert_create_failure(
        &fixture.state,
        &principal,
        &invalid_expiry,
        Ok(1),
        StatusCode::BAD_REQUEST,
    );
}

#[tokio::test]
async fn create_propagates_listing_and_repository_failures_after_snapshotting() {
    let (fixture, principal) = prepared_fixture().await;
    let failed_listing = faulted_state(fixture.state.clone(), 2);
    assert_eq!(
        error_status(operations::create_at(
            &failed_listing,
            &principal,
            &create_request(),
            Ok(1)
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let failed = faulted_state(fixture.state, 3);
    assert_eq!(
        error_status(operations::create_at(
            &failed,
            &principal,
            &create_request(),
            Ok(1)
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
