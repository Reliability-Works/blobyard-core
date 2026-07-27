use super::{Corrupting, Corruption};
use blobyard_contract::{
    NewAuditEvent, NewYardAccessGrant, NewYardContinuation, NewYardGuestInvite, RepositoryError,
    YardGuestAcceptance, YardGuestInviteCursor, YardGuestInvitePage, YardGuestInviteRecord,
    YardGuestInviteStatus, YardGuestLoginKeyRecord, YardGuestRepository, YardSubjectRecord,
};

impl<T: YardGuestRepository> YardGuestRepository for Corrupting<'_, T> {
    fn list_yard_guest_invites(
        &self,
        yard_id: &str,
        cursor: Option<&YardGuestInviteCursor>,
        limit: usize,
    ) -> Result<YardGuestInvitePage, RepositoryError> {
        self.inner.list_yard_guest_invites(yard_id, cursor, limit)
    }

    fn yard_guest_invite_by_id(
        &self,
        invitation_id: &str,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        self.inner.yard_guest_invite_by_id(invitation_id)
    }

    fn create_yard_guest_invite(
        &self,
        invitation: &NewYardGuestInvite,
        grant: &NewYardAccessGrant,
        event: &NewAuditEvent,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        if matches!(self.corruption, Corruption::YardGuestCapacityCreateFailure)
            && invitation.email == "capacity0@example.test"
        {
            return Err(RepositoryError::Unavailable);
        }
        if matches!(
            self.corruption,
            Corruption::YardGuestCapacityOverflowAccepted
        ) && invitation.email == "capacity100@example.test"
        {
            return Ok(YardGuestInviteRecord {
                id: invitation.id.clone(),
                workspace_id: invitation.workspace_id.clone(),
                project_id: invitation.project_id.clone(),
                yard_id: invitation.yard_id.clone(),
                environment_id: invitation.environment_id.clone(),
                email: invitation.email.clone(),
                status: YardGuestInviteStatus::Pending,
                accepted_subject_id: None,
                grant_id: invitation.grant_id.clone(),
                app_roles: grant.app_roles.clone(),
                created_at_ms: invitation.created_at_ms,
                expires_at_ms: invitation.expires_at_ms,
                accepted_at_ms: None,
                revoked_at_ms: None,
            });
        }
        self.inner
            .create_yard_guest_invite(invitation, grant, event)
            .map(|mut record| {
                if matches!(self.corruption, Corruption::YardGuestCreatedRecord) {
                    record.app_roles.clear();
                } else if matches!(self.corruption, Corruption::YardGuestBoundaryScope)
                    && invitation.email.starts_with("boundary")
                {
                    record.environment_id = Some("environment_corrupt".to_owned());
                }
                record
            })
    }

    fn pending_yard_guest_invite_by_token(
        &self,
        token_hash: &str,
        now_ms: u64,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        self.inner
            .pending_yard_guest_invite_by_token(token_hash, now_ms)
    }

    fn accept_yard_guest_invite(
        &self,
        token_hash: &str,
        subject: &YardSubjectRecord,
        key: &YardGuestLoginKeyRecord,
        continuation: &NewYardContinuation,
        event: &NewAuditEvent,
        now_ms: u64,
    ) -> Result<YardGuestAcceptance, RepositoryError> {
        self.inner
            .accept_yard_guest_invite(token_hash, subject, key, continuation, event, now_ms)
    }

    fn revoke_yard_guest_invite(
        &self,
        yard_id: &str,
        invitation_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        self.inner
            .revoke_yard_guest_invite(yard_id, invitation_id, now_ms, event)
    }

    fn authenticate_yard_guest_key(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<YardSubjectRecord, RepositoryError> {
        self.inner.authenticate_yard_guest_key(secret_hash, now_ms)
    }
}
