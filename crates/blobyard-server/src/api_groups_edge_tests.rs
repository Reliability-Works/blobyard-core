#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::operations;
use crate::auth::Principal;
use crate::contract_test_support::{assert_error, send, send_as};
use crate::repository_fault_tests::FaultingRepository;
use crate::transfers::test_seams;
use axum::http::StatusCode;
use blobyard_api_client::{
    CreateGroupRequest, DeactivateGroupRequest, GroupMemberRequest, RenameGroupRequest,
};
use blobyard_contract::{WorkspaceGroupRecord, WorkspaceGroupStatus};
use blobyard_core::Slug;
use std::sync::Arc;

use super::tests::fixture;

const GROUP_ID: &str = "group_00000000000000000000000000000001";

#[test]
fn group_management_rejects_machine_principals_before_scope_evaluation() {
    let fixture = test_seams::fixture(&["users:manage"]);
    let mut machine = Principal(fixture.principal);
    machine.0.id = "machine_fixture".to_owned();
    assert_eq!(
        crate::test_support::error_status(crate::api_local_users::require_users_manage(&machine)),
        StatusCode::FORBIDDEN
    );
}

pub(super) fn route_shapes() -> [(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [u8],
); 7] {
    [
        (
            "machine-cannot-list-workspace-groups",
            "listGroups",
            "GET",
            "/v1/groups?workspace=fixture",
            b"",
        ),
        (
            "machine-cannot-create-workspace-group",
            "createGroup",
            "POST",
            "/v1/groups",
            br#"{"workspace":"fixture","name":"Reviewers"}"#,
        ),
        (
            "machine-cannot-rename-workspace-group",
            "renameGroup",
            "POST",
            "/v1/groups/rename",
            br#"{"groupId":"group_00000000000000000000000000000001","name":"Approvers"}"#,
        ),
        (
            "machine-cannot-list-workspace-group-members",
            "listGroupMembers",
            "GET",
            "/v1/groups/members?groupId=group_00000000000000000000000000000001",
            b"",
        ),
        (
            "machine-cannot-add-workspace-group-member",
            "addGroupMember",
            "POST",
            "/v1/groups/members",
            br#"{"groupId":"group_00000000000000000000000000000001","userId":"user_fixture"}"#,
        ),
        (
            "machine-cannot-remove-workspace-group-member",
            "removeGroupMember",
            "POST",
            "/v1/groups/members/remove",
            br#"{"groupId":"group_00000000000000000000000000000001","userId":"user_fixture"}"#,
        ),
        (
            "machine-cannot-deactivate-workspace-group",
            "deactivateGroup",
            "POST",
            "/v1/groups/deactivate",
            br#"{"groupId":"group_00000000000000000000000000000001"}"#,
        ),
    ]
}

#[tokio::test]
async fn every_group_route_rejects_missing_scope_and_malformed_inputs() {
    let forbidden = test_seams::fixture(&["object:read"]);
    for (_fixture_id, _operation_id, method, path, body) in route_shapes() {
        assert_error(
            send(&forbidden, method, path, body, false).await,
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        )
        .await;
    }
    blobyard_testkit::assert_group_authorization_fixture_case(
        "object-reader-cannot-manage-workspace-groups",
        &serde_json::json!({
            "action": "users:manage",
            "expected": {"allowed": false, "code": "FORBIDDEN"},
            "id": "object-reader-cannot-manage-workspace-groups",
            "principalScopes": ["object:read"],
            "resource": {"project": "demo", "workspace": "default"}
        }),
    )
    .expect("object reader authorization vector");
    let fixture = fixture();
    for (method, path) in [
        ("GET", "/v1/groups?workspace=%"),
        ("POST", "/v1/groups"),
        ("POST", "/v1/groups/rename"),
        ("GET", "/v1/groups/members"),
        ("POST", "/v1/groups/members"),
        ("POST", "/v1/groups/members/remove"),
        ("POST", "/v1/groups/deactivate"),
    ] {
        assert_error(
            send(&fixture, method, path, b"{", false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
    for path in [
        "/v1/groups?workspace=fixture&cursor=bad!",
        "/v1/groups/members?groupId=group_00000000000000000000000000000001&cursor=bad!",
    ] {
        assert_error(
            send(&fixture, "GET", path, b"", false).await,
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
        )
        .await;
    }
}

#[tokio::test]
async fn group_routes_map_each_repository_failure() {
    for (index, (_fixture_id, _operation_id, method, path, body)) in
        route_shapes().into_iter().enumerate()
    {
        let fixture = fixture();
        let mut state = fixture.state.clone();
        let failure_index = if index < 2 { 2 } else { 1 };
        state.repository = Arc::new(FaultingRepository::new(
            Arc::clone(&state.repository),
            failure_index,
        ));
        assert_error(
            send_as(
                test_seams::fixture_router(&state),
                "secret",
                method,
                path,
                body,
            )
            .await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
        )
        .await;
    }
}

#[tokio::test]
async fn group_routes_conceal_missing_workspaces_and_reject_invalid_names() {
    let fixture = fixture();
    for (method, path, body) in [
        ("GET", "/v1/groups?workspace=missing", b"".as_slice()),
        (
            "POST",
            "/v1/groups",
            br#"{"workspace":"missing","name":"Reviewers"}"#,
        ),
        (
            "POST",
            "/v1/groups/rename",
            br#"{"groupId":"group_00000000000000000000000000000001","name":"x"}"#,
        ),
    ] {
        assert_error(
            send(&fixture, method, path, body, false).await,
            if method == "POST" && path.ends_with("rename") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::NOT_FOUND
            },
            if method == "POST" && path.ends_with("rename") {
                "INVALID_REQUEST"
            } else {
                "NOT_FOUND"
            },
        )
        .await;
    }
    blobyard_testkit::assert_group_authorization_fixture_case(
        "cross-workspace-groups-are-concealed",
        &serde_json::json!({
            "action": "users:manage",
            "expected": {"allowed": false, "code": "NOT_FOUND"},
            "id": "cross-workspace-groups-are-concealed",
            "principalScopes": ["users:manage"],
            "resource": {"project": "demo", "workspace": "other"}
        }),
    )
    .expect("cross-workspace authorization vector");
}

#[test]
fn group_operations_propagate_clock_and_timestamp_rendering_failures() {
    let fixture = fixture();
    let principal = Principal(fixture.principal.clone());
    let clock = || Err(crate::error::ApiError::internal());
    let create = CreateGroupRequest {
        workspace: Slug::new("fixture".to_owned()).expect("slug"),
        name: "Reviewers".to_owned(),
    };
    let rename = RenameGroupRequest {
        group_id: GROUP_ID.to_owned(),
        name: "Approvers".to_owned(),
    };
    let member = GroupMemberRequest {
        group_id: GROUP_ID.to_owned(),
        user_id: "user_fixture".to_owned(),
    };
    let deactivate = DeactivateGroupRequest {
        group_id: GROUP_ID.to_owned(),
    };
    assert!(operations::create(&fixture.state, &principal, &create, clock()).is_err());
    assert!(operations::rename(&fixture.state, &principal, &rename, clock()).is_err());
    assert!(operations::add_member(&fixture.state, &principal, &member, clock()).is_err());
    assert!(operations::remove_member(&fixture.state, &principal, &member, clock()).is_err());
    assert!(operations::deactivate(&fixture.state, &principal, &deactivate, clock()).is_err());

    let malformed = group(u64::MAX, None);
    assert!(operations::summaries(vec![malformed.clone()]).is_err());
    assert!(operations::response(malformed).is_err());
    assert!(
        operations::list_response(
            "workspace_fixture",
            blobyard_contract::WorkspaceGroupPage {
                items: vec![group(u64::MAX, None)],
                next_cursor: None,
            },
        )
        .is_err()
    );
    assert!(operations::summaries(vec![group(1, Some(u64::MAX))]).is_err());
}

fn group(created_at_ms: u64, deactivated_at_ms: Option<u64>) -> WorkspaceGroupRecord {
    WorkspaceGroupRecord {
        id: GROUP_ID.to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Reviewers".to_owned(),
        status: WorkspaceGroupStatus::Deactivated,
        member_count: 0,
        created_at_ms,
        deactivated_at_ms,
    }
}
