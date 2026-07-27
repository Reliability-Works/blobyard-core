use super::yard_validation;
use blobyard_contract::{
    NewAuditEvent, RepositoryError, YardGuestAuditInvitation, yard_guest_audit_metadata,
};

pub(super) fn validate(
    event: &NewAuditEvent,
    action: &str,
    invitation: &impl YardGuestAuditInvitation,
    subject_id: Option<&str>,
    at_ms: u64,
) -> Result<i64, RepositoryError> {
    yard_validation::action_event(
        event,
        action,
        "yard_guest_invite",
        invitation.workspace_id(),
        at_ms,
        yard_guest_audit_metadata(invitation, subject_id),
    )
}
