#![allow(
    clippy::expect_used,
    reason = "fixed validation fixtures must fail loudly"
)]

use super::*;
use crate::headless_commands::{CreateTokenArgs, WorkspaceRole};

fn token(name: impl Into<String>, expires_days: u16, scopes: Vec<String>) -> CreateTokenArgs {
    CreateTokenArgs {
        name: name.into(),
        expires_days,
        scopes,
    }
}

#[test]
fn token_validation_checks_each_public_bound() {
    assert!(validate_token(&token("CI", 7, vec!["audit:read".to_owned()])).is_ok());
    for invalid in [
        token(" ", 7, vec!["audit:read".to_owned()]),
        token("a".repeat(81), 7, vec!["audit:read".to_owned()]),
        token("line\nbreak", 7, vec!["audit:read".to_owned()]),
        token("CI", 0, vec!["audit:read".to_owned()]),
        token("CI", 7, Vec::new()),
        token("CI", 7, vec!["audit:read".to_owned(); 21]),
        token("CI", 7, vec!["account:admin".to_owned()]),
    ] {
        assert_eq!(
            validate_token(&invalid)
                .expect_err("invalid token request")
                .code(),
            ErrorCode::InvalidRequest
        );
    }
}

#[test]
fn workspace_roles_have_stable_wire_values() {
    assert_eq!(std::hint::black_box(WorkspaceRole::Owner).as_str(), "owner");
    assert_eq!(std::hint::black_box(WorkspaceRole::Admin).as_str(), "admin");
    assert_eq!(
        std::hint::black_box(WorkspaceRole::Member).as_str(),
        "member"
    );
}

#[test]
fn administration_mappers_fail_closed_for_unrelated_commands() {
    let command = Command::Whoami;
    let scope = Scope::default();
    assert_eq!(
        admin_call(&command).expect_err("unrelated command").code(),
        ErrorCode::InternalError
    );
    assert!(audit_members_call(&command, scope.clone()).is_err());
    assert!(invites_tokens_call(&command, scope.clone()).is_err());
    assert!(trusts_sessions_call(&command, scope).is_err());
    assert_eq!(
        admin_human(&command, &json!({ "items": [] })),
        "{\n  \"items\": []\n}"
    );
}
