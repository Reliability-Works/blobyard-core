#![allow(
    clippy::expect_used,
    clippy::needless_pass_by_value,
    reason = "fixed JSON tool fixtures stay concise and must parse"
)]

use crate::{DashboardToolCall, Scope, ToolCall};
use serde_json::json;

fn parse(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall::parse(name, &arguments).expect("dashboard tool must parse")
}

#[test]
fn parses_safe_dashboard_tools() {
    assert_eq!(
        parse(
            "blobyard_rename_workspace",
            json!({ "workspace": "team", "name": "Product" }),
        ),
        ToolCall::Dashboard(DashboardToolCall::RenameWorkspace {
            scope: Scope {
                workspace: Some("team".into()),
                project: None,
            },
            name: "Product".into(),
        })
    );
    assert!(ToolCall::parse("blobyard_complete_account_deletion", &json!({})).is_err());
}

#[test]
fn parses_export_tools_without_deletion_confirmation_tokens() {
    assert_eq!(
        parse("blobyard_request_account_export", json!({})),
        ToolCall::Dashboard(DashboardToolCall::RequestAccountExport {
            scope: Scope::default(),
        })
    );
    assert!(ToolCall::parse("blobyard_prepare_account_deletion", &json!({})).is_err());
}

#[test]
fn parses_dashboard_read_tools() {
    for (name, expected) in [
        (
            "blobyard_get_billing",
            DashboardToolCall::GetBilling {
                scope: Scope::default(),
            },
        ),
        (
            "blobyard_get_account_export",
            DashboardToolCall::GetAccountExport {
                scope: Scope::default(),
            },
        ),
        (
            "blobyard_get_account_deletion",
            DashboardToolCall::GetAccountDeletion {
                scope: Scope::default(),
            },
        ),
        (
            "blobyard_get_retention_overview",
            DashboardToolCall::GetRetentionOverview {
                scope: Scope::default(),
            },
        ),
    ] {
        assert_eq!(parse(name, json!({})), ToolCall::Dashboard(expected));
    }
}

#[test]
fn rejects_hosted_billing_session_tools() {
    for name in [
        "blobyard_create_billing_checkout",
        "blobyard_create_billing_portal",
        "blobyard_create_storage_checkout",
        "blobyard_create_storage_update",
        "blobyard_create_billing_subscription_update",
    ] {
        assert!(
            ToolCall::parse(name, &json!({})).is_err(),
            "{name} must be absent"
        );
    }
}

#[test]
fn dashboard_parser_fails_closed_for_unknown_operations() {
    let error = crate::dashboard_call::parse_dashboard_call(
        "unknown",
        &serde_json::Map::new(),
        Scope::default(),
    )
    .expect_err("unknown dashboard operation");
    assert_eq!(error, "unknown tool: unknown");
}

#[test]
fn dashboard_parser_rejects_operation_and_scope_argument_errors() {
    for (name, arguments, expected) in [
        (
            "blobyard_get_billing",
            json!({ "extra": true }),
            "unexpected argument",
        ),
        (
            "blobyard_rename_workspace",
            json!({}),
            "missing required argument",
        ),
        (
            "blobyard_get_billing",
            json!({ "workspace": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_get_billing",
            json!({ "project": 1 }),
            "non-empty string",
        ),
    ] {
        let error = ToolCall::parse(name, &arguments).expect_err("invalid dashboard arguments");
        assert!(error.contains(expected), "unexpected error: {error}");
    }
}

#[test]
fn dashboard_parser_directly_rejects_malformed_scope_maps() {
    for key in ["workspace", "project"] {
        let arguments = serde_json::Map::from_iter([(key.to_owned(), json!(1))]);
        let error = crate::dashboard_call::parse_dashboard_call(
            "get_billing",
            &arguments,
            Scope::default(),
        )
        .expect_err("malformed dashboard scope");
        assert_eq!(error, format!("{key} must be a non-empty string"));
    }
}
