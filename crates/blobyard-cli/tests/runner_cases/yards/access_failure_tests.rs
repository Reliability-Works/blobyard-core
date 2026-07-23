//! Web Yard access-policy failure workflows.

use super::super::support::{Fixture, api_failure, ok, result_json};
use super::access_tests::{access_fixture, documentation_yard_page, grant};
use blobyard_core::ErrorCode;

fn empty_yard_page() -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({ "items": [], "nextCursor": null }),
        "req_yards",
    )
}

#[tokio::test]
async fn access_commands_propagate_scope_selection_and_api_failures() {
    let commands: [&[&str]; 4] = [
        &["access", "list", "documentation"],
        &["access", "set-visibility", "documentation", "owner"],
        &[
            "access",
            "grant",
            "documentation",
            "--principal-kind",
            "user",
            "--principal-id",
            "user_reader",
        ],
        &["access", "revoke", "documentation", "yardgrant_1"],
    ];
    for arguments in commands {
        let unscoped_arguments: Vec<&str> = std::iter::once("blobyard")
            .chain(arguments.iter().copied())
            .collect();
        let unscoped = Fixture::new(&unscoped_arguments, Vec::new(), Some("ci-token"), None);
        assert!(unscoped.runner.execute(&unscoped.command).await.is_err());
        let missing = access_fixture(arguments, vec![empty_yard_page()]);
        assert_eq!(
            missing
                .runner
                .execute(&missing.command)
                .await
                .expect_err("missing Yard")
                .code(),
            ErrorCode::NotFound
        );
        let remote = access_fixture(
            arguments,
            vec![
                documentation_yard_page(),
                api_failure(ErrorCode::ProviderUnavailable, "req_access"),
            ],
        );
        assert!(remote.runner.execute(&remote.command).await.is_err());
    }
}

#[tokio::test]
async fn access_mutations_reject_invalid_yard_names_before_network_access() {
    let commands: [&[&str]; 3] = [
        &["access", "set-visibility", "Bad Name", "owner"],
        &[
            "access",
            "grant",
            "Bad Name",
            "--principal-kind",
            "user",
            "--principal-id",
            "user_reader",
        ],
        &["access", "revoke", "Bad Name", "yardgrant_1"],
    ];
    for arguments in commands {
        let fixture = access_fixture(arguments, Vec::new());
        assert_eq!(
            fixture
                .runner
                .execute(&fixture.command)
                .await
                .expect_err("invalid Yard name")
                .code(),
            ErrorCode::InvalidRequest
        );
    }
}

#[tokio::test]
async fn access_grant_omits_optional_restrictions_by_default() {
    let fixture = access_fixture(
        &[
            "access",
            "grant",
            "documentation",
            "--principal-kind",
            "group",
            "--principal-id",
            "group_docs",
        ],
        vec![
            documentation_yard_page(),
            ok(
                serde_json::json!({ "grant": grant("yardgrant_2") }),
                "req_grant",
            ),
        ],
    );
    let granted = result_json(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("minimal grant"),
    );
    assert_eq!(granted["data"]["grant"]["id"], "yardgrant_2");
    let requests = fixture.transport.requests();
    let body = requests[1].body().expect("body");
    assert!(body.get("environmentId").is_none());
    assert!(body.get("expiresAt").is_none());
    assert_eq!(body["appRoles"], serde_json::json!([]));
}
