use crate::{ensure_equal, local_user_event};
use blobyard_contract::{
    AuditValue, LifecycleRepository, LocalUserRepository, NewAuditEvent, RepositoryError,
    WorkspaceGroupMemberRecord, WorkspaceGroupRecord, WorkspaceGroupRepository,
    WorkspaceGroupStatus,
};

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
    let renamed = create_and_rename(repository, &group)?;
    membership_conformance(repository, &group, &renamed)
}

fn create_and_rename(
    repository: &dyn GroupConformanceRepository,
    group: &WorkspaceGroupRecord,
) -> Result<WorkspaceGroupRecord, RepositoryError> {
    repository.create_workspace_group(
        group,
        &group_event(
            "audit_group_created",
            "group.created",
            group,
            50,
            [("name", AuditValue::String(group.name.clone()))],
        ),
    )?;
    let page = repository.list_workspace_groups(&group.workspace_id, None, 50)?;
    ensure_equal(&page.items, &vec![group.clone()])?;
    repository.rename_workspace_group(
        &group.workspace_id,
        &group.id,
        "Approvers",
        &group_event(
            "audit_group_renamed",
            "group.renamed",
            group,
            51,
            [("to", AuditValue::String("Approvers".to_owned()))],
        ),
    )
}

fn membership_conformance(
    repository: &dyn GroupConformanceRepository,
    group: &WorkspaceGroupRecord,
    renamed: &WorkspaceGroupRecord,
) -> Result<(), RepositoryError> {
    let member = WorkspaceGroupMemberRecord {
        group_id: group.id.clone(),
        workspace_id: group.workspace_id.clone(),
        user_id: "user_first".to_owned(),
        added_at_ms: 52,
    };
    repository.add_workspace_group_member(
        &member,
        &group_event(
            "audit_group_member",
            "group.member_added",
            renamed,
            52,
            [("userId", AuditValue::String(member.user_id.clone()))],
        ),
    )?;
    if repository.add_workspace_group_member(
        &member,
        &group_event(
            "audit_group_duplicate",
            "group.member_added",
            renamed,
            52,
            [("userId", AuditValue::String(member.user_id.clone()))],
        ),
    ) != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    ensure_equal(
        &repository
            .list_workspace_group_members(&group.workspace_id, &group.id, None, 50)?
            .items,
        &vec![member.clone()],
    )?;
    remove_and_readd(repository, renamed, member)?;
    verify_user_deactivation(repository, group)
}

fn remove_and_readd(
    repository: &dyn GroupConformanceRepository,
    renamed: &WorkspaceGroupRecord,
    member: WorkspaceGroupMemberRecord,
) -> Result<(), RepositoryError> {
    repository.remove_workspace_group_member(
        &member.workspace_id,
        &member.group_id,
        &member.user_id,
        &group_event(
            "audit_group_removed",
            "group.member_removed",
            renamed,
            53,
            [("userId", AuditValue::String(member.user_id.clone()))],
        ),
    )?;
    repository.add_workspace_group_member(
        &WorkspaceGroupMemberRecord {
            added_at_ms: 54,
            ..member
        },
        &group_event(
            "audit_group_readded",
            "group.member_added",
            renamed,
            54,
            [("userId", AuditValue::String("user_first".to_owned()))],
        ),
    )?;
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
    repository.deactivate_local_user(
        &user.id,
        55,
        &local_user_event(
            "audit_group_user_deactivated",
            &user,
            "user.deactivated",
            55,
        ),
    )?;
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
