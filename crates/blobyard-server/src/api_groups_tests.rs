#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::contract_test_support::{assert_error, response_json, send};
use crate::transfers::test_seams::{self, TransferFixture};
use axum::http::StatusCode;

pub(super) fn fixture() -> TransferFixture {
    test_seams::fixture(&["audit:read", "users:manage"])
}

async fn create_user(fixture: &TransferFixture) -> String {
    let created = response_json(
        send(
            fixture,
            "POST",
            "/v1/users",
            br#"{"displayName":"Ada","workspace":"fixture"}"#,
            false,
        )
        .await,
    )
    .await;
    created["data"]["user"]["id"]
        .as_str()
        .expect("user id")
        .to_owned()
}

#[tokio::test]
async fn group_routes_cover_the_complete_management_lifecycle() {
    let fixture = fixture();
    let (user_id, group_id) = create_and_rename_group(&fixture).await;
    exercise_membership(&fixture, &group_id, &user_id).await;
    deactivate_and_assert_audit(&fixture, &group_id).await;
}

async fn create_and_rename_group(fixture: &TransferFixture) -> (String, String) {
    let user_id = create_user(fixture).await;
    let group_id = create_group(fixture).await;
    list_and_rename_group(fixture, &group_id).await;
    (user_id, group_id)
}

async fn create_group(fixture: &TransferFixture) -> String {
    let created = response_json(
        send(
            fixture,
            "POST",
            "/v1/groups",
            "{ \"workspace\":\"fixture\", \"name\":\"\\u2003e\\u0301quipe\\u2003\" }".as_bytes(),
            false,
        )
        .await,
    )
    .await;
    let group_id = created["data"]["group"]["id"]
        .as_str()
        .expect("group id")
        .to_owned();
    assert!(group_id.starts_with("group_"));
    assert_eq!(created["data"]["group"]["name"], "équipe");
    assert_eq!(created["data"]["group"]["memberCount"], 0);
    group_id
}

async fn list_and_rename_group(fixture: &TransferFixture, group_id: &str) {
    let listed =
        response_json(send(fixture, "GET", "/v1/groups?workspace=fixture", b"", false).await).await;
    assert_eq!(listed["data"]["items"][0]["id"], group_id);
    assert_eq!(listed["data"]["nextCursor"], serde_json::Value::Null);

    let renamed = response_json(
        send(
            fixture,
            "POST",
            "/v1/groups/rename",
            format!(r#"{{"groupId":"{group_id}","name":"Reviewers"}}"#).as_bytes(),
            false,
        )
        .await,
    )
    .await;
    assert_eq!(renamed["data"]["group"]["name"], "Reviewers");
}

async fn exercise_membership(fixture: &TransferFixture, group_id: &str, user_id: &str) {
    let member_body = format!(r#"{{"groupId":"{group_id}","userId":"{user_id}"}}"#);
    assert_eq!(
        send(
            fixture,
            "POST",
            "/v1/groups/members",
            member_body.as_bytes(),
            false,
        )
        .await
        .status(),
        StatusCode::OK
    );
    let members = response_json(
        send(
            fixture,
            "GET",
            &format!("/v1/groups/members?groupId={group_id}"),
            b"",
            false,
        )
        .await,
    )
    .await;
    assert_eq!(members["data"]["items"], serde_json::json!([user_id]));

    assert_eq!(
        send(
            fixture,
            "POST",
            "/v1/groups/members/remove",
            member_body.as_bytes(),
            false,
        )
        .await
        .status(),
        StatusCode::OK
    );
}

async fn deactivate_and_assert_audit(fixture: &TransferFixture, group_id: &str) {
    assert_eq!(
        send(
            fixture,
            "POST",
            "/v1/groups/deactivate",
            format!(r#"{{"groupId":"{group_id}"}}"#).as_bytes(),
            false,
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_error(
        send(
            fixture,
            "POST",
            "/v1/groups/deactivate",
            format!(r#"{{"groupId":"{group_id}"}}"#).as_bytes(),
            false,
        )
        .await,
        StatusCode::CONFLICT,
        "CONFLICT",
    )
    .await;
    let listed =
        response_json(send(fixture, "GET", "/v1/groups?workspace=fixture", b"", false).await).await;
    assert_eq!(listed["data"]["items"][0]["status"], "deactivated");
    assert_eq!(listed["data"]["items"][0]["memberCount"], 0);
    assert!(listed["data"]["items"][0]["deactivatedAt"].is_string());
    let audit = fixture
        .state
        .repository
        .list_audit("workspace_fixture", None, 20)
        .expect("audit");
    let actions = audit
        .items
        .iter()
        .map(|event| event.action.as_str())
        .collect::<Vec<_>>();
    for expected in [
        "group.created",
        "group.renamed",
        "group.member_added",
        "group.member_removed",
        "group.deactivated",
    ] {
        assert!(actions.contains(&expected), "missing {expected}");
    }
}

#[tokio::test]
async fn group_routes_reject_missing_scope_bad_shapes_and_missing_targets() {
    let forbidden = test_seams::fixture(&["workspace:read"]);
    assert_error(
        send(
            &forbidden,
            "GET",
            "/v1/groups?workspace=fixture",
            b"",
            false,
        )
        .await,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
    )
    .await;
    let fixture = fixture();
    for (method, path, body, status, code) in [
        (
            "POST",
            "/v1/groups",
            br#"{"workspace":"fixture","name":"x"}"#.as_slice(),
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        ),
        (
            "GET",
            "/v1/groups?workspace=fixture&cursor=bad!",
            b"".as_slice(),
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        ),
        (
            "POST",
            "/v1/groups/rename",
            br#"{"groupId":"group_00000000000000000000000000000000","name":"Valid"}"#.as_slice(),
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        ),
    ] {
        assert_error(
            send(&fixture, method, path, body, false).await,
            status,
            code,
        )
        .await;
    }
}
