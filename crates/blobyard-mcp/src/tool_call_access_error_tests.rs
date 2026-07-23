use super::error_tests::assert_invalid;
use serde_json::json;

#[test]
fn rejects_malformed_yard_access_calls() {
    assert_invalid([
        ("blobyard_get_yard_access", json!({}), "missing required"),
        (
            "blobyard_set_yard_visibility",
            json!({ "yard": "site" }),
            "missing required",
        ),
        (
            "blobyard_grant_yard_access",
            json!({ "yard": "site", "principal_kind": "user" }),
            "missing required",
        ),
    ]);
}

#[test]
fn rejects_malformed_yard_access_grant_arguments() {
    assert_invalid([
        (
            "blobyard_grant_yard_access",
            json!({
                "yard": "site",
                "principal_kind": "user",
                "principal_id": "user_1",
                "roles": "viewer"
            }),
            "must be an array",
        ),
        (
            "blobyard_grant_yard_access",
            json!({
                "yard": "site",
                "principal_kind": "user",
                "principal_id": "user_1",
                "roles": [1]
            }),
            "non-empty strings",
        ),
        (
            "blobyard_grant_yard_access",
            json!({
                "yard": "site",
                "principal_kind": "user",
                "principal_id": "user_1",
                "roles": [""]
            }),
            "non-empty strings",
        ),
        (
            "blobyard_revoke_yard_access",
            json!({ "yard": "site" }),
            "missing required",
        ),
    ]);
}

#[test]
fn rejects_malformed_yard_access_revocations() {
    assert_invalid([
        (
            "blobyard_set_yard_visibility",
            json!({ "visibility": "owner" }),
            "missing required",
        ),
        ("blobyard_grant_yard_access", json!({}), "missing required"),
        (
            "blobyard_grant_yard_access",
            json!({ "yard": "site", "principal_id": "user_1" }),
            "missing required",
        ),
        (
            "blobyard_grant_yard_access",
            json!({
                "yard": "site",
                "principal_kind": "user",
                "principal_id": "user_1",
                "environment_id": 1
            }),
            "non-empty string",
        ),
        (
            "blobyard_grant_yard_access",
            json!({
                "yard": "site",
                "principal_kind": "user",
                "principal_id": "user_1",
                "expires_at": 1
            }),
            "non-empty string",
        ),
        (
            "blobyard_revoke_yard_access",
            json!({ "grant_id": "yardgrant_1" }),
            "missing required",
        ),
        (
            "blobyard_revoke_yard_access",
            json!({ "yard": "site", "grant_id": "yardgrant_1", "extra": true }),
            "unexpected argument",
        ),
    ]);
}
