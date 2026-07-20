#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    Principal, authenticate, credential_error, generate_token, hash, validate_machine_record,
};
use axum::{
    extract::FromRequestParts,
    http::{HeaderValue, Request, StatusCode, header},
    response::IntoResponse,
};
use blobyard_contract::{
    AuditValue, CiAction, GithubOidcIdentity, LocalApiTokenRecord, LocalMachineSessionRecord,
    MachineSessionMintResult, NewAuditEvent, NewMachineSession, ObjectSource, ProjectRecord,
    RepositoryError,
};
use blobyard_core::{GeneratedSecretKind, SecretString, Slug};

fn principal(scopes: &[&str]) -> Principal {
    Principal(LocalApiTokenRecord {
        id: "auth_token_fixture".to_owned(),
        name: "Authentication fixture".to_owned(),
        token_prefix: "bya_fixture".to_owned(),
        secret_hash: hash("secret"),
        scopes: scopes.iter().copied().map(str::to_owned).collect(),
        workspace_id: "auth_workspace_fixture".to_owned(),
        project_id: None,
        created_at_ms: 1,
        expires_at_ms: i64::MAX as u64,
        last_used_at_ms: None,
        revoked_at_ms: None,
    })
}

#[test]
fn principals_require_an_exact_scope() {
    let principal = principal(&["object:read"]);
    assert!(principal.require("object:read").is_ok());
    assert_eq!(
        principal
            .require("object:write")
            .expect_err("missing scope")
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[test]
fn machine_principals_require_exact_actions_and_emit_ci_provenance() {
    let mut record = principal(&["upload"]).0;
    record.id = "machine_fixture".to_owned();
    record.project_id = Some("project_fixture".to_owned());
    let machine_principal = Principal(record);
    assert!(
        machine_principal
            .require_action(CiAction::Upload, "object:write")
            .is_ok()
    );
    assert_eq!(
        machine_principal
            .require_action(CiAction::Download, "object:read")
            .expect_err("missing action")
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(machine_principal.object_source(), ObjectSource::Ci);
    assert_eq!(principal(&[]).object_source(), ObjectSource::Cli);
}

#[test]
fn web_yard_authority_requires_both_machine_actions_but_accepts_either_user_read_scope() {
    let mut machine = principal(&["upload", "yard:manage"]).0;
    machine.id = "machine_yard_fixture".to_owned();
    machine.project_id = Some("project_fixture".to_owned());
    assert!(
        Principal(machine.clone())
            .require_actions(&[CiAction::Upload, CiAction::YardManage], "yard:manage")
            .is_ok()
    );
    machine.scopes.pop();
    assert_eq!(
        Principal(machine)
            .require_actions(&[CiAction::Upload, CiAction::YardManage], "yard:manage")
            .expect_err("missing yard action")
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert!(
        principal(&["yard:read"])
            .require_any(&["yard:read", "yard:manage"])
            .is_ok()
    );
    assert!(
        principal(&["yard:manage"])
            .require_any(&["yard:read", "yard:manage"])
            .is_ok()
    );
    assert_eq!(
        principal(&["object:read"])
            .require_any(&["yard:read", "yard:manage"])
            .expect_err("missing yard read authority")
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[test]
fn machine_token_and_session_records_must_match_exactly() {
    let mut token = principal(&["upload"]).0;
    token.id = "machine_fixture".to_owned();
    token.project_id = Some("project_fixture".to_owned());
    let session = LocalMachineSessionRecord {
        id: token.id.clone(),
        trust_id: "trust_fixture".to_owned(),
        workspace_id: token.workspace_id.clone(),
        project_id: "project_fixture".to_owned(),
        repository: "reliability-works/blobyard-core".to_owned(),
        git_ref: "refs/heads/main".to_owned(),
        run_id: "12345".to_owned(),
        run_attempt: Some("1".to_owned()),
        actions: vec![CiAction::Upload],
        created_at_ms: 1,
        expires_at_ms: 1_000,
        last_used_at_ms: None,
        revoked_at_ms: None,
    };
    assert!(validate_machine_record(&token, &session).is_ok());

    let mut variants = Vec::new();
    let mut id = token.clone();
    id.id = "machine_other".to_owned();
    variants.push(id);
    let mut workspace = token.clone();
    workspace.workspace_id = "workspace_other".to_owned();
    variants.push(workspace);
    let mut project = token.clone();
    project.project_id = Some("project_other".to_owned());
    variants.push(project);
    let mut scopes = token;
    scopes.scopes.push("download".to_owned());
    variants.push(scopes);
    for variant in variants {
        assert_eq!(
            validate_machine_record(&variant, &session)
                .expect_err("mismatched machine record")
                .into_response()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

#[test]
fn token_generation_and_hashing_are_stable_and_fail_closed() {
    let token = generate_token(GeneratedSecretKind::AccessToken);
    assert!(token.expose_secret().starts_with("bya_"));
    assert_ne!(hash("one"), hash("two"));
}

#[test]
fn credential_failures_map_to_the_expected_public_status() {
    for error in [RepositoryError::NotFound, RepositoryError::InvalidInput] {
        assert_eq!(
            credential_error(error).into_response().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    for error in [
        RepositoryError::Conflict,
        RepositoryError::SchemaTooNew,
        RepositoryError::Unavailable,
    ] {
        assert_eq!(
            credential_error(error).into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    assert_eq!(
        super::test_seams::credential_failure_statuses(),
        [
            StatusCode::UNAUTHORIZED,
            StatusCode::UNAUTHORIZED,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::INTERNAL_SERVER_ERROR,
        ]
    );
}

#[test]
fn principal_authentication_propagates_clock_failure_before_repository_access() {
    let fixture = crate::transfers::test_seams::fixture(&["object:read"]);
    let error = authenticate(
        &fixture.state,
        &SecretString::new("secret").expect("secret"),
        Err(crate::error::ApiError::internal()),
    )
    .expect_err("clock failure");
    assert_eq!(
        error.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn deterministic_authentication_seam_uses_the_supplied_time() {
    let fixture = crate::transfers::test_seams::fixture(&["object:read"]);
    let principal = super::test_seams::authenticate_at(&fixture.state, "secret", 2)
        .expect("authenticated principal");
    assert_eq!(principal.0.id, fixture.principal.id);
    assert!(super::test_seams::authenticate_at(&fixture.state, "", 2).is_err());
    assert!(super::test_seams::authenticate_at(&fixture.state, "wrong", 2).is_err());

    let machine = LocalApiTokenRecord {
        id: "machine_orphan".to_owned(),
        name: "Orphan machine".to_owned(),
        token_prefix: "byd_ci_orphan".to_owned(),
        secret_hash: hash("machine-secret"),
        scopes: vec!["upload".to_owned()],
        workspace_id: fixture.principal.workspace_id.clone(),
        project_id: Some("project_fixture".to_owned()),
        created_at_ms: 3,
        expires_at_ms: 1_000,
        last_used_at_ms: None,
        revoked_at_ms: None,
    };
    fixture
        .state
        .repository
        .create_api_token(
            &machine,
            &NewAuditEvent {
                id: "audit_machine_orphan".to_owned(),
                workspace_id: machine.workspace_id.clone(),
                actor: "fixture".to_owned(),
                action: "api_token.created".to_owned(),
                request_id: "request_machine_orphan".to_owned(),
                target_type: "api_token".to_owned(),
                metadata: vec![("tokenId".to_owned(), AuditValue::String(machine.id.clone()))],
                created_at_ms: machine.created_at_ms,
            },
        )
        .expect("create orphan machine token");
    assert!(super::test_seams::authenticate_at(&fixture.state, "machine-secret", 4).is_err());
}

#[tokio::test]
async fn principal_extraction_rejects_wrong_schemes_and_empty_bearer_secrets() {
    let fixture = crate::transfers::test_seams::fixture(&["fixture"]);
    let invalid_text = HeaderValue::from_bytes(&[0xff]).expect("opaque header value");
    for authorization in [
        None,
        Some(HeaderValue::from_static("Basic secret")),
        Some(HeaderValue::from_static("Bearer ")),
        Some(invalid_text),
    ] {
        let mut request = Request::builder();
        if let Some(authorization) = authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        let (mut parts, ()) = request.body(()).expect("request").into_parts();
        let response = Principal::from_request_parts(&mut parts, &fixture.state)
            .await
            .expect_err("rejected authorization")
            .into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

#[path = "auth_machine_session_tests.rs"]
mod machine_sessions;
