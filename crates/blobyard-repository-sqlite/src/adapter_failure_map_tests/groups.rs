use super::*;
use blobyard_contract::{
    AuditValue, WorkspaceGroupMemberRecord, WorkspaceGroupRecord, WorkspaceGroupRepository,
    WorkspaceGroupStatus,
};

pub(super) fn assert_poisoned_groups(repository: &SqliteRepository) {
    let group = WorkspaceGroupRecord {
        id: "group_00000000000000000000000000000071".to_owned(),
        workspace_id: "workspace_fixture".to_owned(),
        name: "Reviewers".to_owned(),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 1,
        deactivated_at_ms: None,
    };
    let event = blobyard_testkit::group_event(
        "audit_poisoned_group",
        "group.created",
        &group,
        1,
        [("name", AuditValue::String(group.name.clone()))],
    );
    let member = WorkspaceGroupMemberRecord {
        group_id: group.id.clone(),
        workspace_id: group.workspace_id.clone(),
        user_id: "user_first".to_owned(),
        added_at_ms: 2,
    };
    unavailable(repository.create_workspace_group(&group, &event));
    unavailable(repository.list_workspace_groups(&group.workspace_id, None, 50));
    unavailable(repository.rename_workspace_group(
        &group.workspace_id,
        &group.id,
        "Approvers",
        &event,
    ));
    unavailable(repository.list_workspace_group_members(&group.workspace_id, &group.id, None, 50));
    unavailable(repository.add_workspace_group_member(&member, &event));
    unavailable(repository.remove_workspace_group_member(
        &group.workspace_id,
        &group.id,
        &member.user_id,
        &event,
    ));
    unavailable(repository.deactivate_workspace_group(&group.workspace_id, &group.id, 2, &event));
}
