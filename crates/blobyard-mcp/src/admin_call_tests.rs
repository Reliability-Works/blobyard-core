#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use serde_json::json;

fn parse(name: &str, value: &Value) -> AdminToolCall {
    parse_admin_call(name, &arguments(value), Scope::default()).expect("valid administration call")
}

#[test]
fn recognizes_only_administration_tools() {
    for name in [
        "list_audit",
        "list_members",
        "list_invites",
        "create_invite",
        "revoke_invite",
        "update_member_role",
        "remove_member",
        "list_api_tokens",
        "revoke_api_token",
        "list_ci_trusts",
        "create_ci_trust",
        "revoke_ci_trust",
        "list_cli_sessions",
        "revoke_cli_session",
    ] {
        assert!(is_admin_tool(name));
    }
    assert!(!is_admin_tool("upload_file"));
}

#[test]
fn parses_administration_reads() {
    assert_eq!(
        parse("list_audit", &json!({ "cursor": "next" })),
        AdminToolCall::ListAudit {
            scope: Scope::default(),
            cursor: Some("next".to_owned()),
        }
    );
    assert_eq!(
        parse("list_audit", &json!({})),
        AdminToolCall::ListAudit {
            scope: Scope::default(),
            cursor: None,
        }
    );
    for (name, expected) in [
        (
            "list_members",
            AdminToolCall::ListMembers {
                scope: Scope::default(),
            },
        ),
        (
            "list_invites",
            AdminToolCall::ListInvites {
                scope: Scope::default(),
            },
        ),
        (
            "list_api_tokens",
            AdminToolCall::ListApiTokens {
                scope: Scope::default(),
            },
        ),
        (
            "list_ci_trusts",
            AdminToolCall::ListCiTrusts {
                scope: Scope::default(),
            },
        ),
        (
            "list_cli_sessions",
            AdminToolCall::ListCliSessions {
                scope: Scope::default(),
            },
        ),
    ] {
        assert_eq!(parse(name, &json!({})), expected);
    }
}

#[test]
fn parses_member_administration_writes() {
    assert_eq!(
        parse(
            "create_invite",
            &json!({ "email": "developer@example.com", "role": "admin" }),
        ),
        AdminToolCall::CreateInvite {
            scope: Scope::default(),
            email: "developer@example.com".to_owned(),
            role: "admin".to_owned(),
        }
    );
    assert_eq!(
        parse(
            "revoke_invite",
            &json!({ "confirm": true, "invite_id": "invite_1" }),
        ),
        AdminToolCall::RevokeInvite {
            scope: Scope::default(),
            invite_id: "invite_1".to_owned(),
            confirmed: true,
        }
    );
    assert_eq!(
        parse(
            "update_member_role",
            &json!({ "confirm": true, "role": "owner", "user_id": "user_1" }),
        ),
        AdminToolCall::UpdateMemberRole {
            scope: Scope::default(),
            user_id: "user_1".to_owned(),
            role: "owner".to_owned(),
            confirmed: true,
        }
    );
    assert_eq!(
        parse(
            "remove_member",
            &json!({ "confirm": true, "user_id": "user_1" }),
        ),
        AdminToolCall::RemoveMember {
            scope: Scope::default(),
            user_id: "user_1".to_owned(),
            confirmed: true,
        }
    );
}

#[test]
fn parses_credential_administration_writes() {
    assert_eq!(
        parse(
            "revoke_api_token",
            &json!({ "confirm": true, "token_id": "token_1" }),
        ),
        AdminToolCall::RevokeApiToken {
            scope: Scope::default(),
            token_id: "token_1".to_owned(),
            confirmed: true,
        }
    );
    assert_eq!(
        parse(
            "revoke_ci_trust",
            &json!({ "confirm": true, "trust_id": "trust_1" }),
        ),
        AdminToolCall::RevokeCiTrust {
            scope: Scope::default(),
            trust_id: "trust_1".to_owned(),
            confirmed: true,
        }
    );
    assert_eq!(
        parse(
            "revoke_cli_session",
            &json!({ "confirm": true, "session_id": "session_1" }),
        ),
        AdminToolCall::RevokeCliSession {
            scope: Scope::default(),
            session_id: "session_1".to_owned(),
            confirmed: true,
        }
    );
}

#[test]
fn parses_ci_trust_with_optional_environment() {
    let required = json!({
        "allowed_actions": ["upload", "share"],
        "allowed_ref_glob": "refs/heads/main",
        "repository": "acme/artifacts",
        "workflow_path": ".github/workflows/upload-artifacts.yml",
        "workflow_ref": "refs/heads/main"
    });
    let parsed = parse("create_ci_trust", &required);
    assert!(matches!(
        parsed,
        AdminToolCall::CreateCiTrust {
            environment: None,
            ..
        }
    ));
    let mut with_environment = arguments(&required);
    with_environment.insert("environment".to_owned(), json!("Production"));
    assert!(matches!(
        parse_admin_call("create_ci_trust", &with_environment, Scope::default()),
        Ok(AdminToolCall::CreateCiTrust {
            allowed_actions,
            environment: Some(environment),
            ..
        }) if allowed_actions == ["upload", "share"] && environment == "Production"
    ));
}

#[test]
fn rejects_unknown_missing_and_invalid_scalar_arguments() {
    for (name, value, expected) in [
        ("unknown", json!({}), "unknown tool"),
        (
            "list_members",
            json!({ "extra": true }),
            "unexpected argument",
        ),
        (
            "create_invite",
            json!({ "role": "member" }),
            "missing required argument",
        ),
        (
            "create_invite",
            json!({ "email": "developer@example.com", "role": "invalid" }),
            "role is not valid",
        ),
        ("list_audit", json!({ "cursor": "" }), "non-empty string"),
        ("list_audit", json!({ "cursor": 1 }), "non-empty string"),
        (
            "revoke_invite",
            json!({ "confirm": false, "invite_id": "invite_1" }),
            "confirm must be true",
        ),
    ] {
        let error = parse_admin_call(name, &arguments(&value), Scope::default())
            .expect_err("invalid arguments must fail");
        assert!(error.contains(expected));
    }
    let confirmed = arguments(&json!({ "confirm": true }));
    assert_eq!(
        parse_confirmed_admin_call("unknown", &confirmed, Scope::default()),
        Err("unknown tool: unknown".to_owned())
    );
}

#[test]
fn rejects_invalid_ci_action_arrays() {
    let base = json!({
        "allowed_ref_glob": "refs/heads/main",
        "repository": "acme/artifacts",
        "workflow_path": ".github/workflows/upload-artifacts.yml",
        "workflow_ref": "refs/heads/main"
    });
    for actions in [json!(null), json!([]), json!([""]), json!([1])] {
        let mut invalid = arguments(&base);
        invalid.insert("allowed_actions".to_owned(), actions);
        assert!(parse_admin_call("create_ci_trust", &invalid, Scope::default()).is_err());
    }
}

#[test]
fn rejects_each_missing_administration_identifier() {
    for (name, value) in [
        ("create_invite", json!({ "email": "developer@example.com" })),
        ("revoke_invite", json!({})),
        ("update_member_role", json!({ "role": "member" })),
        ("update_member_role", json!({ "user_id": "user_1" })),
        ("remove_member", json!({})),
        ("revoke_api_token", json!({})),
        ("revoke_ci_trust", json!({})),
        ("revoke_cli_session", json!({})),
    ] {
        assert!(parse_admin_call(name, &arguments(&value), Scope::default()).is_err());
    }
}

#[test]
fn rejects_each_missing_ci_trust_field() {
    let required = [
        ("allowed_actions", json!(["upload"])),
        ("allowed_ref_glob", json!("refs/heads/main")),
        ("repository", json!("acme/artifacts")),
        (
            "workflow_path",
            json!(".github/workflows/upload-artifacts.yml"),
        ),
        ("workflow_ref", json!("refs/heads/main")),
    ];
    for omitted in required.iter().map(|(key, _)| *key) {
        let arguments = required
            .iter()
            .filter(|(key, _)| *key != omitted)
            .cloned()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<Map<_, _>>();
        assert!(parse_admin_call("create_ci_trust", &arguments, Scope::default()).is_err());
    }
}

#[test]
fn scalar_helpers_reject_missing_required_and_invalid_optional_values() {
    assert_eq!(
        required_string(&Map::new(), "identifier"),
        Err("missing required argument: identifier".to_owned())
    );
    assert!(required_string(&arguments(&json!({ "identifier": 42 })), "identifier").is_err());
    let arguments = arguments(&json!({
        "allowed_actions": ["upload"],
        "allowed_ref_glob": "refs/heads/main",
        "environment": 42,
        "repository": "acme/artifacts",
        "workflow_path": ".github/workflows/upload-artifacts.yml",
        "workflow_ref": "refs/heads/main"
    }));
    assert!(parse_admin_call("create_ci_trust", &arguments, Scope::default()).is_err());
}
