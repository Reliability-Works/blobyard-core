#![allow(
    clippy::too_many_lines,
    reason = "the parser matrix keeps all seven group tools visible in one table"
)]

use super::{GroupToolCall, parse_group_call};
use crate::Scope;
use serde_json::json;

#[test]
fn parses_group_reads_writes_and_confirmed_destructive_calls() {
    let cases = [
        (
            "list_groups",
            json!({ "cursor": "next" }),
            GroupToolCall::List {
                scope: Scope::default(),
                cursor: Some("next".to_owned()),
            },
        ),
        (
            "create_group",
            json!({ "name": "Reviewers" }),
            GroupToolCall::Create {
                scope: Scope::default(),
                name: "Reviewers".to_owned(),
            },
        ),
        (
            "rename_group",
            json!({ "group_id": "group_fixture", "name": "Reviewers" }),
            GroupToolCall::Rename {
                scope: Scope::default(),
                group_id: "group_fixture".to_owned(),
                name: "Reviewers".to_owned(),
            },
        ),
        (
            "list_group_members",
            json!({ "cursor": "member-next", "group_id": "group_fixture" }),
            GroupToolCall::ListMembers {
                scope: Scope::default(),
                group_id: "group_fixture".to_owned(),
                cursor: Some("member-next".to_owned()),
            },
        ),
        (
            "add_group_member",
            json!({
                "group_id": "group_fixture",
                "user_id": "user_fixture"
            }),
            GroupToolCall::AddMember {
                scope: Scope::default(),
                group_id: "group_fixture".to_owned(),
                user_id: "user_fixture".to_owned(),
            },
        ),
        (
            "remove_group_member",
            json!({
                "confirm": true,
                "group_id": "group_fixture",
                "user_id": "user_fixture"
            }),
            GroupToolCall::RemoveMember {
                scope: Scope::default(),
                group_id: "group_fixture".to_owned(),
                user_id: "user_fixture".to_owned(),
                confirmed: true,
            },
        ),
        (
            "deactivate_group",
            json!({ "confirm": true, "group_id": "group_fixture" }),
            GroupToolCall::Deactivate {
                scope: Scope::default(),
                group_id: "group_fixture".to_owned(),
                confirmed: true,
            },
        ),
    ];
    for (name, value, expected) in cases {
        let arguments = value.as_object().cloned().unwrap_or_default();
        assert_eq!(
            parse_group_call(name, &arguments, Scope::default()),
            Ok(expected)
        );
    }
}

#[test]
fn rejects_missing_confirmation_and_unknown_arguments() {
    for (name, value) in [
        ("deactivate_group", json!({ "group_id": "group_fixture" })),
        (
            "add_group_member",
            json!({ "group_id": "group_fixture", "user_id": "user_fixture", "extra": true }),
        ),
    ] {
        let arguments = value.as_object().cloned().unwrap_or_default();
        assert!(parse_group_call(name, &arguments, Scope::default()).is_err());
    }
}

#[test]
fn recognizes_only_group_tools_and_rejects_unknown_calls() {
    assert!(super::is_group_tool("list_groups"));
    assert!(!super::is_group_tool("list_audit"));
    assert_eq!(
        parse_group_call("unknown", &serde_json::Map::new(), Scope::default()),
        Err("unknown tool: unknown".to_owned())
    );
}

#[test]
fn rejects_each_missing_or_invalid_group_argument() {
    for (name, value) in [
        ("list_groups", json!({ "cursor": "" })),
        ("create_group", json!({})),
        ("rename_group", json!({ "name": "Reviewers" })),
        ("rename_group", json!({ "group_id": "group_fixture" })),
        ("list_group_members", json!({ "cursor": "next" })),
        (
            "list_group_members",
            json!({ "cursor": 1, "group_id": "group_fixture" }),
        ),
        ("add_group_member", json!({ "user_id": "user_fixture" })),
        ("add_group_member", json!({ "group_id": "group_fixture" })),
        (
            "remove_group_member",
            json!({ "confirm": true, "user_id": "user_fixture" }),
        ),
        (
            "remove_group_member",
            json!({ "group_id": "group_fixture", "user_id": "user_fixture" }),
        ),
        (
            "remove_group_member",
            json!({ "confirm": true, "group_id": "group_fixture" }),
        ),
        ("deactivate_group", json!({ "confirm": true })),
    ] {
        assert!(
            parse_group_call(
                name,
                &value.as_object().cloned().unwrap_or_default(),
                Scope::default(),
            )
            .is_err(),
            "{name}"
        );
    }
}
