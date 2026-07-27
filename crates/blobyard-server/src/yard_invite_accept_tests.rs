#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    acceptance_environment, acceptance_error, invitation as accept_invitation, select_environment,
};
use crate::{
    Repository, repository_fault_tests::FaultingRepository, test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_contract::{
    RepositoryError, YardEnvironmentKind, YardEnvironmentRecord, YardEnvironmentStatus,
    YardGuestInviteRecord, YardGuestInviteStatus,
};
use blobyard_core::{SecretString, Slug};
use http_body_util::BodyExt;
use std::sync::Arc;

fn invitation(environment_id: Option<&str>) -> YardGuestInviteRecord {
    YardGuestInviteRecord {
        id: "ygi_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        yard_id: "yard_fixture".to_owned(),
        environment_id: environment_id.map(ToOwned::to_owned),
        email: "guest@example.test".to_owned(),
        status: YardGuestInviteStatus::Pending,
        accepted_subject_id: None,
        grant_id: "grant_fixture".to_owned(),
        app_roles: Vec::new(),
        created_at_ms: 1,
        expires_at_ms: 10,
        accepted_at_ms: None,
        revoked_at_ms: None,
    }
}

fn environment(id: &str, kind: YardEnvironmentKind) -> YardEnvironmentRecord {
    YardEnvironmentRecord {
        id: id.to_owned(),
        yard_id: "yard_fixture".to_owned(),
        name: Slug::new(id).expect("environment name"),
        kind,
        status: YardEnvironmentStatus::Active,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

#[test]
fn acceptance_selects_only_the_invited_active_environment() {
    let production = environment("production", YardEnvironmentKind::Production);
    let preview = environment("preview", YardEnvironmentKind::Preview);
    assert_eq!(
        select_environment(vec![preview.clone(), production], &invitation(None)),
        Some("production".to_owned())
    );
    assert_eq!(
        select_environment(
            vec![preview.clone()],
            &invitation(Some(preview.id.as_str()))
        ),
        None
    );
    let mut deleted = preview;
    deleted.status = YardEnvironmentStatus::Deleted;
    assert_eq!(
        select_environment(vec![deleted], &invitation(Some("preview"))),
        None
    );
}

#[test]
fn acceptance_environment_conceals_missing_yards_and_surfaces_provider_failures() {
    let fixture = test_seams::fixture(&["yard:read"]);
    assert!(matches!(
        acceptance_environment(&fixture.state, &invitation(None)),
        Ok(None)
    ));
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut state = fixture.state.clone();
    state.repository = Arc::new(FaultingRepository::new(inner, 0));
    assert_eq!(
        error_status(acceptance_environment(&state, &invitation(None))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let mut malformed = invitation(None);
    malformed.yard_id.clear();
    assert!(matches!(
        acceptance_environment(&fixture.state, &malformed),
        Ok(None)
    ));
}

#[tokio::test]
async fn acceptance_repository_errors_are_uniformly_concealed() {
    for error in [
        RepositoryError::NotFound,
        RepositoryError::Conflict,
        RepositoryError::InvalidInput,
    ] {
        let response = acceptance_error(error).expect("concealed response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("response body")
            .to_bytes();
        assert!(
            body.windows(b"Invalid invitation".len())
                .any(|window| window == b"Invalid invitation")
        );
    }
    for error in [RepositoryError::Unavailable, RepositoryError::SchemaTooNew] {
        assert_eq!(
            error_status(acceptance_error(error)),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[tokio::test]
async fn acceptance_conceals_missing_targets_and_repository_rejections() {
    let fixture = test_seams::fixture(&["yard:read"]);
    let started = super::super::test_support::start_yard(&fixture.state);
    let yard = started.yard;
    let continuation = crate::yard_session_contracts::issue_invitation(
        &fixture.state.yard_continuation_key,
        &yard.host_label,
        "/",
        1,
        10,
    )
    .expect("continuation");
    let claims = crate::yard_session_contracts::verify_invitation(
        &fixture.state.yard_continuation_key,
        &continuation,
        1,
    )
    .expect("claims");
    let token = SecretString::new(format!("bygi_{}", "a".repeat(64))).expect("token");
    let mut record = invitation(Some("yardenv_unknown"));
    record.workspace_id = yard.workspace_id.clone();
    record.project_id = yard.project_id.clone();
    record.yard_id = yard.id.clone();

    assert_missing_target_is_concealed(&fixture.state, &token, &continuation, &claims, &record)
        .await;
    record.environment_id = None;
    assert_acceptance_provider_failures(&fixture.state, &token, &continuation, &claims, &record);
}

async fn assert_missing_target_is_concealed(
    state: &crate::api::AppState,
    token: &SecretString,
    continuation: &SecretString,
    claims: &crate::yard_session_contracts::ContinuationClaims,
    record: &YardGuestInviteRecord,
) {
    let missing = accept_invitation(state, token, continuation, claims, record, 1)
        .expect("concealed response");
    assert_eq!(missing.status(), StatusCode::OK);
    assert!(
        missing
            .into_body()
            .collect()
            .await
            .expect("response body")
            .to_bytes()
            .windows(b"Invalid invitation".len())
            .any(|window| window == b"Invalid invitation")
    );
}

fn assert_acceptance_provider_failures(
    state: &crate::api::AppState,
    token: &SecretString,
    continuation: &SecretString,
    claims: &crate::yard_session_contracts::ContinuationClaims,
    record: &YardGuestInviteRecord,
) {
    let inner: Arc<dyn Repository> = Arc::clone(&state.repository);
    let mut environment_fault = state.clone();
    environment_fault.repository = Arc::new(FaultingRepository::new(Arc::clone(&inner), 0));
    assert_eq!(
        error_status(accept_invitation(
            &environment_fault,
            token,
            continuation,
            claims,
            record,
            1,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    assert_eq!(
        error_status(accept_invitation(
            state,
            token,
            continuation,
            claims,
            record,
            u64::MAX,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let mut faulted = state.clone();
    faulted.repository = Arc::new(FaultingRepository::new(inner, 1));
    assert_eq!(
        error_status(accept_invitation(
            &faulted,
            token,
            continuation,
            claims,
            record,
            1,
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
