use super::map_error;
use blobyard_contract::{RepositoryError, YardOidcIdentityRecord};
use rusqlite::{Transaction, params};

pub(super) fn active(
    transaction: &Transaction<'_>,
    identity: &YardOidcIdentityRecord,
    now: i64,
) -> Result<bool, RepositoryError> {
    transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1
               FROM yard_subjects subject
               LEFT JOIN local_users user
                 ON subject.kind = 'member'
                AND user.id = subject.local_user_id
                AND user.workspace_id = subject.workspace_id
               LEFT JOIN yard_guest_invitations invitation
                 ON subject.kind = 'guest'
                AND invitation.id = subject.invitation_id
                AND invitation.workspace_id = subject.workspace_id
                AND invitation.accepted_subject_id = subject.id
               LEFT JOIN yard_access_grants grant
                 ON grant.id = invitation.grant_id
                AND grant.yard_id = invitation.yard_id
                AND grant.principal_kind = 'guest-invite'
                AND grant.principal_id = invitation.id
               WHERE subject.id = ?1
                 AND subject.workspace_id = ?2
                 AND subject.revoked_at_ms IS NULL
                 AND (
                   (
                     subject.kind = 'member'
                     AND user.status = 'active'
                     AND user.deactivated_at_ms IS NULL
                   )
                   OR (
                     subject.kind = 'guest'
                     AND invitation.status = 'accepted'
                     AND invitation.revoked_at_ms IS NULL
                     AND invitation.expires_at_ms > ?3
                     AND grant.status = 'active'
                     AND grant.revoked_at_ms IS NULL
                     AND grant.expires_at_ms = invitation.expires_at_ms
                     AND grant.expires_at_ms > ?3
                   )
                 )
             )",
            params![identity.yard_subject_id, identity.workspace_id, now],
            |row| row.get(0),
        )
        .map_err(map_error)
}
