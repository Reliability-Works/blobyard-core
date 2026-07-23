//! Local-user adapter contracts for bearer-authenticated CLI commands.

#![allow(clippy::expect_used, reason = "test fixture setup must fail loudly")]

use super::dashboard_contract::contract_support::human_stdout;
use super::support::{Fixture, ok};
use blobyard_api_client::Endpoint;
use blobyard_core::ErrorCode;

fn fixture_login_key() -> String {
    let mut value = String::from("byuk");
    value.push('_');
    value.push_str("0123456789ab");
    value
}

#[tokio::test]
async fn users_list_uses_the_scoped_versioned_endpoint() {
    let fixture = Fixture::new(
        &["blobyard", "users", "list", "--workspace", "team"],
        vec![ok(serde_json::json!({ "users": [] }), "req_users_list")],
        Some("token"),
        None,
    );
    let output = human_stdout(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("user list"),
    );
    assert_eq!(output, "{\n  \"users\": []\n}\n");
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::ListLocalUsers);
    assert_eq!(requests[0].query(), Some("workspace=team"));
}

#[tokio::test]
async fn users_create_sends_the_exact_body_and_reveals_the_key_once() {
    let login_key = fixture_login_key();
    let login_key_prefix = login_key
        .strip_suffix('b')
        .expect("fixture key suffix")
        .to_owned();
    let fixture = Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "team",
            "users",
            "create",
            "Ada Lovelace",
            "--email",
            "ada@example.test",
        ],
        vec![ok(
            serde_json::json!({
                "loginKey": login_key.clone(),
                "loginKeyPrefix": login_key_prefix,
                "user": { "id": "user_1" }
            }),
            "req_users_create",
        )],
        Some("token"),
        None,
    );
    let output = human_stdout(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("user creation"),
    );
    assert!(output.contains(&format!("Sign-in key: {login_key}")));
    assert!(output.contains("It will not be shown again."));
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::CreateLocalUser);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({
            "displayName": "Ada Lovelace",
            "email": "ada@example.test",
            "workspace": "team"
        }))
    );
}

#[tokio::test]
async fn users_reset_key_reveals_the_replacement_once() {
    let fixture = Fixture::new(
        &["blobyard", "users", "reset-key", "user_1"],
        vec![ok(
            serde_json::json!({
                "loginKey": "byuk_replacement",
                "loginKeyPrefix": "byuk_replacemen"
            }),
            "req_users_reset",
        )],
        Some("token"),
        None,
    );
    let output = human_stdout(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("key reset"),
    );
    assert!(output.contains("Sign-in key: byuk_replacement"));
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::ResetLocalUserLoginKey);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({ "userId": "user_1" }))
    );
}

#[tokio::test]
async fn users_deactivate_confirms_with_a_stable_human_result() {
    let fixture = Fixture::new(
        &["blobyard", "users", "deactivate", "user_1"],
        vec![ok(serde_json::json!({}), "req_users_deactivate")],
        Some("token"),
        None,
    );
    let output = human_stdout(
        fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect("deactivation"),
    );
    assert_eq!(output, "Local user deactivated.\n");
    let requests = fixture.transport.requests();
    assert_eq!(requests[0].endpoint(), Endpoint::DeactivateLocalUser);
    assert_eq!(
        requests[0].body(),
        Some(&serde_json::json!({ "userId": "user_1" }))
    );
}

#[tokio::test]
async fn users_commands_fail_closed_before_any_request() {
    for args in [
        vec!["blobyard", "users", "create", "Ada Lovelace"],
        vec!["blobyard", "--workspace", "team", "users", "create", " "],
        vec![
            "blobyard",
            "--workspace",
            "team",
            "users",
            "create",
            "Ada",
            "--email",
            "missing-at",
        ],
    ] {
        let fixture = Fixture::new(&args, vec![], Some("token"), None);
        let error = fixture
            .runner
            .execute(&fixture.command)
            .await
            .expect_err("rejected user creation");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
        assert!(fixture.transport.requests().is_empty());
    }
}

#[tokio::test]
async fn users_key_reveal_requires_the_raw_key_in_the_response() {
    let fixture = Fixture::new(
        &["blobyard", "users", "reset-key", "user_1"],
        vec![ok(serde_json::json!({}), "req_users_reset")],
        Some("token"),
        None,
    );
    let error = fixture
        .runner
        .execute(&fixture.command)
        .await
        .expect_err("missing raw key");
    assert_eq!(error.code(), ErrorCode::InternalError);
}
