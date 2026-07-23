#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{require_users_manage, summary};
use crate::{
    auth::Principal,
    contract_test_support::{assert_error, response_json, send, send_as},
    repository_fault_tests::FaultingRepository,
    transfers::test_seams::{self, TransferFixture},
};
use axum::http::StatusCode;
use blobyard_contract::{LocalUserListing, LocalUserStatus};
use std::sync::Arc;

/// Far enough ahead of the wall clock and before the sentinel key expiry.
const PROBE_NOW_MS: u64 = 9_223_372_036_854_775_806;

fn manager_fixture() -> TransferFixture {
    test_seams::fixture(&["audit:read", "object:read", "users:manage"])
}

#[tokio::test]
async fn user_routes_create_list_reset_deactivate_and_audit_without_secret_disclosure() {
    let fixture = manager_fixture();
    let (user_id, first_key) = create_user(&fixture).await;
    assert_redacted_list(&fixture, &user_id, &first_key).await;
    let second_key = reset_key(&fixture, &user_id, &first_key).await;
    deactivate_and_verify(&fixture, &user_id, &second_key).await;
    assert_user_audit(&fixture);
}

async fn create_user(fixture: &TransferFixture) -> (String, String) {
    let created = send(
        fixture,
        "POST",
        "/v1/users",
        br#"{"displayName":"Ada Lovelace","email":" ada@example.test ","workspace":"fixture"}"#,
        false,
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let created = response_json(created).await;
    let login_key = created["data"]["loginKey"]
        .as_str()
        .expect("raw sign-in key")
        .to_owned();
    assert!(login_key.starts_with("byuk_"));
    let prefix = login_key.chars().take(16).collect::<String>();
    assert_eq!(created["data"]["loginKeyPrefix"], prefix);
    let user = &created["data"]["user"];
    assert_eq!(user["displayName"], "Ada Lovelace");
    assert_eq!(user["email"], "ada@example.test");
    assert_eq!(user["status"], "active");
    assert_eq!(user["workspaceId"], "workspace_fixture");
    assert_eq!(user["loginKeyPrefix"], prefix);
    (user["id"].as_str().expect("user id").to_owned(), login_key)
}

async fn assert_redacted_list(fixture: &TransferFixture, user_id: &str, login_key: &str) {
    let listed =
        response_json(send(fixture, "GET", "/v1/users?workspace=fixture", b"", false).await).await;
    let users = listed["data"]["users"].as_array().expect("user list");
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["id"], user_id);
    assert_eq!(
        users[0]["loginKeyPrefix"],
        login_key.chars().take(16).collect::<String>()
    );
    let listed_text = listed.to_string();
    assert!(!listed_text.contains(login_key));
    assert!(!listed_text.contains("\"loginKey\""));
    assert!(!listed_text.contains("secretHash"));
}

async fn reset_key(fixture: &TransferFixture, user_id: &str, first_key: &str) -> String {
    let reset = response_json(
        send(
            fixture,
            "POST",
            "/v1/users/reset-key",
            format!(r#"{{"userId":"{user_id}"}}"#).as_bytes(),
            false,
        )
        .await,
    )
    .await;
    let replacement = reset["data"]["loginKey"]
        .as_str()
        .expect("replacement key")
        .to_owned();
    assert!(replacement.starts_with("byuk_"));
    assert_ne!(replacement, first_key);
    assert_eq!(
        reset["data"]["loginKeyPrefix"],
        replacement.chars().take(16).collect::<String>()
    );
    let repository = &fixture.state.repository;
    assert!(
        repository
            .authenticate_local_user_key(&crate::auth::hash(first_key), PROBE_NOW_MS)
            .is_err(),
        "the replaced key must stop authenticating"
    );
    assert_eq!(
        repository
            .authenticate_local_user_key(&crate::auth::hash(&replacement), PROBE_NOW_MS)
            .expect("replacement authenticates")
            .id,
        user_id
    );
    replacement
}

async fn deactivate_and_verify(fixture: &TransferFixture, user_id: &str, active_key: &str) {
    let body = format!(r#"{{"userId":"{user_id}"}}"#);
    let deactivated = send(
        fixture,
        "POST",
        "/v1/users/deactivate",
        body.as_bytes(),
        false,
    )
    .await;
    assert_eq!(deactivated.status(), StatusCode::OK);
    for path in ["/v1/users/deactivate", "/v1/users/reset-key"] {
        assert_error(
            send(fixture, "POST", path, body.as_bytes(), false).await,
            StatusCode::CONFLICT,
            "CONFLICT",
        )
        .await;
    }
    assert!(
        fixture
            .state
            .repository
            .authenticate_local_user_key(&crate::auth::hash(active_key), PROBE_NOW_MS)
            .is_err(),
        "deactivation must revoke the active key"
    );
    let listed =
        response_json(send(fixture, "GET", "/v1/users?workspace=fixture", b"", false).await).await;
    let users = listed["data"]["users"].as_array().expect("user list");
    assert_eq!(users[0]["status"], "deactivated");
    assert_eq!(users[0]["loginKeyPrefix"], serde_json::Value::Null);
}

fn assert_user_audit(fixture: &TransferFixture) {
    let audit = fixture
        .state
        .repository
        .list_audit(&fixture.principal.workspace_id, None, 10)
        .expect("audit");
    let actions = audit
        .items
        .iter()
        .map(|event| event.action.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        ["user.deactivated", "user.login_key_reset", "user.created"]
    );
}

#[tokio::test]
async fn user_routes_conceal_foreign_workspaces_and_reject_duplicates() {
    let fixture = manager_fixture();
    let created = send(
        &fixture,
        "POST",
        "/v1/users",
        br#"{"displayName":"Ada Lovelace","email":"ada@example.test","workspace":"fixture"}"#,
        false,
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    assert_error(
        send(
            &fixture,
            "POST",
            "/v1/users",
            br#"{"displayName":"Duplicate email","email":"ada@example.test","workspace":"fixture"}"#,
            false,
        )
        .await,
        StatusCode::CONFLICT,
        "CONFLICT",
    )
    .await;
    fixture
        .state
        .repository
        .create_workspace(&blobyard_contract::WorkspaceRecord {
            id: "workspace_other".to_owned(),
            name: "Other".to_owned(),
            slug: blobyard_core::Slug::new("other".to_owned()).expect("slug"),
        })
        .expect("foreign workspace");
    for (method, path, body) in [
        (
            "POST",
            "/v1/users",
            br#"{"displayName":"Foreign","workspace":"other"}"#.as_slice(),
        ),
        (
            "POST",
            "/v1/users",
            br#"{"displayName":"Missing","workspace":"missing"}"#.as_slice(),
        ),
        ("GET", "/v1/users?workspace=other", b"".as_slice()),
        ("GET", "/v1/users?workspace=missing", b"".as_slice()),
        (
            "POST",
            "/v1/users/reset-key",
            br#"{"userId":"user_missing"}"#.as_slice(),
        ),
        (
            "POST",
            "/v1/users/deactivate",
            br#"{"userId":"user_missing"}"#.as_slice(),
        ),
    ] {
        assert_error(
            send(&fixture, method, path, body, false).await,
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        )
        .await;
    }
}

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
        let mut state = fixture.state.clone();
        state.repository = Arc::new(FaultingRepository::new(Arc::clone(&state.repository), 1));
        assert_error(
            send_as(
                test_seams::fixture_router(&state),
                "secret",
                method,
                path,
                body,
            )
            .await,
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

#[tokio::test]
async fn user_mutations_propagate_clock_failures() {
    let fixture = manager_fixture();
    let principal = Principal(fixture.principal.clone());
    let clock = || Err(crate::error::ApiError::internal());
    let create = blobyard_api_client::CreateLocalUserRequest {
        workspace: blobyard_core::Slug::new("fixture".to_owned()).expect("slug"),
        display_name: "Valid name".to_owned(),
        email: None,
    };
    assert!(super::create_with_clock(&fixture.state, &principal, create, clock()).is_err());
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
