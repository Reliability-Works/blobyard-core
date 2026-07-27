#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::graph;
use crate::TokenStore;
use crate::runner::login::tests::support::{Fixture, fixture_tokens, fixture_yards, ok};
use blobyard_api_client::{ApiRequest, Endpoint, RawResponse};
use blobyard_core::SecretString;
use serde_json::{Value, json};

#[path = "yard_identity_command_failure_tests.rs"]
mod failure_tests;

async fn execute_identity_command(
    arguments: &[&str],
    response: RawResponse,
    endpoint: Endpoint,
) -> (Value, ApiRequest) {
    let fixture = Fixture::new(
        arguments,
        vec![
            fixture_tokens(),
            fixture_yards(),
            fixture_tokens(),
            response,
        ],
    );
    fixture
        .store
        .save(&SecretString::new("local-api-token").expect("token"))
        .expect("store token");
    let result = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect("identity command");
    let requests = fixture.transport.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[1].endpoint(), Endpoint::ListWebYards);
    assert_eq!(requests[3].endpoint(), endpoint);
    (result.into_data(), requests[3].clone())
}

#[tokio::test]
async fn management_role_list_executes_the_typed_api_contract() {
    let (listed, request) = execute_identity_command(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "management-roles",
            "list",
            "documentation",
            "--cursor",
            "next",
        ],
        ok(
            &json!({
                "items": [{
                    "userId": "user_developer",
                    "role": "developer",
                    "createdAt": "2026-07-24T09:00:00Z",
                    "updatedAt": "2026-07-24T09:00:00Z"
                }],
                "nextCursor": "after"
            }),
            "request_roles",
        ),
        Endpoint::ListYardManagementRoles,
    )
    .await;
    assert_eq!(listed["yard"], "documentation");
    assert_eq!(listed["items"][0]["userId"], "user_developer");
    assert_eq!(
        request.query(),
        Some("yardId=yard_documentation&cursor=next")
    );
}

#[tokio::test]
async fn management_role_mutations_execute_the_typed_api_contracts() {
    let (set, request) = execute_identity_command(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "management-roles",
            "set",
            "documentation",
            "user_developer",
            "developer",
        ],
        ok(
            &json!({
                "userId": "user_developer",
                "role": "developer",
                "createdAt": "2026-07-24T09:00:00Z",
                "updatedAt": "2026-07-24T09:00:00Z"
            }),
            "request_set_role",
        ),
        Endpoint::SetYardManagementRole,
    )
    .await;
    assert_eq!(set["assignment"]["role"], "developer");
    assert_eq!(
        request.body(),
        Some(&json!({
            "yardId": "yard_documentation",
            "userId": "user_developer",
            "role": "developer"
        }))
    );

    let (revoked, request) = execute_identity_command(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "management-roles",
            "revoke",
            "documentation",
            "user_developer",
        ],
        ok(&json!({}), "request_revoke_role"),
        Endpoint::RevokeYardManagementRole,
    )
    .await;
    assert_eq!(revoked["revoked"], true);
    assert_eq!(revoked["assignment"], Value::Null);
    assert_eq!(
        request.body(),
        Some(&json!({
            "yardId": "yard_documentation",
            "userId": "user_developer"
        }))
    );
}

#[tokio::test]
async fn application_policy_get_executes_the_typed_api_contract() {
    let (current, request) = execute_identity_command(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "application-policy",
            "get",
            "documentation",
        ],
        ok(&json!({ "policy": null }), "request_get_policy"),
        Endpoint::GetYardApplicationPolicy,
    )
    .await;
    assert_eq!(current["yard"], "documentation");
    assert_eq!(current["response"]["policy"], Value::Null);
    assert_eq!(request.query(), Some("yardId=yard_documentation"));
}

#[tokio::test]
async fn application_policy_set_executes_the_typed_api_contract() {
    let encoded = serde_json::to_string(&graph()).expect("policy");
    let digest = "a".repeat(64);
    let (updated, request) = execute_identity_command(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "application-policy",
            "set",
            "documentation",
            "--policy-json",
            &encoded,
            "--source-manifest-digest",
            &digest,
        ],
        ok(
            &json!({
                "policy": {
                    "revision": 1,
                    "sourceManifestDigest": digest,
                    "defaultRole": graph().default_role,
                    "roles": graph().roles,
                    "approvedAt": "2026-07-24T09:00:00Z",
                    "approvedByPrincipalId": "operator"
                }
            }),
            "request_set_policy",
        ),
        Endpoint::SetYardApplicationPolicy,
    )
    .await;
    assert_eq!(updated["response"]["policy"]["revision"], 1);
    assert_eq!(
        request.body().and_then(|body| body["yardId"].as_str()),
        Some("yard_documentation")
    );
    assert_eq!(
        request
            .body()
            .and_then(|body| body["sourceManifestDigest"].as_str()),
        Some(digest.as_str())
    );
    assert_eq!(
        request.body().and_then(|body| body["defaultRole"].as_str()),
        Some("viewer")
    );
}
