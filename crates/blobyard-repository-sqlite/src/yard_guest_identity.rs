use super::{map_error, rows, yard_access, yard_identity_grants::IdentityGrantSources};
use blobyard_contract::RepositoryError;
use rusqlite::{Connection, params};

pub(super) fn resolve(
    connection: &Connection,
    yard_id: &str,
    environment_id: &str,
    workspace_id: &str,
    subject_id: &str,
    invitation_id: &str,
    now_ms: i64,
) -> Result<IdentityGrantSources, RepositoryError> {
    for value in [
        yard_id,
        environment_id,
        workspace_id,
        subject_id,
        invitation_id,
    ] {
        rows::validate_text(value)?;
    }
    let encoded: String = connection
        .query_row(
            "SELECT grant.app_roles
             FROM yard_subjects subject
             JOIN yard_guest_invitations invitation
               ON invitation.id = subject.invitation_id
              AND invitation.accepted_subject_id = subject.id
              AND invitation.workspace_id = subject.workspace_id
             JOIN yard_access_grants grant
               ON grant.id = invitation.grant_id
              AND grant.yard_id = invitation.yard_id
              AND grant.principal_kind = 'guest-invite'
              AND grant.principal_id = invitation.id
             WHERE subject.id = ?4 AND subject.kind = 'guest'
               AND subject.workspace_id = ?3 AND subject.invitation_id = ?5
               AND subject.local_user_id IS NULL AND subject.revoked_at_ms IS NULL
               AND invitation.yard_id = ?1 AND invitation.status = 'accepted'
               AND invitation.revoked_at_ms IS NULL AND invitation.expires_at_ms > ?6
               AND (invitation.environment_id IS NULL OR invitation.environment_id = ?2)
               AND grant.status = 'active' AND grant.revoked_at_ms IS NULL
               AND grant.expires_at_ms = invitation.expires_at_ms
               AND grant.expires_at_ms > ?6
               AND grant.environment_id IS invitation.environment_id",
            params![
                yard_id,
                environment_id,
                workspace_id,
                subject_id,
                invitation_id,
                now_ms
            ],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    let roles = yard_access::decode_roles(&encoded).ok_or(RepositoryError::Unavailable)?;
    Ok(IdentityGrantSources {
        groups: Vec::new(),
        roles,
    })
}
