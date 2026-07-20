#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use serde_json::json;

#[test]
fn confirmation_must_be_boolean() {
    let error = parse_admin_call(
        "revoke_invite",
        &arguments(&json!({ "confirm": "yes", "invite_id": "invite_1" })),
        Scope::default(),
    )
    .expect_err("non-boolean confirmation");
    assert_eq!(error, "confirm must be a boolean");
}

#[test]
fn confirmed_mutations_require_each_operation_argument() {
    for (name, value) in [
        ("revoke_invite", json!({ "confirm": true })),
        (
            "update_member_role",
            json!({ "confirm": true, "role": "member" }),
        ),
        (
            "update_member_role",
            json!({ "confirm": true, "user_id": "user_1" }),
        ),
        ("remove_member", json!({ "confirm": true })),
        ("revoke_api_token", json!({ "confirm": true })),
        ("revoke_ci_trust", json!({ "confirm": true })),
        ("revoke_cli_session", json!({ "confirm": true })),
    ] {
        assert!(parse_admin_call(name, &arguments(&value), Scope::default()).is_err());
    }
}
