#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{require_users_manage, summary};
use crate::auth::Principal;
use crate::contract_test_support::{assert_error, send, send_as};
use crate::repository_fault_tests::FaultingRepository;
use crate::transfers::test_seams;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use blobyard_api_client::{CreateLocalUserRequest, ListLocalUsersQuery};
use blobyard_contract::{LocalUserListing, LocalUserStatus};
use blobyard_core::Slug;
use std::sync::Arc;

use super::tests::manager_fixture;

#[tokio::test]
async fn user_routes_reject_invalid_bodies_and_missing_management_authority() {
    let fixture = manager_fixture();
    for body in [
        b"{".as_slice(),
        br#"{"displayName":"Valid name","workspace":"fixture","unknown":true}"#,
        br#"{"displayName":"x","workspace":"fixture"}"#,
        br#"{"displayName":"line\nbreak","workspace":"fixture"}"#,
        br#"{"displayName":"Valid name","email":"missing-at","workspace":"fixture"}"#,
        br#"{"displayName":"Valid name","email":"@half","workspace":"fixture"}"#,
        br#"{"displayName":"Valid name","email":"half@","workspace":"fixture"}"#,
        br#"{"displayName":"Valid name","email":"split @example.test","workspace":"fixture"}"#,
    ] {
        assert_error(
            send(&fixture, "POST", "/v1/users", body, false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    for (method, path, body) in [
        ("GET", "/v1/users", b"".as_slice()),
        ("POST", "/v1/users/reset-key", b"{".as_slice()),
        ("POST", "/v1/users/deactivate", b"{".as_slice()),
    ] {
        assert_error(
            send(&fixture, method, path, body, false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    assert!(
        fixture
            .state
            .repository
            .list_local_users("workspace_fixture")
            .expect("users")
            .is_empty()
    );
    assert_unprivileged_user_routes().await;
}

async fn assert_unprivileged_user_routes() {
    let unprivileged = test_seams::fixture(&["object:read"]);
    for (method, path, body) in user_route_shapes() {
        assert_error(
            send(&unprivileged, method, path, body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
}

#[tokio::test]
async fn user_routes_map_repository_failures_without_partial_mutation() {
    for (method, path, body) in user_route_shapes() {
        let fixture = manager_fixture();
        assert_error(
            send_with_repository_failure(&fixture, method, path, body).await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
        assert!(
            fixture
                .state
                .repository
                .list_local_users("workspace_fixture")
                .expect("users")
                .is_empty()
        );
    }
}

async fn send_with_repository_failure(
    fixture: &crate::transfers::test_seams::TransferFixture,
    method: &str,
    path: &str,
    body: &[u8],
) -> Response {
    let mut state = fixture.state.clone();
    state.repository = Arc::new(FaultingRepository::new(Arc::clone(&state.repository), 1));
    send_as(
        test_seams::fixture_router(&state),
        "secret",
        method,
        path,
        body,
    )
    .await
}

fn user_route_shapes() -> [(&'static str, &'static str, &'static [u8]); 4] {
    [
        ("GET", "/v1/users?workspace=fixture", b"".as_slice()),
        (
            "POST",
            "/v1/users",
            br#"{"displayName":"Valid name","workspace":"fixture"}"#,
        ),
        (
            "POST",
            "/v1/users/reset-key",
            br#"{"userId":"user_fixture"}"#,
        ),
        (
            "POST",
            "/v1/users/deactivate",
            br#"{"userId":"user_fixture"}"#,
        ),
    ]
}

#[test]
fn machine_principals_are_denied_before_scope_evaluation() {
    let machine = Principal(blobyard_contract::LocalApiTokenRecord {
        id: "machine_fixture".to_owned(),
        name: "Machine".to_owned(),
        token_prefix: "byd_ci_fixture".to_owned(),
        secret_hash: crate::auth::hash("machine"),
        scopes: vec!["users:manage".to_owned()],
        workspace_id: "workspace_fixture".to_owned(),
        project_id: Some("project_fixture".to_owned()),
        created_at_ms: 1,
        expires_at_ms: 10,
        last_used_at_ms: None,
        revoked_at_ms: None,
    });
    assert!(require_users_manage(&machine).is_err());
}

#[test]
fn summaries_reject_unrepresentable_creation_times() {
    let listing = LocalUserListing {
        user: blobyard_contract::LocalUserRecord {
            id: "user_fixture".to_owned(),
            workspace_id: "workspace_fixture".to_owned(),
            display_name: "Fixture".to_owned(),
            email: None,
            status: LocalUserStatus::Active,
            created_at_ms: u64::MAX,
            deactivated_at_ms: None,
        },
        active_key_prefix: None,
    };
    assert!(summary(listing).is_err());
}

#[test]
fn creation_rejects_unrenderable_timestamps_before_persistence() {
    let fixture = manager_fixture();
    let principal = Principal(fixture.principal.clone());
    let request = CreateLocalUserRequest {
        workspace: Slug::new("fixture".to_owned()).expect("slug"),
        display_name: "Valid name".to_owned(),
        email: None,
    };
    assert!(
        super::create_with_clock(
            &fixture.state,
            &principal,
            &request,
            Ok(9_223_372_036_854_775_806),
        )
        .is_err()
    );
    assert!(
        fixture
            .state
            .repository
            .list_local_users("workspace_fixture")
            .expect("users")
            .is_empty()
    );
}

#[tokio::test]
async fn listing_propagates_repository_failures_after_workspace_authorization() {
    let fixture = manager_fixture();
    let mut state = fixture.state.clone();
    state.repository = Arc::new(FaultingRepository::new(Arc::clone(&state.repository), 1));
    let query = ListLocalUsersQuery {
        workspace: Slug::new("fixture".to_owned()).expect("slug"),
    };
    assert!(
        super::list(
            State(state),
            Principal(fixture.principal.clone()),
            Ok(Query(query)),
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn user_mutations_propagate_clock_failures() {
    let fixture = manager_fixture();
    let principal = Principal(fixture.principal.clone());
    let clock = || Err(crate::error::ApiError::internal());
    let create = CreateLocalUserRequest {
        workspace: Slug::new("fixture".to_owned()).expect("slug"),
        display_name: "Valid name".to_owned(),
        email: None,
    };
    assert!(super::create_with_clock(&fixture.state, &principal, &create, clock()).is_err());
    let reset = blobyard_api_client::ResetLocalUserLoginKeyRequest {
        user_id: "user_fixture".to_owned(),
    };
    assert!(super::reset_with_clock(&fixture.state, &principal, &reset, clock()).is_err());
    let deactivate = blobyard_api_client::DeactivateLocalUserRequest {
        user_id: "user_fixture".to_owned(),
    };
    assert!(
        super::deactivate_with_clock(&fixture.state, &principal, &deactivate, clock()).is_err()
    );
}
