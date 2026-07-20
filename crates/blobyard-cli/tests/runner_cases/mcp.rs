//! MCP tool adapter coverage over the existing authorized runner.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::support::{Fixture, api_failure, ok};
use blobyard_core::ErrorCode;
use blobyard_mcp::{BackendError, Scope, ToolBackend, ToolCall};

fn invalid_scope() -> Scope {
    Scope {
        workspace: Some("bad slug".to_owned()),
        project: None,
    }
}

fn calls() -> Vec<ToolCall> {
    let mut calls = resource_calls();
    calls.extend(capability_calls());
    calls
}

fn resource_calls() -> Vec<ToolCall> {
    vec![
        ToolCall::Whoami {
            scope: invalid_scope(),
        },
        ToolCall::ListProjects {
            scope: invalid_scope(),
        },
        ToolCall::ListObjects {
            scope: invalid_scope(),
            prefix: None,
            versions: false,
        },
        ToolCall::GetRetention {
            scope: invalid_scope(),
        },
        ToolCall::ListInboxes {
            scope: invalid_scope(),
        },
        ToolCall::ListShares {
            scope: invalid_scope(),
        },
        ToolCall::ListPreviews {
            scope: invalid_scope(),
        },
        ToolCall::CreateProject {
            scope: invalid_scope(),
            name: "Project".to_owned(),
        },
        ToolCall::UploadFile {
            scope: invalid_scope(),
            source: "artifact.bin".to_owned(),
            path: None,
            include_ignored: false,
        },
        ToolCall::DownloadFile {
            scope: invalid_scope(),
            uri: "blobyard://team/project/artifact.bin".to_owned(),
            output: "artifact.bin".to_owned(),
            force: false,
        },
        ToolCall::DeleteObject {
            scope: invalid_scope(),
            uri: "blobyard://team/project/artifact.bin".to_owned(),
        },
    ]
}

fn capability_calls() -> Vec<ToolCall> {
    vec![
        ToolCall::CreateShare {
            scope: invalid_scope(),
            target: "blobyard://team/project/artifact.bin".to_owned(),
            expires: None,
            notify: None,
        },
        ToolCall::RevokeShare {
            scope: invalid_scope(),
            share_id: "share_1".to_owned(),
        },
        ToolCall::CreatePreview {
            scope: invalid_scope(),
            directory: "./site".to_owned(),
            expires: None,
        },
        ToolCall::RevokePreview {
            scope: invalid_scope(),
            preview_id: "preview_1".to_owned(),
        },
        ToolCall::CreateInbox {
            scope: invalid_scope(),
            name: "Uploads".to_owned(),
            expires: None,
        },
        ToolCall::RevokeInbox {
            scope: invalid_scope(),
            inbox_id: "inbox_1".to_owned(),
        },
        ToolCall::SetRetention {
            scope: invalid_scope(),
            latest: 1,
            branch: None,
            path: None,
        },
        ToolCall::ClearRetention {
            scope: invalid_scope(),
        },
    ]
}

#[tokio::test]
async fn every_mcp_tool_rejects_an_invalid_scope_before_network_access() {
    let fixture = Fixture::new(&["blobyard", "whoami"], Vec::new(), Some("token"), None);
    for call in calls() {
        let error = fixture.runner.call(call).await.expect_err("invalid scope");
        assert_eq!(
            error,
            BackendError::new("INVALID_REQUEST", "The workspace slug is not valid.")
        );
    }
    assert!(fixture.transport.requests().is_empty());
}

#[tokio::test]
async fn mcp_tool_returns_the_existing_structured_command_result() {
    let fixture = Fixture::new(
        &["blobyard", "whoami"],
        vec![ok(
            serde_json::json!({
                "principalId": "user_1",
                "principalType": "cli",
                "displayName": "Developer",
                "email": "developer@example.com",
                "defaultWorkspace": { "id": "workspace_1", "name": "Personal", "slug": "team" },
                "scopes": ["object:read"]
            }),
            "req_identity",
        )],
        Some("token"),
        None,
    );
    let result = fixture
        .runner
        .call(ToolCall::Whoami {
            scope: Scope::default(),
        })
        .await
        .expect("whoami");
    assert_eq!(result["principalId"], "user_1");
    assert_eq!(result["defaultWorkspace"]["slug"], "team");
}

#[tokio::test]
async fn mcp_can_revoke_a_share_without_receiving_its_capability_token() {
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "--project",
            "mobile",
            "whoami",
        ],
        vec![ok(serde_json::json!({}), "req_revoke")],
        Some("token"),
        None,
    );
    let result = fixture
        .runner
        .call(ToolCall::RevokeShare {
            scope: Scope::default(),
            share_id: "share_1".to_owned(),
        })
        .await
        .expect("revoke share");
    assert_eq!(result, serde_json::json!({}));
    let requests = fixture.transport.requests();
    assert_eq!(
        requests[0].endpoint(),
        blobyard_api_client::Endpoint::RevokeShare
    );
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({ "shareId": "share_1" }))
    );
}

#[tokio::test]
async fn mcp_share_revocation_propagates_safe_api_failures() {
    let fixture = Fixture::new(
        &["blobyard", "whoami"],
        vec![api_failure(ErrorCode::Forbidden, "req_forbidden")],
        Some("token"),
        None,
    );
    let error = fixture
        .runner
        .call(ToolCall::RevokeShare {
            scope: Scope::default(),
            share_id: "share_1".to_owned(),
        })
        .await
        .expect_err("forbidden revoke");
    assert_eq!(
        error,
        BackendError::new("FORBIDDEN", "You don't have access to do that.")
    );
}
