use super::{
    map_error, yard_application_policy, yard_guest_identity, yard_identity_grants,
    yard_management_roles, yard_session_admission, yard_session_rows,
};
use blobyard_contract::{RepositoryError, YardIdentity};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::BTreeSet;

struct IdentityBase {
    session_id: String,
    user_id: String,
    subject_kind: String,
    invitation_id: Option<String>,
    workspace_id: String,
    project_id: String,
    yard_id: String,
    environment_id: String,
    display_name: Option<String>,
    email: Option<String>,
    visibility: String,
}

pub(super) fn resolve(
    connection: &Connection,
    host_label: &str,
    token_hash: &str,
    now_ms: i64,
) -> Result<YardIdentity, RepositoryError> {
    yard_session_rows::validate_host_label(host_label)?;
    super::auth_validation::validate_hash(token_hash)?;
    let base = base_by_host(connection, host_label, token_hash, now_ms)?;
    let admitted = yard_session_admission::session_id(
        connection,
        token_hash,
        host_label,
        &base.yard_id,
        &base.visibility,
        now_ms,
    )?;
    require_admitted_session(admitted.as_deref(), &base.session_id)?;
    yard_management_roles::validate_integrity(connection, &base.yard_id)?;
    let (management_role, sources) = authority(connection, &base, now_ms)?;
    let (app_roles, permissions) = effective_application_authority(
        yard_application_policy::policy_by_yard(connection, &base.yard_id)?,
        sources.roles,
    );
    touch(connection, &base.session_id, now_ms)?;
    Ok(YardIdentity {
        user_id: base.user_id,
        workspace_id: base.workspace_id,
        project_id: base.project_id,
        yard_id: base.yard_id,
        environment_id: base.environment_id,
        display_name: base.display_name,
        email: base.email,
        groups: sources.groups,
        management_role,
        app_roles,
        permissions,
        session_id: base.session_id,
    })
}

fn require_admitted_session(
    admitted_session_id: Option<&str>,
    resolved_session_id: &str,
) -> Result<(), RepositoryError> {
    if admitted_session_id == Some(resolved_session_id) {
        Ok(())
    } else {
        Err(RepositoryError::NotFound)
    }
}

fn authority(
    connection: &Connection,
    base: &IdentityBase,
    now_ms: i64,
) -> Result<
    (
        Option<blobyard_contract::YardManagementRole>,
        yard_identity_grants::IdentityGrantSources,
    ),
    RepositoryError,
> {
    match (base.subject_kind.as_str(), base.invitation_id.as_deref()) {
        ("member", None) => {
            let role = yard_management_roles::by_user(connection, &base.yard_id, &base.user_id)?
                .map(|assignment| assignment.role);
            let sources = yard_identity_grants::resolve(
                connection,
                &base.yard_id,
                &base.environment_id,
                &base.workspace_id,
                &base.user_id,
                now_ms,
            )?;
            Ok((role, sources))
        }
        ("guest", Some(invitation_id)) => {
            let sources = yard_guest_identity::resolve(
                connection,
                &base.yard_id,
                &base.environment_id,
                &base.workspace_id,
                &base.user_id,
                invitation_id,
                now_ms,
            )?;
            Ok((None, sources))
        }
        _ => Err(RepositoryError::Unavailable),
    }
}

fn base_by_host(
    connection: &Connection,
    host_label: &str,
    token_hash: &str,
    now_ms: i64,
) -> Result<IdentityBase, RepositoryError> {
    connection
        .query_row(
            "SELECT s.id, subject.id, subject.kind, subject.invitation_id,
                    subject.workspace_id, y.project_id, y.id, e.id,
                    COALESCE(u.display_name, invitation.email),
                    COALESCE(u.email, invitation.email), p.visibility
             FROM yard_sessions s
             JOIN yard_subjects subject
               ON subject.id = s.subject_id
              AND subject.revoked_at_ms IS NULL
             LEFT JOIN local_users u
               ON subject.kind = 'member'
              AND u.id = subject.local_user_id
              AND u.workspace_id = subject.workspace_id
              AND u.deactivated_at_ms IS NULL AND u.status = 'active'
             LEFT JOIN yard_guest_invitations invitation
               ON subject.kind = 'guest'
              AND invitation.id = subject.invitation_id
              AND invitation.workspace_id = subject.workspace_id
             JOIN web_yards y
               ON y.id = s.yard_id AND y.status = 'active'
             JOIN yard_environments e
               ON e.id = s.environment_id
              AND e.yard_id = y.id
              AND e.status = 'active'
             JOIN yard_access_policies p
               ON p.yard_id = y.id AND p.visibility != 'public' AND p.visibility != 'owner'
             WHERE s.token_hash = ?1 AND s.host_label = ?2
               AND s.revoked_at_ms IS NULL AND s.expires_at_ms > ?3
               AND (
                 (subject.kind = 'member' AND subject.invitation_id IS NULL AND u.id IS NOT NULL)
                 OR (
                   subject.kind = 'guest' AND subject.local_user_id IS NULL
                   AND invitation.id IS NOT NULL
                 )
               )
               AND (
                 y.host_label = ?2
                 OR EXISTS (
                   SELECT 1 FROM yard_deploys d
                   WHERE d.yard_id = y.id
                     AND d.deployment_host_label = ?2
                     AND d.status IN ('live', 'superseded')
                 )
               )",
            params![token_hash, host_label, now_ms],
            identity_base,
        )
        .optional()
        .map_err(map_error)?
        .ok_or(RepositoryError::NotFound)
}

fn identity_base(row: &Row<'_>) -> rusqlite::Result<IdentityBase> {
    Ok(IdentityBase {
        session_id: row.get(0)?,
        user_id: row.get(1)?,
        subject_kind: row.get(2)?,
        invitation_id: row.get(3)?,
        workspace_id: row.get(4)?,
        project_id: row.get(5)?,
        yard_id: row.get(6)?,
        environment_id: row.get(7)?,
        display_name: row.get(8)?,
        email: row.get(9)?,
        visibility: row.get(10)?,
    })
}

fn effective_application_authority(
    policy: Option<blobyard_contract::YardApplicationPolicyRecord>,
    assigned_roles: Vec<String>,
) -> (Vec<String>, Vec<String>) {
    let Some(policy) = policy else {
        return (Vec::new(), Vec::new());
    };
    let mut assigned = assigned_roles.into_iter().collect::<BTreeSet<_>>();
    if let Some(default) = &policy.policy.default_role {
        assigned.insert(default.clone());
    }
    let mut roles = BTreeSet::new();
    let mut permissions = BTreeSet::new();
    for assigned_role in assigned {
        let Some(expanded_roles) = policy.effective.effective_roles.get(&assigned_role) else {
            continue;
        };
        roles.extend(expanded_roles.iter().cloned());
        if let Some(expanded_permissions) =
            policy.effective.effective_permissions.get(&assigned_role)
        {
            permissions.extend(expanded_permissions.iter().cloned());
        }
    }
    (
        roles.into_iter().collect(),
        permissions.into_iter().collect(),
    )
}

fn touch(connection: &Connection, session_id: &str, now_ms: i64) -> Result<(), RepositoryError> {
    let changed = connection
        .execute(
            "UPDATE yard_sessions SET last_used_at_ms = ?2
             WHERE id = ?1 AND revoked_at_ms IS NULL AND expires_at_ms > ?2",
            params![session_id, now_ms],
        )
        .map_err(map_error)?;
    super::changed_once(changed)
}

#[cfg(test)]
#[path = "yard_identity_resolution_tests.rs"]
mod tests;
