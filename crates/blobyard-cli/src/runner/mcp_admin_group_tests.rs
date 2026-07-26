#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use crate::runner::login::tests::support::Fixture;

fn scope() -> Scope {
    Scope::default()
}

fn fixture() -> Fixture {
    Fixture::new(
        &[
            "blobyard",
            "--workspace",
            "main",
            "--project",
            "artifacts",
            "whoami",
        ],
        vec![],
    )
}

fn calls() -> Vec<(AdminToolCall, Endpoint)> {
    vec![
        (
            AdminToolCall::Group(GroupToolCall::List {
                scope: scope(),
                cursor: Some("group-next".to_owned()),
            }),
            Endpoint::ListGroups,
        ),
        (
            AdminToolCall::Group(GroupToolCall::ListMembers {
                scope: scope(),
                group_id: "group_1".to_owned(),
                cursor: Some("member-next".to_owned()),
            }),
            Endpoint::ListGroupMembers,
        ),
        (
            AdminToolCall::Group(GroupToolCall::Create {
                scope: scope(),
                name: "Reviewers".to_owned(),
            }),
            Endpoint::CreateGroup,
        ),
        (
            AdminToolCall::Group(GroupToolCall::Rename {
                scope: scope(),
                group_id: "group_1".to_owned(),
                name: "Approvers".to_owned(),
            }),
            Endpoint::RenameGroup,
        ),
        (
            AdminToolCall::Group(GroupToolCall::AddMember {
                scope: scope(),
                group_id: "group_1".to_owned(),
                user_id: "user_1".to_owned(),
            }),
            Endpoint::AddGroupMember,
        ),
        (
            AdminToolCall::Group(GroupToolCall::RemoveMember {
                scope: scope(),
                group_id: "group_1".to_owned(),
                user_id: "user_1".to_owned(),
                confirmed: true,
            }),
            Endpoint::RemoveGroupMember,
        ),
        (
            AdminToolCall::Group(GroupToolCall::Deactivate {
                scope: scope(),
                group_id: "group_1".to_owned(),
                confirmed: true,
            }),
            Endpoint::DeactivateGroup,
        ),
    ]
}

#[test]
fn maps_every_group_call_to_its_versioned_endpoint() {
    let fixture = fixture();
    for (call, endpoint) in calls() {
        assert_eq!(admin_scope(&call), &scope());
        let request = admin_request(&fixture.runner, call).expect("group request");
        assert_eq!(request.endpoint(), endpoint);
    }
}

#[test]
fn builds_exact_group_queries_and_workspace_body() {
    let fixture = fixture();
    let groups = admin_request(
        &fixture.runner,
        AdminToolCall::Group(GroupToolCall::List {
            scope: scope(),
            cursor: Some("next".to_owned()),
        }),
    )
    .expect("group list request");
    assert_eq!(groups.query(), Some("workspace=main&cursor=next"));

    let members = admin_request(
        &fixture.runner,
        AdminToolCall::Group(GroupToolCall::ListMembers {
            scope: scope(),
            group_id: "group_1".to_owned(),
            cursor: Some("member-next".to_owned()),
        }),
    )
    .expect("member list request");
    assert_eq!(members.query(), Some("groupId=group_1&cursor=member-next"));

    let group = admin_request(
        &fixture.runner,
        AdminToolCall::Group(GroupToolCall::Create {
            scope: scope(),
            name: "Reviewers".to_owned(),
        }),
    )
    .expect("group create request");
    assert_eq!(
        group.body().and_then(|body| body["workspace"].as_str()),
        Some("main")
    );
}

#[test]
fn destructive_group_calls_require_confirmation() {
    for call in [
        AdminToolCall::Group(GroupToolCall::RemoveMember {
            scope: scope(),
            group_id: "group_1".to_owned(),
            user_id: "user_1".to_owned(),
            confirmed: false,
        }),
        AdminToolCall::Group(GroupToolCall::Deactivate {
            scope: scope(),
            group_id: "group_1".to_owned(),
            confirmed: false,
        }),
    ] {
        let error = require_admin_confirmation(&call).expect_err("confirmation required");
        assert_eq!(error.code(), ErrorCode::InvalidRequest);
    }
}

#[test]
fn group_request_mapping_covers_missing_scope_and_member_cursor_omission() {
    let unscoped = Fixture::new(&["blobyard", "whoami"], vec![]);
    assert!(
        admin_request(
            &unscoped.runner,
            AdminToolCall::Group(GroupToolCall::List {
                scope: scope(),
                cursor: None,
            }),
        )
        .is_err()
    );
    let fixture = fixture();
    let request = admin_request(
        &fixture.runner,
        AdminToolCall::Group(GroupToolCall::ListMembers {
            scope: scope(),
            group_id: "group_1".to_owned(),
            cursor: None,
        }),
    )
    .expect("member request");
    assert_eq!(request.query(), Some("groupId=group_1"));
}
