#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    resolved_deploys, resolved_invitation, resolved_yard, target_is_live, values,
    yard_matches_invitation,
};
use crate::{
    Repository, repository_fault_tests::FaultingRepository, test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_contract::{
    RepositoryError, WebYardRecord, WebYardStatus, YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS,
    YardDeployRecord, YardDeployStatus, YardGuestInviteRecord, YardGuestInviteStatus,
};
use blobyard_core::Slug;
use std::sync::Arc;

fn invitation() -> YardGuestInviteRecord {
    YardGuestInviteRecord {
        id: "ygi_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        yard_id: "yard_fixture".to_owned(),
        environment_id: None,
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

fn yard() -> WebYardRecord {
    WebYardRecord {
        id: "yard_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        name: Slug::new("fixture").expect("yard name"),
        host_label: "fixture-host".to_owned(),
        current_deploy_id: Some("deploy_fixture".to_owned()),
        status: WebYardStatus::Active,
        created_at_ms: 1,
        updated_at_ms: 1,
        deleted_at_ms: None,
    }
}

fn deploy(status: YardDeployStatus, host_label: &str) -> YardDeployRecord {
    YardDeployRecord {
        id: "deploy_fixture".to_owned(),
        yard_id: "yard_fixture".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        project_id: "project_fixture".to_owned(),
        client_deploy_id: "client_fixture".to_owned(),
        manifest_root: ".blobyard-yard/yard_fixture/client_fixture/".to_owned(),
        deployment_host_label: host_label.to_owned(),
        spa: false,
        clean_urls: false,
        status,
        created_at_ms: 1,
        finalised_at_ms: Some(1),
        file_count: 1,
        total_bytes: 1,
    }
}

#[test]
fn repository_resolution_errors_are_concealed_or_internal() {
    for error in [RepositoryError::NotFound, RepositoryError::InvalidInput] {
        assert!(matches!(resolved_invitation(Err(error)), Ok(None)));
        assert!(matches!(resolved_yard(Err(error)), Ok(None)));
        assert!(matches!(resolved_deploys(Err(error), "host"), Ok(false)));
    }
    for error in [
        RepositoryError::Conflict,
        RepositoryError::SchemaTooNew,
        RepositoryError::Unavailable,
    ] {
        assert_eq!(
            error_status(resolved_invitation(Err(error))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            error_status(resolved_yard(Err(error))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            error_status(resolved_deploys(Err(error), "host")),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[test]
fn invitation_targets_require_live_matching_yards_and_deploys() {
    let invitation = invitation();
    let yard = yard();
    assert!(yard_matches_invitation(&yard, &invitation));
    let mut foreign = invitation.clone();
    foreign.workspace_id = "workspace_other".to_owned();
    assert!(!yard_matches_invitation(&yard, &foreign));
    let mut deleted = yard;
    deleted.status = WebYardStatus::Deleted;
    assert!(!yard_matches_invitation(&deleted, &invitation));
    assert!(matches!(
        resolved_deploys(
            Ok(vec![deploy(YardDeployStatus::Live, "deploy-host")]),
            "deploy-host"
        ),
        Ok(true)
    ));
    assert!(matches!(
        resolved_deploys(
            Ok(vec![deploy(YardDeployStatus::Superseded, "deploy-host")]),
            "deploy-host"
        ),
        Ok(true)
    ));
    assert!(matches!(
        resolved_deploys(
            Ok(vec![deploy(YardDeployStatus::Failed, "deploy-host")]),
            "deploy-host"
        ),
        Ok(false)
    ));
}

#[test]
fn malformed_and_unknown_invitation_values_are_concealed() {
    let fixture = test_seams::fixture(&["yard:read"]);
    assert!(matches!(
        values(&fixture.state, String::new(), "continuation".to_owned(), 1),
        Ok(None)
    ));
    assert!(matches!(
        values(
            &fixture.state,
            "bygi_bad".to_owned(),
            "continuation".to_owned(),
            1
        ),
        Ok(None)
    ));
    let token = format!("bygi_{}", "a".repeat(64));
    assert!(matches!(
        values(&fixture.state, token.clone(), String::new(), 1),
        Ok(None)
    ));
    assert!(matches!(
        values(
            &fixture.state,
            token.clone(),
            "invalid-continuation".to_owned(),
            1
        ),
        Ok(None)
    ));
    let continuation = crate::yard_session_contracts::issue_invitation(
        &fixture.state.yard_continuation_key,
        "fixture-host",
        "/",
        1,
        10,
    )
    .expect("continuation");
    assert!(matches!(
        values(
            &fixture.state,
            token.clone(),
            continuation.expose_secret().to_owned(),
            1
        ),
        Ok(None)
    ));
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut faulted = fixture.state.clone();
    faulted.repository = Arc::new(FaultingRepository::new(inner, 0));
    assert_eq!(
        error_status(values(
            &faulted,
            token,
            continuation.expose_secret().to_owned(),
            1
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn live_target_resolution_conceals_missing_mismatched_and_inactive_targets() {
    let fixture = test_seams::fixture(&["yard:read"]);
    assert!(matches!(
        target_is_live(&fixture.state, &invitation(), "fixture-host"),
        Ok(false)
    ));

    let started = super::super::test_support::start_yard(&fixture.state);
    let mut record = invitation();
    record.workspace_id.clone_from(&started.yard.workspace_id);
    record.project_id.clone_from(&started.yard.project_id);
    record.yard_id.clone_from(&started.yard.id);
    let mut foreign = record.clone();
    foreign.workspace_id = "workspace_other".to_owned();
    assert!(matches!(
        target_is_live(&fixture.state, &foreign, &started.yard.host_label),
        Ok(false)
    ));
    assert!(matches!(
        target_is_live(&fixture.state, &record, &started.yard.host_label),
        Ok(true)
    ));
    assert!(matches!(
        target_is_live(
            &fixture.state,
            &record,
            &started.deploy.deployment_host_label
        ),
        Ok(false)
    ));

    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut faulted = fixture.state.clone();
    faulted.repository = Arc::new(FaultingRepository::new(inner, 0));
    assert_eq!(
        error_status(target_is_live(&faulted, &record, &started.yard.host_label,)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn invitation_values_conceal_an_inactive_deployment_target() {
    let fixture = test_seams::fixture(&["yard:read"]);
    let started = super::super::test_support::start_yard(&fixture.state);
    let raw_token = format!("bygi_{}", "b".repeat(64));
    let expires_at_ms = 1 + YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS;
    let record = super::super::test_support::create_invitation(
        &fixture.state,
        &started.yard,
        &raw_token,
        expires_at_ms,
    );
    let continuation = crate::yard_session_contracts::issue_invitation(
        &fixture.state.yard_continuation_key,
        &started.deploy.deployment_host_label,
        "/",
        1,
        expires_at_ms,
    )
    .expect("continuation");
    assert!(matches!(
        values(
            &fixture.state,
            raw_token,
            continuation.expose_secret().to_owned(),
            2,
        ),
        Ok(None)
    ));
    assert_eq!(record.status, YardGuestInviteStatus::Pending);
}
