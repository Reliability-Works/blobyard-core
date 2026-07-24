//! Web Yard browser-session management workflows.

use super::super::support::{api_failure, ok, result_json};
use super::access_tests::{access_fixture, documentation_yard_page};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

fn session(id: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "createdAt": "2026-07-24T09:00:00Z",
        "environmentId": "yardenv_documentation",
        "expiresAt": "2026-08-23T09:00:00Z",
        "hostLabel": "documentation-x",
        "id": id,
        "lastUsedAt": null,
        "status": status,
        "userDisplayName": "Avery Reader",
        "userId": "user_reader",
        "yardId": "yard_documentation"
    })
}

#[tokio::test]
async fn yard_sessions_list_auto_selects_one_yard_and_preserves_metadata() {
    let fixture = access_fixture(
        &["yard-sessions", "list"],
        vec![
            documentation_yard_page(),
            ok(
                serde_json::json!({
                    "sessions": [
                        session("byys_active", "active"),
                        session("byys_expired", "expired")
                    ]
                }),
                "req_sessions",
            ),
        ],
    );
    let listed = result_json(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("sessions"),
    );
    assert_eq!(listed["data"]["yard"], "documentation");
    assert_eq!(listed["data"]["sessions"][0]["id"], "byys_active");
    assert_eq!(listed["data"]["sessions"][1]["status"], "expired");
    let request = &fixture.transport.requests()[1];
    assert_eq!(request.endpoint(), Endpoint::ListYardSessions);
    assert_eq!(request.query().expect("query"), "yardId=yard_documentation");
}

#[tokio::test]
async fn yard_sessions_revoke_targets_one_session_and_propagates_failures() {
    let fixture = access_fixture(
        &["yard-sessions", "revoke", "documentation", "byys_active"],
        vec![
            documentation_yard_page(),
            ok(serde_json::json!({}), "req_revoke"),
        ],
    );
    let revoked = result_json(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("revoke"),
    );
    assert_eq!(revoked["data"]["sessionId"], "byys_active");
    assert_eq!(revoked["data"]["revoked"], true);
    let request = &fixture.transport.requests()[1];
    assert_eq!(request.endpoint(), Endpoint::RevokeYardSession);
    assert_eq!(
        request.body().expect("body")["yardId"],
        "yard_documentation"
    );
    assert_eq!(request.body().expect("body")["sessionId"], "byys_active");

    let failed = access_fixture(
        &["yard-sessions", "revoke", "documentation", "byys_missing"],
        vec![
            documentation_yard_page(),
            api_failure(ErrorCode::NotFound, "req_revoke"),
        ],
    );
    assert_eq!(
        failed
            .runner
            .execute(&failed.command)
            .await
            .expect_err("missing session")
            .code(),
        ErrorCode::NotFound
    );
}

#[tokio::test]
async fn yard_sessions_revoke_rejects_invalid_identifiers_before_api_access() {
    let invalid = access_fixture(
        &["yard-sessions", "revoke", "documentation", ""],
        Vec::new(),
    );
    assert_eq!(
        invalid
            .runner
            .execute(&invalid.command)
            .await
            .expect_err("invalid session")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert!(invalid.transport.requests().is_empty());
}
