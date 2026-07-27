use blobyard_contract::{NewAuditEvent, NewYardGuestInvite, yard_guest_audit_metadata};

pub(in crate::adapter) fn event(
    action: &str,
    invitation: &NewYardGuestInvite,
    subject_id: Option<&str>,
    at_ms: u64,
) -> NewAuditEvent {
    NewAuditEvent {
        id: format!("audit_guest_{action}_{at_ms}"),
        workspace_id: "workspace_guest".to_owned(),
        actor: "operator".to_owned(),
        action: format!("yard.guest_invite.{action}"),
        request_id: format!("request_guest_{action}_{at_ms}"),
        target_type: "yard_guest_invite".to_owned(),
        metadata: yard_guest_audit_metadata(invitation, subject_id),
        created_at_ms: at_ms,
    }
}

pub(in crate::adapter::yard_guest_invites) fn hash(character: char) -> String {
    character.to_string().repeat(64)
}
