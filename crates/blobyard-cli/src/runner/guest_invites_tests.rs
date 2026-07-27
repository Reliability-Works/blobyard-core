#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{invitation_line, invitation_lines, status_label};
use crate::TokenStore;
use crate::runner::login::tests::support::{
    Fixture, api_failure, fixture_tokens, fixture_yards, ok,
};
use blobyard_api_client::{Endpoint, YardGuestInvite, YardGuestInviteStatus};
use blobyard_core::{ErrorCode, SecretString};
use serde_json::json;

fn invitation(status: YardGuestInviteStatus) -> YardGuestInvite {
    YardGuestInvite {
        accepted_at: None,
        app_roles: vec!["viewer".to_owned()],
        created_at: "2026-07-27T09:00:00Z".to_owned(),
        email: "guest@example.com".to_owned(),
        environment_id: None,
        expires_at: "2026-08-03T09:00:00Z".to_owned(),
        id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        revoked_at: None,
        status,
        yard_id: "yard_documentation".to_owned(),
    }
}

#[test]
fn invitation_presentation_is_non_secret_and_covers_lifecycle() {
    assert_eq!(
        invitation_lines(&[], None),
        "No Yard guest invitations found."
    );
    for status in [
        YardGuestInviteStatus::Pending,
        YardGuestInviteStatus::Accepted,
        YardGuestInviteStatus::Revoked,
    ] {
        assert!(invitation_line(&invitation(status)).contains(status_label(status)));
    }
    let lines = invitation_lines(
        &[invitation(YardGuestInviteStatus::Pending)],
        Some("cursor"),
    );
    assert!(lines.contains("guest@example.com"));
    assert!(lines.contains("Next cursor: cursor"));
    assert!(!lines.contains("bygi_"));
    assert!(!lines.contains("byg_"));
    let mut no_roles = invitation(YardGuestInviteStatus::Pending);
    no_roles.app_roles.clear();
    assert!(invitation_line(&no_roles).contains("roles none"));
}

#[tokio::test]
async fn guest_invite_commands_execute_the_typed_api_contracts() {
    assert_list().await;
    assert_create().await;
    assert_revoke().await;
}

#[tokio::test]
async fn guest_invite_commands_propagate_selection_input_and_api_failures() {
    assert_initial_list_failure().await;
    assert_missing_yard_failures().await;
    assert_guest_invite_api_failures().await;
    assert_failure(
        &["guest-invites", "revoke", "documentation", "bad\nid"],
        Vec::new(),
        ErrorCode::InvalidRequest,
    )
    .await;
}

async fn assert_initial_list_failure() {
    assert_failure(
        &["guest-invites", "list"],
        vec![
            fixture_tokens(),
            api_failure(ErrorCode::Conflict, 409, "request_yards_failed"),
        ],
        ErrorCode::Conflict,
    )
    .await;
}

async fn assert_missing_yard_failures() {
    for tail in [
        vec!["guest-invites", "list", "missing-yard"],
        vec![
            "guest-invites",
            "create",
            "missing-yard",
            "guest@example.com",
            "--expires",
            "2026-08-03T09:00:00Z",
        ],
        vec![
            "guest-invites",
            "revoke",
            "missing-yard",
            "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ],
    ] {
        assert_failure(
            &tail,
            vec![fixture_tokens(), fixture_yards()],
            ErrorCode::NotFound,
        )
        .await;
    }
}

async fn assert_guest_invite_api_failures() {
    for tail in [
        vec!["guest-invites", "list", "documentation"],
        vec![
            "guest-invites",
            "create",
            "documentation",
            "guest@example.com",
            "--expires",
            "2026-08-03T09:00:00Z",
        ],
        vec![
            "guest-invites",
            "revoke",
            "documentation",
            "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ],
    ] {
        assert_failure(
            &tail,
            vec![
                fixture_tokens(),
                fixture_yards(),
                fixture_tokens(),
                api_failure(ErrorCode::Conflict, 409, "request_guest_invite_failed"),
            ],
            ErrorCode::Conflict,
        )
        .await;
    }
}

async fn assert_list() {
    let fixture = fixture(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "guest-invites",
            "list",
            "documentation",
            "--cursor",
            "cursor",
        ],
        ok(
            &json!({
                "items": [invitation_json("pending")],
                "nextCursor": null,
            }),
            "request_list",
        ),
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("list");
    assert_eq!(result.into_data()["items"][0]["email"], "guest@example.com");
    let requests = fixture.transport.requests();
    assert_eq!(requests[3].endpoint(), Endpoint::ListYardGuestInvites);
    assert_eq!(
        requests[3].query(),
        Some("cursor=cursor&limit=50&yardId=yard_documentation")
    );
}

async fn assert_create() {
    let invitation_url = format!(
        "https://account.example/account/yard-invite?token=bygi_{}&continuation=opaque",
        "a".repeat(64)
    );
    let fixture = fixture(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "guest-invites",
            "create",
            "documentation",
            "guest@example.com",
            "--role",
            "viewer",
            "--expires",
            "2026-08-03T09:00:00Z",
        ],
        ok(
            &json!({
                "invitation": invitation_json("pending"),
                "invitationUrl": invitation_url,
            }),
            "request_create",
        ),
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("create");
    assert_eq!(result.into_data()["invitationUrl"], invitation_url);
    let requests = fixture.transport.requests();
    assert_eq!(requests[3].endpoint(), Endpoint::CreateYardGuestInvite);
    assert_eq!(
        requests[3].body().expect("body")["email"],
        "guest@example.com"
    );
}

async fn assert_revoke() {
    let fixture = fixture(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "guest-invites",
            "revoke",
            "documentation",
            "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ],
        ok(
            &json!({ "invitation": invitation_json("revoked") }),
            "request_revoke",
        ),
    );
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("revoke");
    assert_eq!(result.into_data()["invitation"]["status"], "revoked");
    let requests = fixture.transport.requests();
    assert_eq!(requests[3].endpoint(), Endpoint::RevokeYardGuestInvite);
    assert_eq!(
        requests[3].body().expect("body")["invitationId"],
        "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}

fn fixture(arguments: &[&str], operation: blobyard_api_client::RawResponse) -> Fixture {
    let fixture = Fixture::new(
        arguments,
        vec![
            fixture_tokens(),
            fixture_yards(),
            fixture_tokens(),
            operation,
        ],
    );
    fixture
        .store
        .save(&SecretString::new("local-api-token").expect("token"))
        .expect("store token");
    fixture
}

async fn assert_failure(
    tail: &[&str],
    responses: Vec<blobyard_api_client::RawResponse>,
    code: ErrorCode,
) {
    let mut arguments = vec!["blobyard", "--workspace", "main", "--project", "artifacts"];
    arguments.extend(tail);
    let fixture = Fixture::new(&arguments, responses);
    fixture
        .store
        .save(&SecretString::new("local-api-token").expect("token"))
        .expect("store token");
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("guest invitation failure")
            .code(),
        code
    );
}

fn invitation_json(status: &str) -> serde_json::Value {
    json!({
        "acceptedAt": null,
        "appRoles": ["viewer"],
        "createdAt": "2026-07-27T09:00:00Z",
        "email": "guest@example.com",
        "environmentId": null,
        "expiresAt": "2026-08-03T09:00:00Z",
        "id": "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "revokedAt": (status == "revoked").then_some("2026-07-27T10:00:00Z"),
        "status": status,
        "yardId": "yard_documentation",
    })
}
