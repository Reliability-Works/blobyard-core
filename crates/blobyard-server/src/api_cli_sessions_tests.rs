#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{RevokeRequest, list};
use crate::{
    api::{RevokeResponse, RevokeTarget, revoke_target},
    auth::Principal,
    error::ApiError,
    response::Success,
    transfers::test_seams,
};
use axum::Json;
use axum::extract::State;
use blobyard_contract::RepositoryError;
use std::sync::Arc;

fn seeded() -> test_seams::TransferFixture {
    test_seams::fixture(&["sessions:manage"])
}

fn faulted() -> test_seams::TransferFixture {
    let mut fixture = seeded();
    fixture.state.repository = Arc::new(crate::repository_fault_tests::FaultingRepository::new(
        fixture.state.repository.clone(),
        0,
    ));
    fixture
}

fn revoke_session(
    fixture: &test_seams::TransferFixture,
    principal: &Principal,
    request: &RevokeRequest,
    now: Result<u64, ApiError>,
) -> Result<Json<Success<RevokeResponse>>, ApiError> {
    revoke_target(
        &fixture.state,
        principal,
        RevokeTarget::CliSession(request.session_id.clone()),
        now,
    )
}

#[tokio::test]
async fn listing_returns_only_active_workspace_sessions_without_credentials() {
    let fixture = seeded();
    let result = list(
        State(fixture.state.clone()),
        Principal(fixture.principal.clone()),
    )
    .await
    .expect("session list");
    let encoded = serde_json::to_value(&result.0).expect("serialize response");
    assert_eq!(encoded["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(encoded["data"][0]["id"], "session_fixture");
    assert_eq!(encoded["data"][0]["name"], "Fixture");
    assert_eq!(encoded["data"][0]["platform"], "test");
    assert_eq!(encoded["data"][0]["version"], env!("CARGO_PKG_VERSION"));
    let serialized = encoded.to_string();
    assert!(!serialized.contains("session-secret"));
    assert!(!serialized.contains("secretHash"));
    assert!(
        fixture
            .state
            .repository
            .list_api_tokens()
            .expect("token list")
            .is_empty()
    );
}

#[test]
fn revocation_invalidates_the_backing_credential_and_is_idempotent() {
    let fixture = seeded();
    let principal = Principal(fixture.principal.clone());
    let request = RevokeRequest {
        session_id: "session_fixture".to_owned(),
    };
    let response = revoke_session(&fixture, &principal, &request, Ok(20)).expect("session revoke");
    assert_eq!(
        serde_json::to_value(&response.0).expect("serialize revoke")["data"]["status"],
        "revoked"
    );
    assert_eq!(
        fixture
            .state
            .repository
            .authenticate_api_token(&crate::auth::hash("secret"), 21),
        Err(RepositoryError::NotFound)
    );
    assert!(
        fixture
            .state
            .repository
            .list_cli_sessions("workspace_fixture")
            .expect("session list")
            .is_empty()
    );
    let replay = revoke_session(&fixture, &principal, &request, Ok(21)).expect("revoke replay");
    assert_eq!(
        serde_json::to_value(&replay.0).expect("serialize replay")["data"]["status"],
        "already_revoked"
    );
    let missing = revoke_session(
        &fixture,
        &principal,
        &RevokeRequest {
            session_id: "session_missing".to_owned(),
        },
        Ok(21),
    )
    .expect("missing session");
    assert_eq!(
        serde_json::to_value(&missing.0).expect("serialize missing")["data"]["status"],
        "invalid"
    );
}

#[test]
fn revocation_clock_failure_does_not_mutate_the_session() {
    let fixture = seeded();
    assert!(
        revoke_session(
            &fixture,
            &Principal(fixture.principal.clone()),
            &RevokeRequest {
                session_id: "session_fixture".to_owned(),
            },
            Err(crate::error::ApiError::internal()),
        )
        .is_err()
    );
    assert_eq!(
        fixture
            .state
            .repository
            .list_cli_sessions("workspace_fixture")
            .expect("session list")
            .len(),
        1
    );
}

#[test]
fn revocation_maps_unavailable_repository_failures() {
    let fixture = faulted();
    assert!(
        revoke_session(
            &fixture,
            &Principal(fixture.principal.clone()),
            &RevokeRequest {
                session_id: "session_fixture".to_owned(),
            },
            Ok(20),
        )
        .is_err()
    );
}

#[tokio::test]
async fn listing_maps_unavailable_repository_failures() {
    let fixture = faulted();
    assert!(
        list(State(fixture.state), Principal(fixture.principal),)
            .await
            .is_err()
    );
}
