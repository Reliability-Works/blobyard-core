//! Web Yard environment listing workflows.

use super::super::support::{Fixture, api_failure, ok, result_json};
use super::yard;
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

fn empty_yard_page() -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({ "items": [], "nextCursor": null }),
        "req_yards",
    )
}

fn documentation_yard_page() -> blobyard_api_client::RawResponse {
    ok(
        serde_json::json!({ "items": [yard("documentation", None)], "nextCursor": null }),
        "req_yards",
    )
}

#[tokio::test]
async fn env_list_resolves_the_name_then_lists_active_environments() {
    let environments = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "web",
            "env",
            "list",
            "documentation",
        ],
        vec![
            ok(
                serde_json::json!({ "items": [yard("documentation", Some("deploy_1"))], "nextCursor": null }),
                "req_yard",
            ),
            ok(
                serde_json::json!({
                    "environments": [{
                        "createdAt": "1970-01-01T00:00:00.001Z",
                        "id": "yardenv_yard_documentation",
                        "kind": "production",
                        "name": "production",
                        "updatedAt": "1970-01-01T00:00:00.001Z"
                    }]
                }),
                "req_environments",
            ),
        ],
        Some("ci-token"),
        None,
    );
    let listed = result_json(
        environments
            .runner
            .execute(&environments.command)
            .await
            .expect("environments"),
    );
    assert_eq!(listed["data"]["yard"], "documentation");
    assert_eq!(
        listed["data"]["environments"][0]["id"],
        "yardenv_yard_documentation"
    );
    assert_eq!(listed["data"]["environments"][0]["kind"], "production");
    assert_eq!(
        environments.transport.requests()[1].endpoint(),
        Endpoint::ListYardEnvironments
    );
    assert_eq!(
        environments.transport.requests()[1].query().expect("query"),
        "yardId=yard_documentation"
    );
}

#[tokio::test]
async fn env_list_propagates_scope_selection_and_api_failures() {
    let missing_scope = Fixture::new(
        &["blobyard", "env", "list"],
        Vec::new(),
        Some("ci-token"),
        None,
    );
    assert!(
        missing_scope
            .runner
            .execute(&missing_scope.command)
            .await
            .is_err()
    );

    let scoped = [
        "blobyard",
        "--workspace",
        "team",
        "--project",
        "web",
        "env",
        "list",
        "documentation",
    ];
    let missing_yard = Fixture::new(&scoped, vec![empty_yard_page()], Some("ci-token"), None);
    assert_eq!(
        missing_yard
            .runner
            .execute(&missing_yard.command)
            .await
            .expect_err("missing Yard")
            .code(),
        ErrorCode::NotFound
    );

    let remote = Fixture::new(
        &scoped,
        vec![
            documentation_yard_page(),
            api_failure(ErrorCode::ProviderUnavailable, "req_environments"),
        ],
        Some("ci-token"),
        None,
    );
    assert!(remote.runner.execute(&remote.command).await.is_err());
}
