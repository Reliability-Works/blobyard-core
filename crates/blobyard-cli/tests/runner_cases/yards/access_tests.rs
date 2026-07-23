//! Web Yard access-policy workflows.

use super::super::support::{Fixture, api_failure, ok, result_json};
use super::yard;
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

pub(super) fn documentation_yard_page() -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({ "items": [yard("documentation", Some("deploy_1"))], "nextCursor": null }),
        "req_yards",
    )
}

pub(super) fn grant(id: &str) -> serde_json::Value {
    serde_json::json!({
        "appRoles": ["viewer"],
        "createdAt": "1970-01-01T00:00:00.001Z",
        "environmentId": null,
        "expiresAt": null,
        "id": id,
        "principalId": "user_reader",
        "principalKind": "user"
    })
}

fn scoped(arguments: &[&str]) -> Vec<String> {
    ["blobyard", "--workspace", "team", "--project", "web"]
        .iter()
        .copied()
        .chain(arguments.iter().copied())
        .map(str::to_owned)
        .collect()
}

pub(super) fn access_fixture(
    arguments: &[&str],
    responses: Vec<blobyard_api_client::RawResponse>,
) -> Fixture {
    let owned = scoped(arguments);
    let borrowed = owned.iter().map(String::as_str).collect::<Vec<_>>();
    Fixture::new(&borrowed, responses, Some("ci-token"), None)
}

#[tokio::test]
async fn access_list_reads_visibility_and_active_grants() {
    let fixture = access_fixture(
        &["access", "list", "documentation"],
        vec![
            documentation_yard_page(),
            ok(
                serde_json::json!({ "grants": [grant("yardgrant_1")], "visibility": "owner" }),
                "req_access",
            ),
        ],
    );
    let listed = result_json(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("access"),
    );
    assert_eq!(listed["data"]["yard"], "documentation");
    assert_eq!(listed["data"]["visibility"], "owner");
    assert_eq!(listed["data"]["grants"][0]["id"], "yardgrant_1");
    assert_eq!(
        fixture.transport.requests()[1].endpoint(),
        Endpoint::GetYardAccess
    );
    assert_eq!(
        fixture.transport.requests()[1].query().expect("query"),
        "yardId=yard_documentation"
    );
}

#[tokio::test]
async fn access_set_visibility_validates_and_updates_the_policy() {
    let invalid = access_fixture(
        &["access", "set-visibility", "documentation", "hidden"],
        Vec::new(),
    );
    assert_eq!(
        invalid
            .runner
            .execute(&invalid.command)
            .await
            .expect_err("invalid visibility")
            .code(),
        ErrorCode::InvalidRequest
    );
    let fixture = access_fixture(
        &["access", "set-visibility", "documentation", "workspace"],
        vec![
            documentation_yard_page(),
            ok(serde_json::json!({ "visibility": "workspace" }), "req_set"),
        ],
    );
    let updated = result_json(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("visibility"),
    );
    assert_eq!(updated["data"]["visibility"], "workspace");
    let request = &fixture.transport.requests()[1];
    assert_eq!(request.endpoint(), Endpoint::SetYardVisibility);
    assert_eq!(request.body().expect("body")["visibility"], "workspace");
}

#[tokio::test]
async fn access_grant_submits_bounded_principals_roles_and_expiries() {
    let invalid_kind = access_fixture(
        &[
            "access",
            "grant",
            "documentation",
            "--principal-kind",
            "robot",
            "--principal-id",
            "user_reader",
        ],
        Vec::new(),
    );
    assert_eq!(
        invalid_kind
            .runner
            .execute(&invalid_kind.command)
            .await
            .expect_err("invalid principal kind")
            .code(),
        ErrorCode::InvalidRequest
    );
    let fixture = access_fixture(
        &[
            "access",
            "grant",
            "documentation",
            "--principal-kind",
            "user",
            "--principal-id",
            "user_reader",
            "--role",
            "viewer",
            "--environment",
            "yardenv_yard_documentation",
            "--expires",
            "2100-01-01T00:00:00Z",
        ],
        vec![
            documentation_yard_page(),
            ok(
                serde_json::json!({ "grant": grant("yardgrant_1") }),
                "req_grant",
            ),
        ],
    );
    let granted = result_json(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("grant"),
    );
    assert_eq!(granted["data"]["grant"]["id"], "yardgrant_1");
    let request = &fixture.transport.requests()[1];
    assert_eq!(request.endpoint(), Endpoint::GrantYardAccess);
    let body = request.body().expect("body");
    assert_eq!(body["principalKind"], "user");
    assert_eq!(body["appRoles"], serde_json::json!(["viewer"]));
    assert_eq!(body["environmentId"], "yardenv_yard_documentation");
    assert_eq!(body["expiresAt"], "2100-01-01T00:00:00Z");
}

#[tokio::test]
async fn access_revoke_targets_one_grant_and_propagates_failures() {
    let fixture = access_fixture(
        &["access", "revoke", "documentation", "yardgrant_1"],
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
    assert_eq!(revoked["data"]["grantId"], "yardgrant_1");
    assert_eq!(revoked["data"]["revoked"], true);
    let request = &fixture.transport.requests()[1];
    assert_eq!(request.endpoint(), Endpoint::RevokeYardAccess);
    assert_eq!(request.body().expect("body")["grantId"], "yardgrant_1");

    let failed = access_fixture(
        &["access", "revoke", "documentation", "yardgrant_missing"],
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
            .expect_err("missing grant")
            .code(),
        ErrorCode::NotFound
    );
}
