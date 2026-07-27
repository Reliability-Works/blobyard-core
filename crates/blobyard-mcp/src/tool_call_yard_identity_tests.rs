use super::parse;
use crate::{Scope, ToolCall, WebYardToolCall};
use serde_json::{Value, json};

#[test]
fn parses_yard_management_role_calls() {
    assert_eq!(
        parse(
            "blobyard_list_yard_management_roles",
            json!({ "yard": "documentation", "cursor": "next" })
        ),
        ToolCall::WebYard(WebYardToolCall::ListYardManagementRoles {
            scope: Scope::default(),
            yard: "documentation".into(),
            cursor: Some("next".into()),
        })
    );
    assert_eq!(
        parse(
            "blobyard_set_yard_management_role",
            json!({
                "yard": "documentation",
                "user_id": "user_developer",
                "role": "developer"
            })
        ),
        ToolCall::WebYard(WebYardToolCall::SetYardManagementRole {
            scope: Scope::default(),
            yard: "documentation".into(),
            user_id: "user_developer".into(),
            role: "developer".into(),
        })
    );
    assert_eq!(
        parse(
            "blobyard_revoke_yard_management_role",
            json!({ "yard": "documentation", "user_id": "user_developer" })
        ),
        ToolCall::WebYard(WebYardToolCall::RevokeYardManagementRole {
            scope: Scope::default(),
            yard: "documentation".into(),
            user_id: "user_developer".into(),
        })
    );
}

#[test]
fn parses_yard_application_policy_calls() {
    assert_eq!(
        parse(
            "blobyard_get_yard_application_policy",
            json!({ "yard": "documentation" })
        ),
        ToolCall::WebYard(WebYardToolCall::GetYardApplicationPolicy {
            scope: Scope::default(),
            yard: "documentation".into(),
        })
    );
    assert_eq!(
        parse(
            "blobyard_set_yard_application_policy",
            json!({
                "yard": "documentation",
                "source_manifest_digest": "aaaaaaaa",
                "default_role": null,
                "roles": {
                    "viewer": {
                        "inherits": [],
                        "permissions": ["content.read"]
                    }
                }
            })
        ),
        ToolCall::WebYard(WebYardToolCall::SetYardApplicationPolicy {
            scope: Scope::default(),
            yard: "documentation".into(),
            source_manifest_digest: "aaaaaaaa".into(),
            default_role: None,
            roles: json!({
                "viewer": {
                    "inherits": [],
                    "permissions": ["content.read"]
                }
            }),
        })
    );
    assert!(matches!(
        parse(
            "blobyard_set_yard_application_policy",
            json!({
                "yard": "documentation",
                "source_manifest_digest": "bbbbbbbb",
                "default_role": "viewer",
                "roles": {}
            })
        ),
        ToolCall::WebYard(WebYardToolCall::SetYardApplicationPolicy {
            default_role: Some(role),
            ..
        }) if role == "viewer"
    ));
}

#[test]
fn parses_yard_access_role_calls() {
    assert_eq!(
        parse(
            "blobyard_set_yard_access_roles",
            json!({
                "yard": "documentation",
                "grant_id": "yardgrant_reader",
                "roles": ["viewer", "editor"]
            })
        ),
        ToolCall::WebYard(WebYardToolCall::SetYardAccessRoles {
            scope: Scope::default(),
            yard: "documentation".into(),
            grant_id: "yardgrant_reader".into(),
            roles: vec!["viewer".into(), "editor".into()],
        })
    );
}

#[test]
fn rejects_incomplete_or_malformed_yard_identity_calls() {
    let cases = [
        (
            "blobyard_set_yard_application_policy",
            json!({
                "yard": "documentation",
                "source_manifest_digest": "aaaaaaaa",
                "roles": {}
            }),
            "missing required argument: default_role",
        ),
        (
            "blobyard_set_yard_application_policy",
            json!({
                "yard": "documentation",
                "source_manifest_digest": "aaaaaaaa",
                "default_role": "",
                "roles": {}
            }),
            "default_role must be a non-empty string or null",
        ),
        (
            "blobyard_set_yard_application_policy",
            json!({
                "yard": "documentation",
                "source_manifest_digest": "aaaaaaaa",
                "default_role": null,
                "roles": []
            }),
            "roles must be an object",
        ),
        (
            "blobyard_set_yard_access_roles",
            json!({
                "yard": "documentation",
                "grant_id": "yardgrant_reader"
            }),
            "missing required argument: roles",
        ),
        (
            "blobyard_set_yard_access_roles",
            json!({
                "yard": "documentation",
                "grant_id": "yardgrant_reader",
                "roles": [""]
            }),
            "roles must contain non-empty strings",
        ),
    ];
    assert_rejections(&cases);
}

#[test]
fn every_management_role_argument_boundary_fails_at_its_own_field() {
    let cases = [
        (
            "blobyard_list_yard_management_roles",
            json!({}),
            "missing required argument: yard",
        ),
        (
            "blobyard_list_yard_management_roles",
            json!({ "yard": "documentation", "cursor": "" }),
            "cursor must be a non-empty string",
        ),
        (
            "blobyard_set_yard_management_role",
            json!({}),
            "missing required argument: yard",
        ),
        (
            "blobyard_set_yard_management_role",
            json!({ "yard": "documentation" }),
            "missing required argument: user_id",
        ),
        (
            "blobyard_set_yard_management_role",
            json!({ "yard": "documentation", "user_id": "user_developer" }),
            "missing required argument: role",
        ),
        (
            "blobyard_revoke_yard_management_role",
            json!({}),
            "missing required argument: yard",
        ),
        (
            "blobyard_revoke_yard_management_role",
            json!({ "yard": "documentation" }),
            "missing required argument: user_id",
        ),
    ];
    assert_rejections(&cases);
}

#[test]
fn every_policy_and_access_role_argument_boundary_fails_at_its_own_field() {
    let cases = [
        (
            "blobyard_get_yard_application_policy",
            json!({}),
            "missing required argument: yard",
        ),
        (
            "blobyard_set_yard_application_policy",
            json!({}),
            "missing required argument: yard",
        ),
        (
            "blobyard_set_yard_application_policy",
            json!({ "yard": "documentation" }),
            "missing required argument: source_manifest_digest",
        ),
        (
            "blobyard_set_yard_application_policy",
            json!({
                "yard": "documentation",
                "source_manifest_digest": "aaaaaaaa",
                "default_role": null
            }),
            "roles must be an object",
        ),
        (
            "blobyard_set_yard_access_roles",
            json!({}),
            "missing required argument: yard",
        ),
        (
            "blobyard_set_yard_access_roles",
            json!({ "yard": "documentation" }),
            "missing required argument: grant_id",
        ),
    ];
    assert_rejections(&cases);
}

fn assert_rejections(cases: &[(&str, Value, &str)]) {
    for (name, arguments, expected) in cases {
        assert_eq!(
            ToolCall::parse(name, arguments).expect_err("invalid identity call"),
            *expected
        );
    }
}
