use super::{
    SqliteRepository, yard_guest_accept, yard_guest_create, yard_guest_keys, yard_guest_queries,
    yard_guest_revoke,
};
use blobyard_contract::{
    LocalUserRecord, NewAuditEvent, NewYardAccessGrant, NewYardContinuation, NewYardGuestInvite,
    RepositoryError, YardGuestAcceptance, YardGuestInviteCursor, YardGuestInvitePage,
    YardGuestInviteRecord, YardGuestLoginKeyRecord, YardGuestRepository, YardSubjectRecord,
};
use rusqlite::{Transaction, params};

impl YardGuestRepository for SqliteRepository {
    fn list_yard_guest_invites(
        &self,
        yard_id: &str,
        cursor: Option<&YardGuestInviteCursor>,
        limit: usize,
    ) -> Result<YardGuestInvitePage, RepositoryError> {
        let connection = self.connection()?;
        yard_guest_queries::list(&connection, yard_id, cursor, limit)
    }

    fn create_yard_guest_invite(
        &self,
        invitation: &NewYardGuestInvite,
        grant: &NewYardAccessGrant,
        event: &NewAuditEvent,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        self.write_transaction(|transaction| {
            yard_guest_create::create(transaction, invitation, grant, event)
        })
    }

    fn yard_guest_invite_by_id(
        &self,
        invitation_id: &str,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        super::rows::validate_text(invitation_id)?;
        let connection = self.connection()?;
        yard_guest_queries::by_id(&connection, invitation_id)?.ok_or(RepositoryError::NotFound)
    }

    fn pending_yard_guest_invite_by_token(
        &self,
        token_hash: &str,
        now_ms: u64,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        let now = super::auth_validation::sql_time(now_ms)?;
        let connection = self.connection()?;
        yard_guest_queries::pending_by_hash(&connection, token_hash, now)
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
        self.write_transaction(|transaction| {
            yard_guest_accept::accept(
                transaction,
                token_hash,
                subject,
                key,
                continuation,
                event,
                now_ms,
            )
        })
    }

    fn revoke_yard_guest_invite(
        &self,
        yard_id: &str,
        invitation_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<YardGuestInviteRecord, RepositoryError> {
        self.write_transaction(|transaction| {
            yard_guest_revoke::revoke(transaction, yard_id, invitation_id, now_ms, event)
        })
    }

    fn authenticate_yard_guest_key(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<YardSubjectRecord, RepositoryError> {
        self.write_transaction(|transaction| {
            yard_guest_keys::authenticate(transaction, secret_hash, now_ms)
        })
    }
}

pub(super) fn insert_member_subject(
    transaction: &Transaction<'_>,
    user: &LocalUserRecord,
    created_at_ms: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO yard_subjects
             (id, kind, workspace_id, local_user_id, invitation_id, created_at_ms, revoked_at_ms)
             VALUES (?1, 'member', ?2, ?1, NULL, ?3, NULL)",
            params![user.id, user.workspace_id, created_at_ms],
        )
        .map(|_changed| ())
        .map_err(super::map_error)
}

pub(super) fn revoke_member_subject(
    transaction: &Transaction<'_>,
    user_id: &str,
    now: i64,
) -> Result<(), RepositoryError> {
    let changed = transaction
        .execute(
            "UPDATE yard_subjects SET revoked_at_ms = ?2
             WHERE id = ?1 AND kind = 'member' AND revoked_at_ms IS NULL",
            params![user_id, now],
        )
        .map_err(super::map_error)?;
    super::changed_once(changed)
}

#[cfg(test)]
#[path = "yard_guest_invites_creation_validation_tests.rs"]
mod creation_validation_tests;
#[cfg(test)]
#[path = "yard_guest_invites_edge_tests.rs"]
mod edge_tests;
#[cfg(test)]
#[path = "yard_guest_invites_edge_validation_tests.rs"]
mod edge_validation_tests;
#[cfg(test)]
#[path = "yard_guest_identity_tests.rs"]
mod identity_tests;
#[cfg(test)]
#[path = "yard_guest_invites_tests.rs"]
pub(super) mod tests;
