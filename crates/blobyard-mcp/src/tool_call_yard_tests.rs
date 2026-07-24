#![allow(
    clippy::expect_used,
    reason = "tool parsing tests use fixed JSON fixtures"
)]

use super::parse;
use crate::{Scope, ToolCall, WebYardToolCall};
use serde_json::json;

#[test]
fn parses_yard_environment_list_calls() {
    assert_eq!(
        parse(
            "blobyard_list_yard_environments",
            json!({ "yard": "documentation" })
        ),
        ToolCall::WebYard(WebYardToolCall::ListYardEnvironments {
            scope: Scope::default(),
            yard: "documentation".into()
        })
    );
}

#[test]
fn parses_yard_access_reads_and_visibility_calls() {
    assert_eq!(
        parse(
            "blobyard_get_yard_access",
            json!({ "yard": "documentation" })
        ),
        ToolCall::WebYard(WebYardToolCall::GetYardAccess {
            scope: Scope::default(),
            yard: "documentation".into()
        })
    );
    assert_eq!(
        parse(
            "blobyard_set_yard_visibility",
            json!({ "yard": "documentation", "visibility": "owner" })
        ),
        ToolCall::WebYard(WebYardToolCall::SetYardVisibility {
            scope: Scope::default(),
            yard: "documentation".into(),
            visibility: "owner".into()
        })
    );
}

#[test]
fn parses_yard_access_grant_and_revoke_calls() {
    assert_eq!(
        parse(
            "blobyard_grant_yard_access",
            json!({
                "yard": "documentation",
                "principal_kind": "user",
                "principal_id": "user_reader",
                "roles": ["viewer", "editor"],
                "environment_id": "yardenv_yard_documentation",
                "expires_at": "2100-01-01T00:00:00Z"
            })
        ),
        ToolCall::WebYard(WebYardToolCall::GrantYardAccess {
            scope: Scope::default(),
            yard: "documentation".into(),
            principal_kind: "user".into(),
            principal_id: "user_reader".into(),
            roles: vec!["viewer".into(), "editor".into()],
            environment_id: Some("yardenv_yard_documentation".into()),
            expires_at: Some("2100-01-01T00:00:00Z".into())
        })
    );
    assert_eq!(
        parse(
            "blobyard_grant_yard_access",
            json!({
                "yard": "documentation",
                "principal_kind": "group",
                "principal_id": "group_docs"
            })
        ),
        ToolCall::WebYard(WebYardToolCall::GrantYardAccess {
            scope: Scope::default(),
            yard: "documentation".into(),
            principal_kind: "group".into(),
            principal_id: "group_docs".into(),
            roles: Vec::new(),
            environment_id: None,
            expires_at: None
        })
    );
    assert_eq!(
        parse(
            "blobyard_revoke_yard_access",
            json!({ "yard": "documentation", "grant_id": "yardgrant_1" })
        ),
        ToolCall::WebYard(WebYardToolCall::RevokeYardAccess {
            scope: Scope::default(),
            yard: "documentation".into(),
            grant_id: "yardgrant_1".into()
        })
    );
}

#[test]
fn parses_yard_session_list_and_revoke_calls() {
    assert_eq!(
        parse(
            "blobyard_list_yard_sessions",
            json!({ "yard": "documentation" })
        ),
        ToolCall::WebYard(WebYardToolCall::ListYardSessions {
            scope: Scope::default(),
            yard: "documentation".into(),
        })
    );
    assert_eq!(
        parse(
            "blobyard_revoke_yard_session",
            json!({ "yard": "documentation", "session_id": "byys_session" })
        ),
        ToolCall::WebYard(WebYardToolCall::RevokeYardSession {
            scope: Scope::default(),
            yard: "documentation".into(),
            session_id: "byys_session".into(),
        })
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
