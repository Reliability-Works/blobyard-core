//! Empty-list presentation and scope-failure contracts for headless commands.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, ok};
use blobyard_api_client::Endpoint;
use blobyard_cli::{Diagnostics, GlobalArgs, OutputOptions, OutputRenderer};
use blobyard_core::ErrorCode;

fn human_stdout(result: blobyard_cli::CommandResult) -> String {
    OutputRenderer::new(
        OutputOptions::from_flags(&GlobalArgs {
            json: false,
            quiet: false,
            verbose: false,
            api_url: None,
            web_yard_origin: None,
            profile: None,
            workspace: None,
            project: None,
            retry_key: None,
        }),
        Diagnostics::default(),
    )
    .success(result)
    .stdout
}

#[tokio::test]
async fn empty_headless_lists_and_retention_overview_are_successes() {
    assert_empty_headless(
        &["blobyard", "workspaces", "list"],
        Endpoint::ListWorkspaces,
        serde_json::json!({ "items": [], "nextCursor": null }),
        "No workspaces found.\n",
    )
    .await;
    assert_empty_headless(
        &[
            "blobyard",
            "shares",
            "list",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        Endpoint::ListShares,
        serde_json::json!({ "items": [], "nextCursor": null }),
        "No shares found.\n",
    )
    .await;
    assert_empty_capability_and_retention_results().await;
}

async fn assert_empty_capability_and_retention_results() {
    assert_empty_headless(
        &[
            "blobyard",
            "previews",
            "list",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        Endpoint::ListPreviews,
        serde_json::json!({ "items": [], "nextCursor": null }),
        "No previews found.\n",
    )
    .await;
    assert_empty_headless(
        &[
            "blobyard",
            "retention",
            "overview",
            "--workspace",
            "team",
            "--project",
            "mobile",
        ],
        Endpoint::GetRetentionOverview,
        serde_json::json!({ "status": "idle" }),
        "{\n  \"status\": \"idle\"\n}\n",
    )
    .await;
}

async fn assert_empty_headless(
    args: &[&str],
    endpoint: Endpoint,
    data: serde_json::Value,
    expected_output: &str,
) {
    let fixture = Fixture::new(
        args,
        vec![ok(data, "req_empty_headless")],
        Some("token"),
        None,
    );
    let output = human_stdout(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("empty list or overview"),
    );
    assert_eq!(output, expected_output);
    assert_eq!(fixture.transport.requests()[0].endpoint(), endpoint);
}

#[tokio::test]
async fn workspace_rename_requires_a_selected_workspace() {
    let fixture = Fixture::new(
        &["blobyard", "workspaces", "rename", "Platform"],
        Vec::new(),
        Some("token"),
        None,
    );
    assert_eq!(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("missing workspace")
            .code(),
        ErrorCode::InvalidRequest
    );
    assert!(fixture.transport.requests().is_empty());
}
