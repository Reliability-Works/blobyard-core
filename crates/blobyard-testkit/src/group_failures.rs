use super::{GroupConformanceRepository, group_event};
use crate::{FixtureExecutionTracker, ensure_equal};
use blobyard_contract::{
    AuditValue, RepositoryError, WorkspaceGroupMemberRecord, WorkspaceGroupRecord,
};

pub(super) fn failed_mutation_conformance(
    repository: &dyn GroupConformanceRepository,
    group: &WorkspaceGroupRecord,
    member: &WorkspaceGroupMemberRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let groups_before = repository.list_workspace_groups(&group.workspace_id, None, 50)?;
    let members_before =
        repository.list_workspace_group_members(&group.workspace_id, &group.id, None, 50)?;
    let audit_before = repository.list_audit(&group.workspace_id, None, 100)?;
    let failed_event_ids = attempt_failed_mutations(repository, group, member)?;

    let groups_after = repository.list_workspace_groups(&group.workspace_id, None, 50)?;
    let members_after =
        repository.list_workspace_group_members(&group.workspace_id, &group.id, None, 50)?;
    let audit_after = repository.list_audit(&group.workspace_id, None, 100)?;
    ensure_equal(
        &(&groups_after, &members_after, &audit_after),
        &(&groups_before, &members_before, &audit_before),
    )?;
    if failed_event_ids
        .iter()
        .any(|event_id| audit_after.items.iter().any(|event| event.id == *event_id))
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "failed-group-mutations-emit-no-audit",
        &serde_json::json!({
            "mutationResult": ["invalid", "conflict", "concealed-not-found"]
        }),
        &serde_json::json!({"stateChanged": false, "auditEventCount": 0}),
    );
    Ok(())
}

fn attempt_failed_mutations(
    repository: &dyn GroupConformanceRepository,
    group: &WorkspaceGroupRecord,
    member: &WorkspaceGroupMemberRecord,
) -> Result<[String; 3], RepositoryError> {
    let duplicate_event = group_event(
        "audit_group_duplicate",
        "group.member_added",
        group,
        52,
        [("userId", AuditValue::String(member.user_id.clone()))],
    );
    if repository.add_workspace_group_member(member, &duplicate_event)
        != Err(RepositoryError::Conflict)
    {
        return Err(RepositoryError::Unavailable);
    }
    let mut invalid = group.clone();
    "group_00000000000000000000000000000002".clone_into(&mut invalid.id);
    "x".clone_into(&mut invalid.name);
    let invalid_event = group_event(
        "audit_group_invalid",
        "group.created",
        &invalid,
        52,
        [("name", AuditValue::String(invalid.name.clone()))],
    );
    if repository.create_workspace_group(&invalid, &invalid_event)
        != Err(RepositoryError::InvalidInput)
    {
        return Err(RepositoryError::Unavailable);
    }
    let missing_event = group_event(
        "audit_group_missing",
        "group.renamed",
        group,
        52,
        [("to", AuditValue::String("Missing".to_owned()))],
    );
    if repository.rename_workspace_group(
        &group.workspace_id,
        "group_ffffffffffffffffffffffffffffffff",
        "Missing",
        &missing_event,
    ) != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok([duplicate_event.id, invalid_event.id, missing_event.id])
}
