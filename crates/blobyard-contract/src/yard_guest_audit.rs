use crate::{AuditValue, NewYardGuestInvite, YardGuestInviteRecord};

/// Non-secret invitation fields included in guest lifecycle audit events.
pub trait YardGuestAuditInvitation {
    /// Stable invitation identifier.
    fn invitation_id(&self) -> &str;
    /// Workspace tenant boundary.
    fn workspace_id(&self) -> &str;
    /// Owning project identifier.
    fn project_id(&self) -> &str;
    /// Owning Yard identifier.
    fn yard_id(&self) -> &str;
    /// Optional environment restriction.
    fn environment_id(&self) -> Option<&str>;
    /// Matching access grant identifier.
    fn grant_id(&self) -> &str;
}

/// Builds the canonical non-secret guest invitation audit projection.
#[must_use]
pub fn yard_guest_audit_metadata(
    invitation: &(impl YardGuestAuditInvitation + ?Sized),
    subject_id: Option<&str>,
) -> Vec<(String, AuditValue)> {
    vec![
        (
            "environmentId".to_owned(),
            invitation
                .environment_id()
                .map_or(AuditValue::Null, |id| AuditValue::String(id.to_owned())),
        ),
        (
            "grantId".to_owned(),
            AuditValue::String(invitation.grant_id().to_owned()),
        ),
        (
            "invitationId".to_owned(),
            AuditValue::String(invitation.invitation_id().to_owned()),
        ),
        (
            "projectId".to_owned(),
            AuditValue::String(invitation.project_id().to_owned()),
        ),
        (
            "subjectId".to_owned(),
            subject_id.map_or(AuditValue::Null, |id| AuditValue::String(id.to_owned())),
        ),
        (
            "yardId".to_owned(),
            AuditValue::String(invitation.yard_id().to_owned()),
        ),
    ]
}

macro_rules! invitation {
    ($type:ty) => {
        impl YardGuestAuditInvitation for $type {
            fn invitation_id(&self) -> &str {
                &self.id
            }
            fn workspace_id(&self) -> &str {
                &self.workspace_id
            }
            fn project_id(&self) -> &str {
                &self.project_id
            }
            fn yard_id(&self) -> &str {
                &self.yard_id
            }
            fn environment_id(&self) -> Option<&str> {
                self.environment_id.as_deref()
            }
            fn grant_id(&self) -> &str {
                &self.grant_id
            }
        }
    };
}

invitation!(NewYardGuestInvite);
invitation!(YardGuestInviteRecord);
