use super::{lifecycle_audit, yard_validation};
use blobyard_contract::{AuditValue, NewAuditEvent, RepositoryError, WorkspaceGroupRecord};
use rusqlite::Transaction;

pub(super) fn group_event<const N: usize>(
    event: &NewAuditEvent,
    action: &str,
    group: &WorkspaceGroupRecord,
    at_ms: u64,
    metadata: [(&'static str, AuditValue); N],
) -> Result<i64, RepositoryError> {
    let mut expected = vec![
        ("groupId", AuditValue::String(group.id.clone())),
        (
            "workspaceId",
            AuditValue::String(group.workspace_id.clone()),
        ),
    ];
    expected.extend(metadata);
    yard_validation::action_event(
        event,
        action,
        "workspace_group",
        &group.workspace_id,
        at_ms,
        expected,
    )
}

pub(super) fn validate_rename_event(
    event: &NewAuditEvent,
    group: &WorkspaceGroupRecord,
    name: &str,
) -> Result<(), RepositoryError> {
    yard_validation::action_event(
        event,
        "group.renamed",
        "workspace_group",
        &group.workspace_id,
        event.created_at_ms,
        [
            ("groupId", AuditValue::String(group.id.clone())),
            ("to", AuditValue::String(name.to_owned())),
            (
                "workspaceId",
                AuditValue::String(group.workspace_id.clone()),
            ),
        ],
    )
    .map(|_at| ())
}

pub(super) fn insert_rename_event(
    transaction: &Transaction<'_>,
    event: &NewAuditEvent,
    from: &str,
) -> Result<(), RepositoryError> {
    let mut final_event = event.clone();
    final_event
        .metadata
        .push(("from".to_owned(), AuditValue::String(from.to_owned())));
    lifecycle_audit::insert(transaction, &final_event)
}

pub(super) fn member_event(
    event: &NewAuditEvent,
    action: &str,
    group: &WorkspaceGroupRecord,
    user_id: &str,
    at_ms: u64,
) -> Result<i64, RepositoryError> {
    yard_validation::action_event(
        event,
        action,
        "workspace_group",
        &group.workspace_id,
        at_ms,
        [
            ("groupId", AuditValue::String(group.id.clone())),
            ("userId", AuditValue::String(user_id.to_owned())),
            (
                "workspaceId",
                AuditValue::String(group.workspace_id.clone()),
            ),
        ],
    )
}

pub(super) fn validate_deactivate_event(
    event: &NewAuditEvent,
    group: &WorkspaceGroupRecord,
    now_ms: u64,
) -> Result<i64, RepositoryError> {
    yard_validation::action_event(
        event,
        "group.deactivated",
        "workspace_group",
        &group.workspace_id,
        now_ms,
        [
            ("groupId", AuditValue::String(group.id.clone())),
            (
                "workspaceId",
                AuditValue::String(group.workspace_id.clone()),
            ),
        ],
    )
}

pub(super) fn insert_deactivate_event(
    transaction: &Transaction<'_>,
    event: &NewAuditEvent,
    revoked: u64,
) -> Result<(), RepositoryError> {
    let mut final_event = event.clone();
    final_event
        .metadata
        .push(("revokedGrantCount".to_owned(), AuditValue::Number(revoked)));
    lifecycle_audit::insert(transaction, &final_event)
}
