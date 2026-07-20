#![allow(
    clippy::expect_used,
    reason = "tool parsing tests use fixed JSON fixtures"
)]

use crate::{Scope, ToolCall, WebYardToolCall};
use serde_json::json;

#[path = "dashboard_call_tests.rs"]
mod dashboard_tests;
#[path = "tool_call_error_tests.rs"]
mod error_tests;

fn parse(name: &str, arguments: impl Into<serde_json::Value>) -> ToolCall {
    let arguments = arguments.into();
    ToolCall::parse(name, &arguments).expect("valid tool fixture must parse")
}

#[test]
fn parses_read_only_calls_and_scope() {
    let scope = Scope {
        workspace: Some("team".to_owned()),
        project: Some("mobile".to_owned()),
    };
    assert_eq!(
        parse(
            "blobyard_whoami",
            json!({ "workspace": "team", "project": "mobile" })
        ),
        ToolCall::Whoami { scope }
    );
    assert_eq!(
        parse("blobyard_list_projects", json!({})),
        ToolCall::ListProjects {
            scope: Scope::default()
        }
    );
    assert_eq!(
        parse("blobyard_list_workspaces", json!({})),
        ToolCall::ListWorkspaces {
            scope: Scope::default()
        }
    );
    assert_eq!(
        parse(
            "blobyard_list_objects",
            json!({ "prefix": "blobyard://team/mobile", "versions": true })
        ),
        ToolCall::ListObjects {
            scope: Scope::default(),
            prefix: Some("blobyard://team/mobile".to_owned()),
            versions: true
        }
    );
    assert_eq!(
        parse("blobyard_get_retention", json!({ "project": "mobile" })),
        ToolCall::GetRetention {
            scope: Scope {
                workspace: None,
                project: Some("mobile".to_owned())
            }
        }
    );
    assert_eq!(
        parse("blobyard_list_inboxes", json!({})),
        ToolCall::ListInboxes {
            scope: Scope::default()
        }
    );
}

#[test]
fn parses_capability_discovery_calls() {
    assert_eq!(
        parse("blobyard_list_shares", json!({ "workspace": "team" })),
        ToolCall::ListShares {
            scope: Scope {
                workspace: Some("team".to_owned()),
                project: None
            }
        }
    );
    assert_eq!(
        parse("blobyard_list_previews", json!({ "project": "mobile" })),
        ToolCall::ListPreviews {
            scope: Scope {
                workspace: None,
                project: Some("mobile".to_owned())
            }
        }
    );
}

#[test]
fn parses_mutating_calls() {
    let empty = Scope::default();
    assert_eq!(
        parse("blobyard_create_workspace", json!({ "name": "Platform" })),
        ToolCall::CreateWorkspace {
            scope: empty.clone(),
            name: "Platform".to_owned()
        }
    );
    assert_eq!(
        parse("blobyard_create_project", json!({ "name": "Mobile" })),
        ToolCall::CreateProject {
            scope: empty,
            name: "Mobile".to_owned()
        }
    );
}

#[test]
fn parses_agent_lifecycle_calls() {
    assert_eq!(
        parse(
            "blobyard_delete_object",
            json!({ "uri": "blobyard://team/mobile/build.zip" })
        ),
        ToolCall::DeleteObject {
            scope: Scope::default(),
            uri: "blobyard://team/mobile/build.zip".to_owned()
        }
    );
    assert_eq!(
        parse("blobyard_revoke_share", json!({ "share_id": "share_1" })),
        ToolCall::RevokeShare {
            scope: Scope::default(),
            share_id: "share_1".to_owned()
        }
    );
    assert_eq!(
        parse(
            "blobyard_create_preview",
            json!({ "directory": "./site", "expires": "7d" })
        ),
        ToolCall::CreatePreview {
            scope: Scope::default(),
            directory: "./site".to_owned(),
            expires: Some("7d".to_owned())
        }
    );
    assert_eq!(
        parse(
            "blobyard_revoke_preview",
            json!({ "preview_id": "preview_1" })
        ),
        ToolCall::RevokePreview {
            scope: Scope::default(),
            preview_id: "preview_1".to_owned()
        }
    );
}

#[test]
fn parses_transfer_and_sharing_calls() {
    let empty = Scope::default();
    assert_eq!(
        parse(
            "blobyard_upload_file",
            json!({ "source": "build.zip", "path": "release/build.zip", "include_ignored": true })
        ),
        ToolCall::UploadFile {
            scope: empty.clone(),
            source: "build.zip".to_owned(),
            path: Some("release/build.zip".to_owned()),
            include_ignored: true
        }
    );
    assert_eq!(
        parse(
            "blobyard_download_file",
            json!({ "uri": "blobyard://team/mobile/build.zip", "output": "./build.zip", "force": true })
        ),
        ToolCall::DownloadFile {
            scope: empty.clone(),
            uri: "blobyard://team/mobile/build.zip".to_owned(),
            output: "./build.zip".to_owned(),
            force: true
        }
    );
    assert_eq!(
        parse(
            "blobyard_create_share",
            json!({ "target": "build.zip", "expires": "7d", "notify": "dev@example.com" })
        ),
        ToolCall::CreateShare {
            scope: empty,
            target: "build.zip".to_owned(),
            expires: Some("7d".to_owned()),
            notify: Some("dev@example.com".to_owned())
        }
    );
}

#[test]
fn parses_inbox_and_retention_calls() {
    let empty = Scope::default();
    assert_eq!(
        parse(
            "blobyard_create_inbox",
            json!({ "name": "Logs", "expires": "24h" })
        ),
        ToolCall::CreateInbox {
            scope: empty.clone(),
            name: "Logs".to_owned(),
            expires: Some("24h".to_owned())
        }
    );
    assert_eq!(
        parse("blobyard_revoke_inbox", json!({ "inbox_id": "inbox_1" })),
        ToolCall::RevokeInbox {
            scope: empty.clone(),
            inbox_id: "inbox_1".to_owned()
        }
    );
    assert_eq!(
        parse(
            "blobyard_set_retention",
            json!({ "latest": 5, "branch": "main", "path": "builds/*" })
        ),
        ToolCall::SetRetention {
            scope: empty.clone(),
            latest: 5,
            branch: Some("main".to_owned()),
            path: Some("builds/*".to_owned())
        }
    );
    assert_eq!(
        parse("blobyard_clear_retention", json!({})),
        ToolCall::ClearRetention { scope: empty }
    );
}

#[test]
fn parses_web_yard_calls_with_explicit_public_and_delete_confirmation() {
    let scope = Scope {
        workspace: Some("team".into()),
        project: Some("web".into()),
    };
    assert_eq!(
        parse(
            "blobyard_deploy_web_yard",
            json!({
                "workspace": "team", "project": "web", "directory": "./dist",
                "yard": "documentation", "spa": true, "clean_urls": true, "public": true
            })
        ),
        ToolCall::WebYard(WebYardToolCall::DeployWebYard {
            scope,
            directory: "./dist".into(),
            yard: "documentation".into(),
            spa: true,
            clean_urls: true,
        })
    );
    assert_eq!(
        parse("blobyard_list_web_yards", json!({})),
        ToolCall::WebYard(WebYardToolCall::ListWebYards {
            scope: Scope::default()
        })
    );
    assert_eq!(
        parse(
            "blobyard_list_yard_deploys",
            json!({ "yard": "documentation" })
        ),
        ToolCall::WebYard(WebYardToolCall::ListYardDeploys {
            scope: Scope::default(),
            yard: "documentation".into()
        })
    );
    assert_eq!(
        parse(
            "blobyard_rollback_web_yard",
            json!({ "yard": "documentation", "deploy_id": "deploy_1" })
        ),
        ToolCall::WebYard(WebYardToolCall::RollbackWebYard {
            scope: Scope::default(),
            yard: "documentation".into(),
            deploy_id: Some("deploy_1".into())
        })
    );
    assert_eq!(
        parse(
            "blobyard_delete_web_yard",
            json!({ "yard": "documentation", "confirm": true })
        ),
        ToolCall::WebYard(WebYardToolCall::DeleteWebYard {
            scope: Scope::default(),
            yard: "documentation".into()
        })
    );
}
