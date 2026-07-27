use super::super::parse;
use crate::{Scope, ToolCall, WebYardToolCall, YardGuestInviteToolCall};
use serde_json::json;

#[test]
fn parses_yard_guest_invitation_calls() {
    assert_eq!(
        parse(
            "blobyard_list_yard_guest_invites",
            json!({ "yard": "documentation", "cursor": "next" }),
        ),
        ToolCall::WebYard(WebYardToolCall::GuestInvite(
            YardGuestInviteToolCall::List {
                scope: Scope::default(),
                yard: "documentation".into(),
                cursor: Some("next".into()),
            },
        ))
    );
    assert_eq!(
        parse(
            "blobyard_revoke_yard_guest_invite",
            json!({
                "yard": "documentation",
                "invitation_id": "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }),
        ),
        ToolCall::WebYard(WebYardToolCall::GuestInvite(
            YardGuestInviteToolCall::Revoke {
                scope: Scope::default(),
                yard: "documentation".into(),
                invitation_id: "ygi_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            },
        ))
    );
}

#[test]
fn parses_yard_guest_invitation_create_calls() {
    assert_eq!(
        parse(
            "blobyard_create_yard_guest_invite",
            json!({
                "yard": "documentation",
                "email": "guest@example.com",
                "roles": ["viewer"],
                "environment_id": "yardenv_docs",
                "expires_at": "2026-08-03T09:00:00Z"
            }),
        ),
        ToolCall::WebYard(WebYardToolCall::GuestInvite(
            YardGuestInviteToolCall::Create {
                scope: Scope::default(),
                yard: "documentation".into(),
                email: "guest@example.com".into(),
                roles: vec!["viewer".into()],
                environment_id: Some("yardenv_docs".into()),
                expires_at: Some("2026-08-03T09:00:00Z".into()),
            },
        ))
    );
    assert_eq!(
        parse(
            "blobyard_create_yard_guest_invite",
            json!({
                "yard": "documentation",
                "email": "guest@example.com"
            }),
        ),
        ToolCall::WebYard(WebYardToolCall::GuestInvite(
            YardGuestInviteToolCall::Create {
                scope: Scope::default(),
                yard: "documentation".into(),
                email: "guest@example.com".into(),
                roles: Vec::new(),
                environment_id: None,
                expires_at: None,
            },
        ))
    );
}

#[test]
fn rejects_malformed_yard_guest_invitation_calls() {
    for (name, arguments) in [
        ("blobyard_list_yard_guest_invites", json!({})),
        (
            "blobyard_list_yard_guest_invites",
            json!({ "yard": "documentation", "cursor": 1 }),
        ),
        (
            "blobyard_create_yard_guest_invite",
            json!({
                "yard": "documentation",
                "email": "guest@example.com",
                "expires_at": "2026-08-03T09:00:00Z",
                "token": "must-not-be-accepted"
            }),
        ),
        ("blobyard_create_yard_guest_invite", json!({})),
        (
            "blobyard_create_yard_guest_invite",
            json!({ "yard": "documentation" }),
        ),
        (
            "blobyard_create_yard_guest_invite",
            json!({
                "yard": "documentation",
                "email": "guest@example.com",
                "roles": "viewer"
            }),
        ),
        (
            "blobyard_create_yard_guest_invite",
            json!({
                "yard": "documentation",
                "email": "guest@example.com",
                "roles": ["viewer"],
                "environment_id": 1
            }),
        ),
        (
            "blobyard_create_yard_guest_invite",
            json!({
                "yard": "documentation",
                "email": "guest@example.com",
                "roles": ["viewer"],
                "expires_at": 1
            }),
        ),
        ("blobyard_revoke_yard_guest_invite", json!({})),
        (
            "blobyard_revoke_yard_guest_invite",
            json!({ "yard": "documentation" }),
        ),
    ] {
        assert!(ToolCall::parse(name, &arguments).is_err());
    }
}
