use super::{Corruption, FaultingRepository};
use blobyard_contract::{
    NewAuditEvent, NewYardAccessGrant, NewYardContinuation, NewYardGuestInvite, RepositoryError,
    YardGuestAcceptance, YardGuestInviteCursor, YardGuestInvitePage, YardGuestInviteRecord,
    YardGuestLoginKeyRecord, YardGuestRepository, YardSubjectRecord,
};

impl YardGuestRepository for FaultingRepository {
    fn list_yard_guest_invites(
        &self,
        yard_id: &str,
        cursor: Option<&YardGuestInviteCursor>,
        limit: usize,
    ) -> Result<YardGuestInvitePage, RepositoryError> {
        self.check()?;
        self.inner
            .list_yard_guest_invites(yard_id, cursor, limit)
            .map(|mut page| {
                if let Some(invitation) = page.items.first_mut() {
                    self.corrupt_guest_invitation(invitation);
                }
                page
            })
    }

    fn yard_guest_invite_by_id(
        &self,
        invitation_id: &str,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        self.check()?;
        self.inner.yard_guest_invite_by_id(invitation_id)
    }

    fn create_yard_guest_invite(
        &self,
        invitation: &NewYardGuestInvite,
        grant: &NewYardAccessGrant,
        event: &NewAuditEvent,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        self.check()?;
        self.inner
            .create_yard_guest_invite(invitation, grant, event)
            .map(|mut invitation| {
                self.corrupt_guest_invitation(&mut invitation);
                invitation
            })
    }

    fn pending_yard_guest_invite_by_token(
        &self,
        token_hash: &str,
        now_ms: u64,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        self.check()?;
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
        self.check()?;
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
        self.check()?;
        self.inner
            .revoke_yard_guest_invite(yard_id, invitation_id, now_ms, event)
            .map(|mut invitation| {
                self.corrupt_guest_invitation(&mut invitation);
                invitation
            })
    }

    fn authenticate_yard_guest_key(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<YardSubjectRecord, RepositoryError> {
        self.check()?;
        self.inner.authenticate_yard_guest_key(secret_hash, now_ms)
    }
}

impl FaultingRepository {
    fn corrupt_guest_invitation(&self, invitation: &mut YardGuestInviteRecord) {
        if matches!(self.corruption, Some(Corruption::YardGuestInviteTimestamp)) {
            invitation.created_at_ms = u64::MAX;
        }
    }
}
