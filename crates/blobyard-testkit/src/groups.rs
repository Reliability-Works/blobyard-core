use crate::{FixtureExecutionTracker, ensure_equal, local_user_event};
use blobyard_contract::{
    AuditValue, LifecycleRepository, LocalUserRepository, NewAuditEvent, RepositoryError,
    WorkspaceGroupMemberRecord, WorkspaceGroupRecord, WorkspaceGroupRepository,
    WorkspaceGroupStatus,
};

#[path = "group_audits.rs"]
mod audits;
#[path = "group_failures.rs"]
mod failures;
use audits::{assert_only_new_audit, audit_state};

/// Combined repository surface required by portable group conformance.
pub trait GroupConformanceRepository:
    WorkspaceGroupRepository + LocalUserRepository + LifecycleRepository
{
}

impl<T> GroupConformanceRepository for T where
    T: WorkspaceGroupRepository + LocalUserRepository + LifecycleRepository
{
}

/// Runs deterministic workspace-group lifecycle and membership checks.
///
/// # Errors
///
/// Returns the first contract failure reported by the adapter.
pub fn group_conformance(
    repository: &dyn GroupConformanceRepository,
    workspace_id: &str,
) -> Result<(), RepositoryError> {
    let group = WorkspaceGroupRecord {
        id: "group_00000000000000000000000000000001".to_owned(),
        workspace_id: workspace_id.to_owned(),
        name: "Reviewers".to_owned(),
        status: WorkspaceGroupStatus::Active,
        member_count: 0,
        created_at_ms: 50,
        deactivated_at_ms: None,
    };
    let mut tracker = FixtureExecutionTracker::new("testkit", "groups");
    let renamed = create_and_rename(repository, &group, &mut tracker)?;
    membership_conformance(repository, &group, &renamed, &mut tracker)?;
    deactivate_conformance(repository, &renamed, &mut tracker)?;
    tracker.finish()
}

fn create_and_rename(
    repository: &dyn GroupConformanceRepository,
    group: &WorkspaceGroupRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<WorkspaceGroupRecord, RepositoryError> {
    let created_event = group_event(
        "audit_group_created",
        "group.created",
        group,
        50,
        [("name", AuditValue::String(group.name.clone()))],
    );
    let audit_before = audit_state(repository, &group.workspace_id)?;
    repository.create_workspace_group(group, &created_event)?;
    let page = repository.list_workspace_groups(&group.workspace_id, None, 50)?;
    ensure_equal(&page.items, &vec![group.clone()])?;
    assert_only_new_audit(repository, &audit_before, &created_event)?;
    tracker.record_case(
        "group-create-emits-exact-audit",
        &serde_json::json!({"mutation": "group-create"}),
        &serde_json::json!({
            "eventType": "group.created",
            "targetType": "workspace_group",
            "metadataKeys": ["groupId", "workspaceId", "name"],
            "eventCount": 1
        }),
    );
    let renamed_event = group_event(
        "audit_group_renamed",
        "group.renamed",
        group,
        51,
        [("to", AuditValue::String("Approvers".to_owned()))],
    );
    let audit_before = audit_state(repository, &group.workspace_id)?;
    let renamed = repository.rename_workspace_group(
        &group.workspace_id,
        &group.id,
        "Approvers",
        &renamed_event,
    )?;
    let mut persisted_rename = renamed_event;
    persisted_rename
        .metadata
        .push(("from".to_owned(), AuditValue::String(group.name.clone())));
    persisted_rename
        .metadata
        .sort_by(|left, right| left.0.cmp(&right.0));
    assert_only_new_audit(repository, &audit_before, &persisted_rename)?;
    tracker.record_case(
        "group-rename-emits-exact-audit",
        &serde_json::json!({"mutation": "group-rename"}),
        &serde_json::json!({
            "eventType": "group.renamed",
            "targetType": "workspace_group",
            "metadataKeys": ["groupId", "workspaceId", "from", "to"],
            "eventCount": 1
        }),
    );
    Ok(renamed)
}

fn membership_conformance(
    repository: &dyn GroupConformanceRepository,
    group: &WorkspaceGroupRecord,
    renamed: &WorkspaceGroupRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let member = WorkspaceGroupMemberRecord {
        group_id: group.id.clone(),
        workspace_id: group.workspace_id.clone(),
        user_id: "user_first".to_owned(),
        added_at_ms: 52,
    };
    let added_event = group_event(
        "audit_group_member",
        "group.member_added",
        renamed,
        52,
        [("userId", AuditValue::String(member.user_id.clone()))],
    );
    let audit_before = audit_state(repository, &group.workspace_id)?;
    repository.add_workspace_group_member(&member, &added_event)?;
    assert_only_new_audit(repository, &audit_before, &added_event)?;
    tracker.record_case(
        "group-member-add-emits-exact-audit",
        &serde_json::json!({"mutation": "group-member-add"}),
        &serde_json::json!({
            "eventType": "group.member_added",
            "targetType": "workspace_group",
            "metadataKeys": ["groupId", "workspaceId", "userId"],
            "eventCount": 1
        }),
    );
    ensure_equal(
        &repository
            .list_workspace_group_members(&group.workspace_id, &group.id, None, 50)?
            .items,
        &vec![member.clone()],
    )?;
    failures::failed_mutation_conformance(repository, renamed, &member, tracker)?;
    remove_and_readd(repository, renamed, member, tracker)?;
    verify_user_deactivation(repository, group)
}

fn remove_and_readd(
    repository: &dyn GroupConformanceRepository,
    renamed: &WorkspaceGroupRecord,
    member: WorkspaceGroupMemberRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let removed_event = group_event(
        "audit_group_removed",
        "group.member_removed",
        renamed,
        53,
        [("userId", AuditValue::String(member.user_id.clone()))],
    );
    let audit_before = audit_state(repository, &member.workspace_id)?;
    repository.remove_workspace_group_member(
        &member.workspace_id,
        &member.group_id,
        &member.user_id,
        &removed_event,
    )?;
    assert_only_new_audit(repository, &audit_before, &removed_event)?;
    tracker.record_case(
        "group-member-remove-emits-exact-audit",
        &serde_json::json!({"mutation": "group-member-remove"}),
        &serde_json::json!({
            "eventType": "group.member_removed",
            "targetType": "workspace_group",
            "metadataKeys": ["groupId", "workspaceId", "userId"],
            "eventCount": 1
        }),
    );
    let readded_event = group_event(
        "audit_group_readded",
        "group.member_added",
        renamed,
        54,
        [("userId", AuditValue::String("user_first".to_owned()))],
    );
    let audit_before = audit_state(repository, &member.workspace_id)?;
    repository.add_workspace_group_member(
        &WorkspaceGroupMemberRecord {
            added_at_ms: 54,
            ..member
        },
        &readded_event,
    )?;
    assert_only_new_audit(repository, &audit_before, &readded_event)
}

fn deactivate_conformance(
    repository: &dyn GroupConformanceRepository,
    group: &WorkspaceGroupRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let event = group_event(
        "audit_group_deactivated",
        "group.deactivated",
        group,
        56,
        [],
    );
    let audit_before = audit_state(repository, &group.workspace_id)?;
    repository.deactivate_workspace_group(&group.workspace_id, &group.id, 56, &event)?;
    let mut persisted = event;
    persisted
        .metadata
        .push(("revokedGrantCount".to_owned(), AuditValue::Number(0)));
    persisted
        .metadata
        .sort_by(|left, right| left.0.cmp(&right.0));
    assert_only_new_audit(repository, &audit_before, &persisted)?;
    tracker.record_case(
        "group-deactivate-emits-exact-audit",
        &serde_json::json!({"mutation": "group-deactivate"}),
        &serde_json::json!({
            "eventType": "group.deactivated",
            "targetType": "workspace_group",
            "metadataKeys": ["groupId", "workspaceId", "revokedGrantCount"],
            "eventCount": 1
        }),
    );
    Ok(())
}

fn verify_user_deactivation(
    repository: &dyn GroupConformanceRepository,
    group: &WorkspaceGroupRecord,
) -> Result<(), RepositoryError> {
    let user = repository
        .list_local_users(&group.workspace_id)?
        .into_iter()
        .find(|listing| listing.user.id == "user_first")
        .ok_or(RepositoryError::Unavailable)?
        .user;
    let event = local_user_event(
        "audit_group_user_deactivated",
        &user,
        "user.deactivated",
        55,
    );
    let audit_before = audit_state(repository, &group.workspace_id)?;
    repository.deactivate_local_user(&user.id, 55, &event)?;
    assert_only_new_audit(repository, &audit_before, &event)?;
    let final_group = repository
        .list_workspace_groups(&group.workspace_id, None, 50)?
        .items
        .into_iter()
        .find(|item| item.id == group.id)
        .ok_or(RepositoryError::Unavailable)?;
    if final_group.member_count == 0 {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

/// Builds an exact group audit event using Core's portable target convention.
#[must_use]
pub fn group_event<const N: usize>(
    id: &str,
    action: &str,
    group: &WorkspaceGroupRecord,
    created_at_ms: u64,
    metadata: [(&str, AuditValue); N],
) -> NewAuditEvent {
    let mut values = vec![
        ("groupId".to_owned(), AuditValue::String(group.id.clone())),
        (
            "workspaceId".to_owned(),
            AuditValue::String(group.workspace_id.clone()),
        ),
    ];
    values.extend(
        metadata
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value)),
    );
    values.sort_by(|left, right| left.0.cmp(&right.0));
    NewAuditEvent {
        id: id.to_owned(),
        workspace_id: group.workspace_id.clone(),
        actor: "token_fixture".to_owned(),
        action: action.to_owned(),
        request_id: format!("request_{id}"),
        target_type: "workspace_group".to_owned(),
        metadata: values,
        created_at_ms,
    }
}
