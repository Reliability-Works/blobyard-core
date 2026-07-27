use super::{map_error, rows, yard_guest_rows};
use blobyard_contract::{RepositoryError, YardGuestLoginKeyRecord, YardSubjectRecord};
use rusqlite::{Transaction, params};

pub(super) fn validate_new(key: &YardGuestLoginKeyRecord) -> Result<(), RepositoryError> {
    for value in [
        &key.id,
        &key.subject_id,
        &key.invitation_id,
        &key.workspace_id,
        &key.token_prefix,
    ] {
        rows::validate_text(value)?;
    }
    super::auth_validation::validate_hash(&key.secret_hash)?;
    if key.created_at_ms < key.expires_at_ms
        && key.last_used_at_ms.is_none()
        && key.revoked_at_ms.is_none()
    {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn authenticate(
    transaction: &Transaction<'_>,
    secret_hash: &str,
    now_ms: u64,
) -> Result<YardSubjectRecord, RepositoryError> {
    super::auth_validation::validate_hash(secret_hash)?;
    let now = super::auth_validation::sql_time(now_ms)?;
    let subject_id: String = transaction
        .query_row(
            "UPDATE yard_guest_login_keys
             SET last_used_at_ms = CASE
               WHEN last_used_at_ms IS NULL OR last_used_at_ms < ?2 THEN ?2
               ELSE last_used_at_ms END
             WHERE secret_hash = ?1 AND revoked_at_ms IS NULL
               AND created_at_ms <= ?2 AND expires_at_ms > ?2
               AND EXISTS (
                 SELECT 1 FROM yard_subjects subject
                 JOIN yard_guest_invitations invitation
                   ON invitation.id = subject.invitation_id
                  AND invitation.workspace_id = subject.workspace_id
                 JOIN yard_access_grants grant
                   ON grant.id = invitation.grant_id
                  AND grant.yard_id = invitation.yard_id
                 WHERE subject.id = yard_guest_login_keys.subject_id
                   AND subject.kind = 'guest' AND subject.revoked_at_ms IS NULL
                   AND invitation.status = 'accepted'
                   AND invitation.accepted_subject_id = subject.id
                   AND invitation.expires_at_ms > ?2
                   AND grant.principal_kind = 'guest-invite'
                   AND grant.principal_id = invitation.id
                   AND grant.status = 'active' AND grant.revoked_at_ms IS NULL
                   AND grant.expires_at_ms > ?2
               )
             RETURNING subject_id",
            params![secret_hash, now],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    transaction
        .query_row(
            "SELECT id, kind, workspace_id, local_user_id, invitation_id,
                    created_at_ms, revoked_at_ms
             FROM yard_subjects WHERE id = ?1",
            [subject_id],
            yard_guest_rows::subject,
        )
        .map_err(map_error)
}
