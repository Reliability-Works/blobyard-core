//! Administration adapter contracts for bearer-authenticated CLI commands.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::dashboard_contract::contract_support::human_stdout;
use super::support::{Fixture, ok};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

#[tokio::test]
async fn administration_reads_use_their_scoped_versioned_endpoints() {
    for (args, endpoint, query) in [
        (
            vec!["blobyard", "members", "list", "--workspace", "team"],
            Endpoint::ListMembers,
            Some("workspace=team"),
        ),
        (
            vec!["blobyard", "invites", "list", "--workspace", "team"],
            Endpoint::ListInvites,
            Some("workspace=team"),
        ),
        (
            vec!["blobyard", "tokens", "list"],
            Endpoint::ListApiTokens,
            None,
        ),
        (
            vec!["blobyard", "trusts", "list", "--workspace", "team"],
            Endpoint::ListCiTrusts,
            Some("workspace=team"),
        ),
        (
            vec!["blobyard", "sessions", "list"],
            Endpoint::ListCliSessions,
            None,
        ),
    ] {
        let fixture = Fixture::new(
            &args,
            vec![ok(serde_json::json!({ "items": [] }), "req_admin_read")],
            Some("token"),
            None,
        );
        let output = human_stdout(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect("administration read"),
        );
        assert_eq!(output, "{\n  \"items\": []\n}\n");
        let requests = fixture.transport.requests();
        assert_eq!(requests[0].endpoint(), endpoint);
        assert_eq!(requests[0].query(), query);
    }
}

#[tokio::test]
async fn administration_mutations_preserve_exact_bodies_and_human_results() {
    assert_member_mutations().await;
    assert_invite_mutations().await;
    assert_token_and_session_mutations().await;
    assert_ci_trust_mutations().await;
}

async fn assert_member_mutations() {
    assert_administration_mutation(
        &[
            "blobyard",
            "members",
            "role",
            "user_1",
            "--role",
            "owner",
            "--workspace",
            "team",
        ],
        Endpoint::UpdateMemberRole,
        serde_json::json!({ "targetUserId": "user_1", "role": "owner", "workspace": "team" }),
        "Member role updated.\n",
    )
    .await;
    assert_administration_mutation(
        &[
            "blobyard",
            "members",
            "remove",
            "user_1",
            "--force",
            "--workspace",
            "team",
        ],
        Endpoint::RemoveMember,
        serde_json::json!({ "targetUserId": "user_1", "workspace": "team" }),
        "Member removed.\n",
    )
    .await;
}

async fn assert_invite_mutations() {
    assert_administration_mutation(
        &[
            "blobyard",
            "invites",
            "create",
            "developer@example.com",
            "--role",
            "admin",
            "--workspace",
            "team",
        ],
        Endpoint::CreateInvite,
        serde_json::json!({
            "email": "developer@example.com",
            "role": "admin",
            "workspace": "team"
        }),
        "Invitation created.\n",
    )
    .await;
    assert_administration_mutation(
        &[
            "blobyard",
            "invites",
            "revoke",
            "invite_1",
            "--workspace",
            "team",
        ],
        Endpoint::RevokeInvite,
        serde_json::json!({ "inviteId": "invite_1", "workspace": "team" }),
        "Invitation revoked.\n",
    )
    .await;
}

async fn assert_token_and_session_mutations() {
    assert_administration_mutation(
        &["blobyard", "tokens", "revoke", "token_1"],
        Endpoint::RevokeApiToken,
        serde_json::json!({ "tokenId": "token_1" }),
        "API token revoked.\n",
    )
    .await;
    assert_administration_mutation(
        &["blobyard", "sessions", "revoke", "session_1"],
        Endpoint::RevokeCliSession,
        serde_json::json!({ "sessionId": "session_1" }),
        "CLI session revoked.\n",
    )
    .await;
}

async fn assert_ci_trust_mutations() {
    assert_administration_mutation(
        &[
            "blobyard",
            "trusts",
            "create",
            "--repository",
            "acme/artifacts",
            "--workflow-path",
            ".github/workflows/ci.yml",
            "--workflow-ref",
            "refs/heads/main",
            "--allowed-ref-glob",
            "refs/tags/*",
            "--action",
            "upload",
            "--environment",
            "Production",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        Endpoint::CreateCiTrust,
        serde_json::json!({
            "allowedActions": ["upload"],
            "allowedRefGlob": "refs/tags/*",
            "environment": "Production",
            "project": "mobile",
            "repository": "acme/artifacts",
            "workflowPath": ".github/workflows/ci.yml",
            "workflowRef": "refs/heads/main",
            "workspace": "team"
        }),
        "GitHub OIDC trust created.\n",
    )
    .await;
    assert_administration_mutation(
        &[
            "blobyard",
            "trusts",
            "revoke",
            "trust_1",
            "--workspace",
            "team",
        ],
        Endpoint::RevokeCiTrust,
        serde_json::json!({ "trustId": "trust_1" }),
        "GitHub OIDC trust revoked.\n",
    )
    .await;
}

async fn assert_administration_mutation(
    args: &[&str],
    endpoint: Endpoint,
    body: serde_json::Value,
    expected_output: &str,
) {
    let fixture = Fixture::new(
        args,
        vec![ok(serde_json::json!({}), "req_admin_write")],
        Some("token"),
        None,
    );
    let output = human_stdout(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("administration mutation"),
    );
    assert_eq!(output, expected_output);
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), endpoint);
    assert_eq!(requests[0].body(), Some(&body));
    assert_eq!(requests[0].idempotency_key(), None);
}

#[tokio::test]
async fn cleanup_tokens_require_both_scope_parts() {
    for args in [
        vec![
            "blobyard",
            "tokens",
            "create",
            "Cleanup",
            "--expires-days",
            "7",
            "--scope",
            "object:write",
        ],
        vec![
            "blobyard",
            "tokens",
            "create",
            "Cleanup",
            "--expires-days",
            "7",
            "--scope",
            "object:write",
            "--workspace",
            "team",
        ],
    ] {
        let fixture = Fixture::new(&args, Vec::new(), Some("token"), None);
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("incomplete cleanup scope")
                .code(),
            ErrorCode::InvalidRequest
        );
        assert!(fixture.transport.requests().is_empty());
    }
}

#[tokio::test]
async fn created_tokens_require_raw_token_results() {
    let missing_token = Fixture::new(
        &[
            "blobyard",
            "tokens",
            "create",
            "CI",
            "--expires-days",
            "7",
            "--scope",
            "audit:read",
        ],
        vec![ok(
            serde_json::json!({ "id": "token_1" }),
            "req_token_missing",
        )],
        Some("token"),
        None,
    );
    assert_eq!(
        missing_token
            .runner
            .execute(&missing_token.command)
            .await
            .expect_err("missing one-time token")
            .code(),
        ErrorCode::InternalError
    );
}
