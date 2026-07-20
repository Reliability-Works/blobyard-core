#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{failure_tests::seeded_share, operations, tests::upload_object};
use crate::{
    Repository,
    auth::{Principal, hash},
    error::ApiError,
    repository_fault_tests::{Corruption, FaultingRepository},
    test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_api_client::{
    CreateShareRequest, ListSharesQuery, ResolveShareQuery, RevokeShareRequest,
};
use blobyard_contract::{ShareRecord, ShareStatus};
use blobyard_core::SecretString;
use std::sync::Arc;

fn request(target: &str) -> CreateShareRequest {
    CreateShareRequest {
        target: target.parse().expect("object URI"),
        expires: Some("1s".to_owned()),
        notify: None,
    }
}

#[tokio::test]
async fn create_preparation_rejects_binding_clock_url_and_expiry_failures() {
    let fixture = test_seams::fixture(&["object:write", "share:manage"]);
    let target = upload_object(&fixture).await;
    let principal = Principal(fixture.principal.clone());

    let mut bound = principal.clone();
    bound.0.project_id = Some("project_foreign".to_owned());
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &bound,
            &request(&target),
            Ok(1),
        )),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &request(&target),
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut invalid_origin = fixture.state.clone();
    "invalid\norigin".clone_into(&mut invalid_origin.public_origin);
    assert_eq!(
        error_status(operations::create_at(
            &invalid_origin,
            &principal,
            &request(&target),
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(operations::create_at(
            &fixture.state,
            &principal,
            &request(&target),
            Ok(253_402_300_799_001),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(
        fixture
            .state
            .repository
            .list_shares(&fixture.principal.workspace_id)
            .expect("shares")
            .is_empty()
    );
}

#[test]
fn list_conversion_propagates_clock_and_summary_failures() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let principal = Principal(fixture.principal.clone());
    let query = ListSharesQuery {
        workspace: "fixture".parse().expect("workspace"),
    };
    assert_eq!(
        error_status(operations::list_at(
            &fixture.state,
            &principal,
            &query,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(operations::share_page(
            vec![ShareRecord {
                id: "share".to_owned(),
                workspace_id: fixture.principal.workspace_id,
                version_id: Some("version".to_owned()),
                expires_at_ms: u64::MAX,
                status: ShareStatus::Active,
                consumed_count: 0,
                maximum_downloads: None,
                created_at_ms: 1,
                revoked_at_ms: None,
            }],
            1,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn public_operations_reject_malformed_tokens_and_clock_failures() {
    let fixture = test_seams::fixture(&["share:manage"]);
    let invalid = || ApiError::not_found_result(SecretString::new("invalid\ncapability"));
    assert_eq!(
        error_status(operations::open_at(&fixture.state, invalid(), Ok(1))),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(operations::download_shared_file_at(
            &fixture.state,
            invalid(),
            Ok(1),
        )),
        StatusCode::NOT_FOUND
    );

    let token = SecretString::new("valid-capability").expect("token");
    let query = ResolveShareQuery {
        token: token.clone(),
    };
    assert_eq!(
        error_status(operations::resolve_at(
            &fixture.state,
            &query,
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(operations::open_at(
            &fixture.state,
            Ok(token.clone()),
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(operations::download_shared_file_at(
            &fixture.state,
            Ok(token),
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn public_operations_reject_unformattable_and_overflowing_expiry() {
    let (fixture, token, _id) = seeded_share().await;
    let token = SecretString::new(token).expect("share token");
    let mut corrupt_state = fixture.state.clone();
    let inner: Arc<dyn Repository> = Arc::clone(&corrupt_state.repository);
    corrupt_state.repository = Arc::new(FaultingRepository::corrupting(
        inner,
        Corruption::ShareExpiry,
    ));
    assert_eq!(
        error_status(operations::resolve_at(
            &corrupt_state,
            &ResolveShareQuery {
                token: token.clone(),
            },
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(operations::open_at(
            &corrupt_state,
            Ok(token.clone()),
            Ok(1),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut target = fixture
        .state
        .repository
        .share_by_capability(&hash(token.expose_secret()), 1)
        .expect("share target");
    target.share.expires_at_ms = u64::MAX;
    let raw_download = SecretString::new("download-capability").expect("download token");
    assert_eq!(
        error_status(operations::issue_for_target(
            &fixture.state,
            &hash(token.expose_secret()),
            &raw_download,
            &target,
            u64::MAX,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn revoke_operation_propagates_clock_failure_before_mutation() {
    let (fixture, _token, id) = seeded_share().await;
    assert_eq!(
        error_status(operations::revoke_at(
            &fixture.state,
            &Principal(fixture.principal),
            &RevokeShareRequest { share_id: id },
            Err(ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
