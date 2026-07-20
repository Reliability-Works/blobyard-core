#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    CreateRequest, TokenSummary, create_at, create_with_clock, expiration, list_with_clock,
    normalize_name, normalize_scopes, resolve_binding,
};
use crate::{
    api::{RevokeTarget, revoke_target},
    error::ApiError,
    repository_fault_tests::FaultingRepository,
    transfers::test_seams,
};
use axum::response::IntoResponse;
use blobyard_contract::LocalApiTokenRecord;
use blobyard_core::Slug;
use std::sync::Arc;

fn cleanup_binding(
    state: &crate::api::AppState,
    principal: &LocalApiTokenRecord,
    workspace: &str,
    project: &str,
) -> Result<Option<(String, String)>, ApiError> {
    resolve_binding(
        state,
        &crate::auth::Principal(principal.clone()),
        &["object:write".to_owned()],
        Some(Slug::new(workspace).expect("workspace")),
        Some(Slug::new(project).expect("project")),
    )
}

#[test]
fn token_names_scopes_and_expiry_fail_closed() {
    assert_eq!(
        normalize_name("  Build agent  ").expect("name"),
        "Build agent"
    );
    for value in ["", "x", &"x".repeat(81), "line\nbreak"] {
        assert!(normalize_name(value).is_err(), "accepted {value:?}");
    }

    let caller = vec![
        "object:read".to_owned(),
        "object:write".to_owned(),
        "tokens:manage".to_owned(),
    ];
    assert_eq!(
        normalize_scopes(
            &[
                "tokens:manage".to_owned(),
                "object:read".to_owned(),
                "object:read".to_owned(),
            ],
            &caller,
        )
        .expect("scopes"),
        vec!["object:read".to_owned(), "tokens:manage".to_owned()]
    );
    assert!(normalize_scopes(&[], &caller).is_err());
    assert!(normalize_scopes(&["unknown".to_owned()], &caller).is_err());
    assert!(normalize_scopes(&["audit:read".to_owned()], &caller).is_err());

    assert_eq!(expiration(1, 7).expect("expiry"), 604_800_001);
    assert!(expiration(1, 1).is_err());
    assert!(expiration(u64::MAX, 90).is_err());
}

#[test]
fn project_binding_is_available_only_for_cleanup_tokens() {
    let fixture = test_seams::fixture(&["object:write", "tokens:manage"]);
    assert!(
        resolve_binding(
            &fixture.state,
            &crate::auth::Principal(fixture.principal.clone()),
            &[],
            None,
            None
        )
        .expect("unbound")
        .is_none()
    );
    assert!(
        resolve_binding(
            &fixture.state,
            &crate::auth::Principal(fixture.principal.clone()),
            &["object:read".to_owned()],
            Some(Slug::new("fixture").expect("workspace")),
            Some(Slug::new("project").expect("project")),
        )
        .is_err()
    );
    for workspace in [None, Some(Slug::new("fixture").expect("workspace"))] {
        assert!(
            resolve_binding(
                &fixture.state,
                &crate::auth::Principal(fixture.principal.clone()),
                &["object:write".to_owned()],
                workspace,
                None,
            )
            .is_err()
        );
    }
    assert_eq!(
        cleanup_binding(&fixture.state, &fixture.principal, "fixture", "project").expect("binding"),
        Some(("workspace_fixture".to_owned(), "project_fixture".to_owned()))
    );
    let _upload_id = fixture.seed_foreign_upload();
    assert!(cleanup_binding(&fixture.state, &fixture.principal, "foreign", "foreign").is_err());
}

#[test]
fn project_binding_conceals_missing_resources_and_maps_repository_failures() {
    let fixture = test_seams::fixture(&["object:write", "tokens:manage"]);
    assert!(cleanup_binding(&fixture.state, &fixture.principal, "fixture", "missing").is_err());
    for failure_index in 0..=1 {
        let mut state = fixture.state.clone();
        state.repository = Arc::new(FaultingRepository::new(
            Arc::clone(&state.repository),
            failure_index,
        ));
        let error = cleanup_binding(&state, &fixture.principal, "fixture", "project")
            .expect_err("repository failure");
        assert_eq!(
            error.into_response().status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[test]
fn creation_returns_the_raw_token_once_and_persists_only_redacted_metadata() {
    let fixture = test_seams::fixture(&["object:read", "tokens:manage"]);
    let principal = crate::auth::Principal(fixture.principal.clone());
    let response = create_at(
        &fixture.state,
        &principal,
        CreateRequest {
            expires_in_days: 7,
            name: "Build agent".to_owned(),
            project: None,
            scopes: vec!["object:read".to_owned()],
            workspace: None,
        },
        10,
    )
    .expect("created");
    let value = serde_json::to_value(response.0).expect("response JSON");
    assert!(
        value["data"]["rawToken"]
            .as_str()
            .is_some_and(|token| token.starts_with("byd_pat_"))
    );
    assert_eq!(value["data"]["expiresAt"], 604_800_010_u64);
    assert!(value["data"].get("workspaceId").is_none());

    let tokens = fixture.state.repository.list_api_tokens().expect("tokens");
    let created = tokens
        .iter()
        .find(|token| token.name == "Build agent")
        .expect("created metadata");
    assert!(
        created
            .secret_hash
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
    assert_eq!(created.token_prefix.len(), 16);
    assert!(!created.secret_hash.contains("byd_pat_"));
}

#[test]
fn summaries_derive_active_expired_and_revoked_states_without_secrets() {
    let base = LocalApiTokenRecord {
        id: "token_summary".to_owned(),
        name: "Summary".to_owned(),
        token_prefix: "byd_pat_fixture".to_owned(),
        secret_hash: "00".repeat(32),
        scopes: vec!["object:read".to_owned()],
        workspace_id: "workspace_fixture".to_owned(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: 10,
        last_used_at_ms: Some(2),
        revoked_at_ms: None,
    };
    assert_eq!(TokenSummary::from_record(base.clone(), 9).status, "active");
    assert_eq!(
        TokenSummary::from_record(base.clone(), 10).status,
        "expired"
    );
    let mut revoked = base;
    revoked.revoked_at_ms = Some(3);
    assert_eq!(TokenSummary::from_record(revoked, 10).status, "revoked");
}

#[test]
fn missing_token_management_scope_is_forbidden_before_creation() {
    let fixture = test_seams::fixture(&["object:read"]);
    let result = create_at(
        &fixture.state,
        &crate::auth::Principal(fixture.principal.clone()),
        CreateRequest {
            expires_in_days: 7,
            name: "Build agent".to_owned(),
            project: None,
            scopes: vec!["object:read".to_owned()],
            workspace: None,
        },
        10,
    );
    let error = result.err().expect("token creation denied");
    assert_eq!(
        error.into_response().status(),
        axum::http::StatusCode::FORBIDDEN
    );
}

#[test]
fn token_clock_failures_return_internal_errors_without_mutation() {
    let fixture = test_seams::fixture(&["object:read", "tokens:manage"]);
    let principal = crate::auth::Principal(fixture.principal.clone());
    let request = || CreateRequest {
        expires_in_days: 7,
        name: "Build agent".to_owned(),
        project: None,
        scopes: vec!["object:read".to_owned()],
        workspace: None,
    };
    for result in [
        create_with_clock(
            &fixture.state,
            &principal,
            request(),
            Err(ApiError::internal()),
        )
        .map(|_response| ()),
        list_with_clock(&fixture.state, &principal, Err(ApiError::internal())).map(|_response| ()),
        revoke_target(
            &fixture.state,
            &principal,
            RevokeTarget::ApiToken("token_fixture".to_owned()),
            Err(ApiError::internal()),
        )
        .map(|_response| ()),
    ] {
        assert_eq!(
            result.expect_err("clock failure").into_response().status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    assert_eq!(
        fixture
            .state
            .repository
            .list_api_tokens()
            .expect("tokens")
            .len(),
        0
    );
}
