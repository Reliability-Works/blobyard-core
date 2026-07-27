use super::{map_error, yard_session_rows};
use blobyard_contract::{RepositoryError, YardAdmission};
use rusqlite::{Connection, params};

pub(super) fn evaluate(
    connection: &Connection,
    host_label: &str,
    subject_id: &str,
    now_ms: i64,
) -> Result<YardAdmission, RepositoryError> {
    connection
        .query_row(
            "SELECT y.id, e.id, y.workspace_id
             FROM yard_subjects subject
             JOIN yard_guest_invitations invitation
               ON invitation.id = subject.invitation_id
              AND invitation.workspace_id = subject.workspace_id
              AND invitation.accepted_subject_id = subject.id
             JOIN yard_access_grants grant
               ON grant.id = invitation.grant_id
              AND grant.yard_id = invitation.yard_id
              AND grant.principal_kind = 'guest-invite'
              AND grant.principal_id = invitation.id
             JOIN web_yards y
               ON y.id = invitation.yard_id
              AND y.project_id = invitation.project_id
              AND y.workspace_id = invitation.workspace_id
             JOIN yard_environments e
               ON e.yard_id = y.id AND e.kind = 'production' AND e.status = 'active'
             JOIN yard_access_policies policy
               ON policy.yard_id = y.id
              AND policy.visibility IN ('selected', 'authenticated-link')
             WHERE subject.id = ?2 AND subject.kind = 'guest'
               AND subject.local_user_id IS NULL
               AND subject.revoked_at_ms IS NULL
               AND invitation.status = 'accepted'
               AND invitation.revoked_at_ms IS NULL
               AND invitation.expires_at_ms > ?3
               AND (invitation.environment_id IS NULL OR invitation.environment_id = e.id)
               AND grant.status = 'active' AND grant.revoked_at_ms IS NULL
               AND grant.expires_at_ms = invitation.expires_at_ms
               AND grant.expires_at_ms > ?3
               AND grant.environment_id IS invitation.environment_id
               AND EXISTS (
                 SELECT 1 FROM yard_guest_login_keys guest_key
                 WHERE guest_key.subject_id = subject.id
                   AND guest_key.invitation_id = invitation.id
                   AND guest_key.workspace_id = subject.workspace_id
                   AND guest_key.revoked_at_ms IS NULL
                   AND guest_key.expires_at_ms > ?3
                 LIMIT 1
               )
               AND y.status = 'active'
               AND (
                 y.host_label = ?1
                 OR EXISTS (
                   SELECT 1 FROM yard_deploys deploy
                   WHERE deploy.yard_id = y.id
                     AND deploy.deployment_host_label = ?1
                     AND deploy.status IN ('live', 'superseded')
                 )
               )
             LIMIT 1",
            params![host_label, subject_id, now_ms],
            yard_session_rows::admission,
        )
        .map_err(map_error)
}
