//! CLI runner session, identity, local-only, and failure-path behavior.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, api_failure, ok, result_json};
use blobyard_api_client::{ApiCallError, RetryAdvice};
use blobyard_core::{BlobyardError, ErrorCode};

#[tokio::test]
async fn runner_generates_completion_and_redacts_debug_state() {
    let fixture = Fixture::new(&["blobyard", "completion", "bash"], Vec::new(), None, None);
    assert!(format!("{:?}", fixture.runner).contains("Runner"));
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("completion");
    let json = result_json(result);
    assert_eq!(json["data"]["shell"], "bash");
}

#[tokio::test]
async fn whoami_uses_environment_token_without_touching_saved_credentials() {
    let fixture = Fixture::new(
        &["blobyard", "whoami"],
        vec![ok(
            serde_json::json!({
                "principalId": "user_1",
                "principalType": "cli",
                "displayName": "Developer",
                "email": "developer@example.com",
                "defaultWorkspace": { "id": "workspace_1", "name": "Personal", "slug": "studio" },
                "scopes": ["object:read"]
            }),
            "req_identity",
        )],
        Some("ci-token"),
        Some("saved-refresh"),
    );
    fixture.store.fail_load();
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("identity");
    let json = result_json(result);
    assert_eq!(json["data"]["principalId"], "user_1");
    assert_eq!(json["data"]["email"], "developer@example.com");
    let requests = fixture.transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .bearer()
            .map(blobyard_core::SecretString::expose_secret),
        Some("ci-token")
    );
}

#[tokio::test]
async fn saved_refresh_rotates_before_authenticated_request() {
    let fixture = Fixture::new(
        &["blobyard", "whoami"],
        vec![
            ok(
                serde_json::json!({
                    "accessToken": "new-access",
                    "refreshToken": "new-refresh",
                    "expiresInSeconds": 900
                }),
                "req_refresh",
            ),
            ok(
                serde_json::json!({
                    "principalId": "user_1",
                    "principalType": "cli",
                    "displayName": "Developer",
                    "email": "developer@example.com",
                    "defaultWorkspace": { "id": "workspace_1", "name": "Personal", "slug": "studio" },
                    "scopes": []
                }),
                "req_identity",
            ),
        ],
        None,
        Some("old-refresh"),
    );
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("refreshed identity");
    assert_eq!(fixture.store.token().as_deref(), Some("new-refresh"));
    assert_eq!(fixture.store.saves(), 1);
    let requests = fixture.transport.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].endpoint(),
        blobyard_api_client::Endpoint::TokenRefresh
    );
    assert_eq!(
        requests[1]
            .bearer()
            .map(blobyard_core::SecretString::expose_secret),
        Some("new-access")
    );
}

#[tokio::test]
async fn authentication_failures_are_safe_and_distinct() {
    let missing = Fixture::new(&["blobyard", "whoami"], Vec::new(), None, None);
    let error = missing
        .runner
        .execute(&missing.command)
        .await
        .expect_err("auth required");
    assert_eq!(error.code(), ErrorCode::AuthRequired);

    let unreadable = Fixture::new(&["blobyard", "whoami"], Vec::new(), None, Some("refresh"));
    unreadable.store.fail_load();
    assert_eq!(
        unreadable
            .runner
            .execute(&unreadable.command)
            .await
            .expect_err("store failure")
            .code(),
        ErrorCode::InternalError
    );

    let rejected = Fixture::new(
        &["blobyard", "whoami"],
        vec![api_failure(ErrorCode::InvalidToken, "req_invalid")],
        None,
        Some("refresh"),
    );
    assert_eq!(
        rejected
            .runner
            .execute(&rejected.command)
            .await
            .expect_err("refresh rejected")
            .code(),
        ErrorCode::InvalidToken
    );

    let save_failure = Fixture::new(
        &["blobyard", "whoami"],
        vec![ok(
            serde_json::json!({
                "accessToken": "access",
                "refreshToken": "rotated",
                "expiresInSeconds": 900
            }),
            "req_refresh",
        )],
        None,
        Some("refresh"),
    );
    save_failure.store.fail_save();
    assert_eq!(
        save_failure
            .runner
            .execute(&save_failure.command)
            .await
            .expect_err("save failure")
            .code(),
        ErrorCode::InternalError
    );
}

#[tokio::test]
async fn logout_revokes_the_right_credential_source() {
    let environment = Fixture::new(
        &["blobyard", "logout"],
        vec![ok(serde_json::json!({}), "req_logout")],
        Some("ci-token"),
        Some("saved-refresh"),
    );
    environment
        .runner
        .execute(&environment.command)
        .await
        .expect("environment logout");
    assert_eq!(environment.store.deletes(), 0);
    assert_eq!(environment.store.token().as_deref(), Some("saved-refresh"));

    let saved = Fixture::new(
        &["blobyard", "logout"],
        vec![ok(serde_json::json!({}), "req_logout")],
        None,
        Some("saved-refresh"),
    );
    saved
        .runner
        .execute(&saved.command)
        .await
        .expect("saved logout");
    assert_eq!(saved.store.deletes(), 1);
    assert_eq!(saved.store.token(), None);

    let missing = Fixture::new(&["blobyard", "logout"], Vec::new(), None, None);
    assert_eq!(
        missing
            .runner
            .execute(&missing.command)
            .await
            .expect_err("missing token")
            .code(),
        ErrorCode::AuthRequired
    );

    let unreadable = Fixture::new(
        &["blobyard", "logout"],
        Vec::new(),
        None,
        Some("saved-refresh"),
    );
    unreadable.store.fail_load();
    assert_eq!(
        unreadable
            .runner
            .execute(&unreadable.command)
            .await
            .expect_err("unreadable token")
            .code(),
        ErrorCode::InternalError
    );
}

#[tokio::test]
async fn logout_preserves_saved_token_when_remote_or_local_delete_fails() {
    let remote = Fixture::new(
        &["blobyard", "logout"],
        vec![api_failure(ErrorCode::Forbidden, "req_forbidden")],
        None,
        Some("saved-refresh"),
    );
    assert_eq!(
        remote
            .runner
            .execute(&remote.command)
            .await
            .expect_err("remote failure")
            .code(),
        ErrorCode::Forbidden
    );
    assert_eq!(remote.store.token().as_deref(), Some("saved-refresh"));

    let local = Fixture::new(
        &["blobyard", "logout"],
        vec![ok(serde_json::json!({}), "req_logout")],
        None,
        Some("saved-refresh"),
    );
    local.store.fail_delete();
    assert_eq!(
        local
            .runner
            .execute(&local.command)
            .await
            .expect_err("delete failure")
            .code(),
        ErrorCode::InternalError
    );
}

#[test]
fn transport_error_fixture_remains_redaction_safe() {
    let error = ApiCallError::new(
        BlobyardError::from_code(ErrorCode::NetworkError),
        RetryAdvice::Never,
    );
    assert!(!format!("{error:?}").contains("token"));
}
