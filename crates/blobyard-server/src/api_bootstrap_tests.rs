#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{BootstrapRequest, bootstrap_error, exchange_bootstrap, exchange_bootstrap_at};
use crate::{Repository, auth, repository_fault_tests::FaultingRepository, test_support};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use blobyard_contract::RepositoryError;
use blobyard_core::{GeneratedSecretKind, SecretString};
use std::sync::Arc;

#[test]
fn bootstrap_failures_conceal_invalid_authority_only() {
    for error in [RepositoryError::NotFound, RepositoryError::InvalidInput] {
        assert_eq!(
            bootstrap_error(error).into_response().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    for (error, expected) in [
        (RepositoryError::Conflict, StatusCode::CONFLICT),
        (
            RepositoryError::SchemaTooNew,
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            RepositoryError::Unavailable,
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ] {
        assert_eq!(bootstrap_error(error).into_response().status(), expected);
    }
}

#[tokio::test]
async fn bootstrap_exchange_reports_audit_failure() {
    let mut state = state_with_bootstrap();
    let inner = Arc::clone(&state.repository);
    state.repository = Arc::new(FaultingRepository::new(inner, 1)) as Arc<dyn Repository>;
    let result = exchange_bootstrap(State(state), Ok(Json(request()))).await;
    let error = result.err().expect("audit failure");
    assert_eq!(
        error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn bootstrap_clock_failure_preserves_the_one_time_authority() {
    let state = state_with_bootstrap();
    let result = exchange_bootstrap_at(
        &state,
        request(),
        auth::generate_token(GeneratedSecretKind::AccessToken),
        Err(crate::error::ApiError::internal()),
    );
    let error = result.err().expect("clock failure");
    assert_eq!(
        error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    drop(
        exchange_bootstrap_at(
            &state,
            request(),
            auth::generate_token(GeneratedSecretKind::AccessToken),
            Ok(1),
        )
        .expect("bootstrap remains usable"),
    );
}

#[tokio::test]
async fn bootstrap_rejects_each_invalid_client_identity_field_before_exchange() {
    let state = state_with_bootstrap();
    let mut requests = Vec::new();
    let mut invalid_name = request();
    invalid_name.name.clear();
    requests.push(invalid_name);
    let mut invalid_platform = request();
    invalid_platform.platform.clear();
    requests.push(invalid_platform);
    let mut invalid_version = request();
    invalid_version.version.clear();
    requests.push(invalid_version);
    for request in requests {
        assert!(
            exchange_bootstrap(State(state.clone()), Ok(Json(request)))
                .await
                .is_err()
        );
    }
    drop(
        exchange_bootstrap(State(state), Ok(Json(request())))
            .await
            .expect("invalid requests preserve bootstrap authority"),
    );
}

fn state_with_bootstrap() -> crate::api::AppState {
    let root = tempfile::tempdir().expect("temporary directory");
    let staging = root.path().join("staging");
    std::fs::create_dir(&staging).expect("staging directory");
    let state = test_support::filesystem_state(&root, staging);
    state
        .repository
        .install_bootstrap(&auth::hash("bootstrap"))
        .expect("bootstrap token");
    state
        .repository
        .create_workspace(&state.default_workspace)
        .expect("default workspace");
    state
}

fn request() -> BootstrapRequest {
    BootstrapRequest {
        name: "Fixture".to_owned(),
        platform: "test".to_owned(),
        token: SecretString::new("bootstrap").expect("bootstrap secret"),
        version: "0.1.12".to_owned(),
    }
}
