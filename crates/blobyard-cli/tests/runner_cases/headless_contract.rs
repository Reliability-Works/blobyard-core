//! Public dashboard operations exposed through direct CLI commands.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, ok, result_json};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

fn assert_cleanup_token_scope(body: &serde_json::Value) {
    assert_eq!(body["workspace"], "team");
    assert_eq!(body["project"], "mobile");
}

#[tokio::test]
async fn workspace_commands_list_create_and_validate_before_transport() {
    let list = Fixture::new(
        &["blobyard", "workspaces", "list"],
        vec![ok(
            serde_json::json!({
                "items": [{ "id": "workspace_1", "slug": "team", "name": "Team" }],
                "nextCursor": null
            }),
            "req_workspaces",
        )],
        Some("token"),
        None,
    );
    let result = list.runner.execute(&list.command).await.expect("list");
    assert_eq!(result_json(result)["data"]["items"][0]["slug"], "team");
    assert_eq!(
        list.transport.requests()[0].endpoint(),
        Endpoint::ListWorkspaces
    );

    let create = Fixture::new(
        &["blobyard", "workspaces", "create", "Product Team"],
        vec![ok(
            serde_json::json!({
                "id": "workspace_2", "slug": "product-team", "name": "Product Team"
            }),
            "req_workspace_create",
        )],
        Some("token"),
        None,
    );
    create
        .runner
        .execute(&create.command)
        .await
        .expect("create");
    let requests = create.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::CreateWorkspace);
    assert_eq!(requests[0].idempotency_key(), None);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({ "name": "Product Team" }))
    );

    let invalid = Fixture::new(
        &["blobyard", "workspaces", "create", "line\nbreak"],
        Vec::new(),
        Some("token"),
        None,
    );
    assert_eq!(
        invalid
            .runner
            .execute(&invalid.command)
            .await
            .expect_err("invalid name")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert!(invalid.transport.requests().is_empty());
}

#[tokio::test]
async fn share_and_preview_management_use_redacted_lists_and_stable_ids() {
    let fixture = Fixture::new(
        &[
            "blobyard",
            "shares",
            "list",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        vec![ok(
            serde_json::json!({
                "items": [{
                    "id": "share_1", "expiresAt": "2026-08-01T00:00:00Z", "status": "active",
                    "consumedCount": 0, "maximumDownloads": null
                }],
                "nextCursor": null
            }),
            "req_shares",
        )],
        Some("token"),
        None,
    );
    fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("shares");
    assert_eq!(
        fixture.transport.requests()[0].endpoint(),
        Endpoint::ListShares
    );

    for (args, endpoint, body) in [
        (
            vec!["blobyard", "shares", "revoke", "share_1"],
            Endpoint::RevokeShare,
            serde_json::json!({ "shareId": "share_1" }),
        ),
        (
            vec!["blobyard", "previews", "revoke", "preview_1"],
            Endpoint::RevokePreview,
            serde_json::json!({ "previewId": "preview_1" }),
        ),
    ] {
        let command = Fixture::new(
            &args,
            vec![ok(serde_json::json!({}), "req_revoke")],
            Some("token"),
            None,
        );
        command
            .runner
            .execute(&command.command)
            .await
            .expect("revoke");
        let requests = command.transport.requests();
        assert_eq!(requests[0].endpoint(), endpoint);
        assert_eq!(requests[0].body(), Some(&body));
    }
}

#[tokio::test]
async fn audit_command_preserves_workspace_scope_and_cursor() {
    let audit = Fixture::new(
        &[
            "blobyard",
            "audit",
            "list",
            "--cursor",
            "next",
            "--workspace",
            "team",
        ],
        vec![ok(
            serde_json::json!({ "items": [], "nextCursor": null }),
            "req_audit",
        )],
        Some("token"),
        None,
    );
    audit.runner.execute(&audit.command).await.expect("audit");
    let requests = audit.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::ListAudit);
    assert_eq!(requests[0].query(), Some("workspace=team&cursor=next"));
}

#[tokio::test]
async fn token_scoping_matches_the_server() {
    let token = Fixture::new(
        &[
            "blobyard",
            "tokens",
            "create",
            "CI cleanup",
            "--expires-days",
            "7",
            "--scope",
            "audit:read",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        vec![ok(
            serde_json::json!({ "id": "token_1", "rawToken": "by_live_once" }),
            "req_token",
        )],
        Some("token"),
        None,
    );
    let output = result_json(token.runner.execute(&token.command).await.expect("token"));
    assert_eq!(output["data"]["rawToken"], "by_live_once");
    let requests = token.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::CreateApiToken);
    let body = requests[0].body().expect("body");
    assert_eq!(body["workspace"], serde_json::Value::Null);
    assert_eq!(body["project"], serde_json::Value::Null);
    assert_eq!(requests[0].idempotency_key(), None);

    let cleanup = Fixture::new(
        &[
            "blobyard",
            "tokens",
            "create",
            "CI cleanup",
            "--expires-days",
            "7",
            "--scope",
            "object:write",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        vec![ok(
            serde_json::json!({ "id": "token_2", "rawToken": "by_cleanup_once" }),
            "req_cleanup_token",
        )],
        Some("token"),
        None,
    );
    cleanup
        .runner
        .execute(&cleanup.command)
        .await
        .expect("cleanup token");
    let cleanup_requests = cleanup.transport.requests();
    let cleanup_body = cleanup_requests[0].body().expect("cleanup body");
    assert_cleanup_token_scope(cleanup_body);
}

#[tokio::test]
async fn member_removal_requires_force() {
    let unconfirmed = Fixture::new(
        &[
            "blobyard",
            "members",
            "remove",
            "user_1",
            "--workspace",
            "team",
        ],
        Vec::new(),
        Some("token"),
        None,
    );
    assert_eq!(
        unconfirmed
            .runner
            .execute(&unconfirmed.command)
            .await
            .expect_err("confirmation")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert!(unconfirmed.transport.requests().is_empty());
}
