use super::GroupConformanceRepository;
use crate::ensure_equal;
use blobyard_contract::{AuditEventRecord, NewAuditEvent, RepositoryError};

pub(super) fn audit_state(
    repository: &dyn GroupConformanceRepository,
    workspace_id: &str,
) -> Result<Vec<AuditEventRecord>, RepositoryError> {
    Ok(repository.list_audit(workspace_id, None, 100)?.items)
}

pub(super) fn assert_only_new_audit(
    repository: &dyn GroupConformanceRepository,
    before: &[AuditEventRecord],
    expected: &NewAuditEvent,
) -> Result<(), RepositoryError> {
    let mut after = audit_state(repository, &expected.workspace_id)?;
    let matching = after
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            event.id == expected.id
                && event.action == expected.action
                && event.actor == expected.actor
                && event.created_at_ms == expected.created_at_ms
                && event.metadata == expected.metadata
                && event.request_id == expected.request_id
                && event.target_type == expected.target_type
                && event.workspace_id == expected.workspace_id
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if after.len() != before.len() + 1 || matching.len() != 1 {
        return Err(RepositoryError::Unavailable);
    }
    after.remove(matching[0]);
    ensure_equal(&after, &before.to_vec())
}
