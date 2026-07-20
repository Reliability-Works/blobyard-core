use crate::{Scope, ToolCall};
use serde_json::{Map, json};

#[test]
fn rejects_general_and_scope_argument_errors() {
    assert_invalid([
        ("whoami", json!({}), "unknown tool"),
        ("blobyard_missing", json!({}), "unknown tool"),
        ("blobyard_whoami", json!([]), "must be an object"),
        (
            "blobyard_whoami",
            json!({ "extra": true }),
            "unexpected argument",
        ),
        ("blobyard_create_project", json!({}), "missing required"),
        (
            "blobyard_create_project",
            json!({ "name": "" }),
            "non-empty string",
        ),
        ("blobyard_create_workspace", json!({}), "missing required"),
        (
            "blobyard_whoami",
            json!({ "workspace": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_list_objects",
            json!({ "versions": "yes" }),
            "must be a boolean",
        ),
        (
            "blobyard_list_objects",
            json!({ "prefix": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_whoami",
            json!({ "project": 1 }),
            "non-empty string",
        ),
    ]);
}

#[test]
fn rejects_transfer_argument_errors() {
    assert_invalid([
        ("blobyard_upload_file", json!({}), "missing required"),
        (
            "blobyard_upload_file",
            json!({ "source": "a", "path": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_upload_file",
            json!({ "source": "a", "include_ignored": 1 }),
            "must be a boolean",
        ),
        (
            "blobyard_download_file",
            json!({ "output": "a" }),
            "missing required",
        ),
        (
            "blobyard_download_file",
            json!({ "uri": "u" }),
            "missing required",
        ),
        (
            "blobyard_download_file",
            json!({ "uri": "u", "output": "a", "force": 1 }),
            "must be a boolean",
        ),
        ("blobyard_delete_object", json!({}), "missing required"),
    ]);
}

#[test]
fn rejects_sharing_and_retention_argument_errors() {
    assert_invalid([
        ("blobyard_create_share", json!({}), "missing required"),
        ("blobyard_revoke_share", json!({}), "missing required"),
        ("blobyard_create_preview", json!({}), "missing required"),
        ("blobyard_revoke_preview", json!({}), "missing required"),
        (
            "blobyard_create_preview",
            json!({ "directory": "./site", "expires": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_create_share",
            json!({ "target": "a", "expires": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_create_share",
            json!({ "target": "a", "notify": 1 }),
            "non-empty string",
        ),
        ("blobyard_create_inbox", json!({}), "missing required"),
        (
            "blobyard_create_inbox",
            json!({ "name": "a", "expires": 1 }),
            "non-empty string",
        ),
        ("blobyard_revoke_inbox", json!({}), "missing required"),
        (
            "blobyard_set_retention",
            json!({ "latest": 1, "branch": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_set_retention",
            json!({ "latest": 1, "path": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_set_retention",
            json!({ "latest": 0 }),
            "positive 32-bit integer",
        ),
        (
            "blobyard_set_retention",
            json!({ "latest": 4_294_967_296_u64 }),
            "positive 32-bit integer",
        ),
    ]);
}

#[test]
fn rejects_implicit_or_malformed_web_yard_confirmation() {
    assert_invalid([
        (
            "blobyard_deploy_web_yard",
            json!({ "public": true }),
            "missing required",
        ),
        (
            "blobyard_deploy_web_yard",
            json!({ "directory": "dist", "public": true }),
            "missing required",
        ),
        (
            "blobyard_deploy_web_yard",
            json!({ "directory": "dist", "yard": "site" }),
            "missing required",
        ),
        (
            "blobyard_deploy_web_yard",
            json!({ "directory": "dist", "yard": "site", "public": "yes" }),
            "must be a boolean",
        ),
        (
            "blobyard_deploy_web_yard",
            json!({ "directory": "dist", "yard": "site", "public": false }),
            "must be true",
        ),
        (
            "blobyard_deploy_web_yard",
            json!({ "directory": "dist", "yard": "site", "public": true, "spa": 1 }),
            "must be a boolean",
        ),
        (
            "blobyard_deploy_web_yard",
            json!({ "directory": "dist", "yard": "site", "public": true, "clean_urls": 1 }),
            "must be a boolean",
        ),
    ]);
}

#[test]
fn rejects_malformed_web_yard_management_calls() {
    assert_invalid([
        ("blobyard_list_yard_deploys", json!({}), "missing required"),
        ("blobyard_rollback_web_yard", json!({}), "missing required"),
        (
            "blobyard_rollback_web_yard",
            json!({ "yard": "site", "deploy_id": 1 }),
            "non-empty string",
        ),
        (
            "blobyard_delete_web_yard",
            json!({ "yard": "site", "confirm": false }),
            "must be true",
        ),
        (
            "blobyard_delete_web_yard",
            json!({ "yard": "site" }),
            "missing required",
        ),
        (
            "blobyard_delete_web_yard",
            json!({ "confirm": true }),
            "missing required",
        ),
        (
            "blobyard_delete_web_yard",
            json!({ "yard": "site", "confirm": 1 }),
            "must be a boolean",
        ),
        (
            "blobyard_list_web_yards",
            json!({ "extra": true }),
            "unexpected argument",
        ),
    ]);
    assert!(crate::yard_call::parse_yard_call("unknown", &Map::new(), Scope::default()).is_err());
}

fn assert_invalid<const N: usize>(cases: [(&str, serde_json::Value, &str); N]) {
    for (name, arguments, message) in cases {
        let error = ToolCall::parse(name, &arguments).expect_err("invalid fixture must fail");
        assert!(error.contains(message), "unexpected error: {error}");
    }
}
