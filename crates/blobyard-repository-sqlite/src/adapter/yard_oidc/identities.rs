use super::{
    lifecycle_audit, map_error, rows as yard_oidc_rows, validation as yard_oidc_validation,
    yard_session_store,
};
use blobyard_contract::{
    NewAuditEvent, NewYardOidcAuthentication, RepositoryError, YARD_OIDC_IDENTITY_AUDIT_TARGET,
    YARD_OIDC_IDENTITY_LINKED_ACTION, YardOidcAuditContext, YardOidcIdentityRecord,
    yard_oidc_identity_audit_metadata,
};
use rusqlite::{OptionalExtension, Transaction, params};

pub(super) enum AuthenticationOutcome {
    Authenticated(YardOidcIdentityRecord),
    Denied,
}

struct YardScope {
    yard: String,
    environment: String,
    workspace: String,
}

pub(super) fn authenticate(
    transaction: &Transaction<'_>,
    authentication: &NewYardOidcAuthentication,
    audit: &YardOidcAuditContext,
) -> Result<AuthenticationOutcome, RepositoryError> {
    yard_oidc_validation::authentication(authentication, audit)?;
    let now = super::auth_validation::sql_time(authentication.authenticated_at_ms)?;
    let scope = scope(transaction, &authentication.host_label)?;
    if let Some(identity) = existing(transaction, authentication, &scope.workspace)? {
        return authenticate_existing(transaction, authentication, identity, now);
    }
    let Some(normalized_email) = authentication.normalized_email.as_deref() else {
        return Ok(AuthenticationOutcome::Denied);
    };
    let candidates = candidates(transaction, normalized_email, &scope, now)?;
    let [subject_id] = candidates.as_slice() else {
        return Ok(AuthenticationOutcome::Denied);
    };
    let identity = new_identity(
        authentication,
        normalized_email,
        &scope.workspace,
        subject_id,
    );
    insert_identity(transaction, &identity, now)?;
    insert_link_audit(
        transaction,
        audit,
        &scope,
        subject_id,
        authentication.authenticated_at_ms,
    )?;
    Ok(AuthenticationOutcome::Authenticated(identity))
}

fn scope(transaction: &Transaction<'_>, host_label: &str) -> Result<YardScope, RepositoryError> {
    let mut statement = transaction
        .prepare(
            "SELECT y.id, e.id, y.workspace_id
             FROM web_yards y
             JOIN yard_environments e
               ON e.yard_id = y.id AND e.kind = 'production' AND e.status = 'active'
             WHERE y.status = 'active'
               AND (
                 y.host_label = ?1
                 OR EXISTS (
                   SELECT 1 FROM yard_deploys deploy
                   WHERE deploy.yard_id = y.id
                     AND deploy.deployment_host_label = ?1
                     AND deploy.status IN ('live', 'superseded')
                 )
               )
             LIMIT 2",
        )
        .map_err(map_error)?;
    let matches = statement
        .query_map([host_label], |row| {
            Ok(YardScope {
                yard: row.get(0)?,
                environment: row.get(1)?,
                workspace: row.get(2)?,
            })
        })
        .map_err(map_error)
        .and_then(|matches| matches.collect::<Result<Vec<_>, _>>().map_err(map_error))?;
    match matches.as_slice() {
        [scope] => Ok(YardScope {
            yard: scope.yard.clone(),
            environment: scope.environment.clone(),
            workspace: scope.workspace.clone(),
        }),
        _ => Err(RepositoryError::NotFound),
    }
}

fn existing(
    transaction: &Transaction<'_>,
    authentication: &NewYardOidcAuthentication,
    workspace_id: &str,
) -> Result<Option<YardOidcIdentityRecord>, RepositoryError> {
    transaction
        .query_row(
            &format!(
                "SELECT {} FROM yard_oidc_identities
                 WHERE issuer = ?1 AND provider_subject = ?2 AND workspace_id = ?3",
                yard_oidc_rows::IDENTITY_COLUMNS
            ),
            params![
                authentication.issuer,
                authentication.provider_subject,
                workspace_id
            ],
            yard_oidc_rows::identity,
        )
        .optional()
        .map_err(map_error)
}

fn authenticate_existing(
    transaction: &Transaction<'_>,
    authentication: &NewYardOidcAuthentication,
    mut identity: YardOidcIdentityRecord,
    now: i64,
) -> Result<AuthenticationOutcome, RepositoryError> {
    if authentication.normalized_email.as_deref() != Some(identity.normalized_email.as_str()) {
        yard_session_store::revoke_for_user(transaction, &identity.yard_subject_id, now)?;
        return Ok(AuthenticationOutcome::Denied);
    }
    if authentication.authenticated_at_ms < identity.created_at_ms
        || !super::authority::active(transaction, &identity, now)?
    {
        return Ok(AuthenticationOutcome::Denied);
    }
    transaction
        .execute(
            "UPDATE yard_oidc_identities
             SET last_authenticated_at_ms =
               CASE WHEN last_authenticated_at_ms < ?4 THEN ?4 ELSE last_authenticated_at_ms END
             WHERE issuer = ?1 AND provider_subject = ?2 AND workspace_id = ?3",
            params![
                identity.issuer,
                identity.provider_subject,
                identity.workspace_id,
                now
            ],
        )
        .map_err(map_error)?;
    identity.last_authenticated_at_ms = identity
        .last_authenticated_at_ms
        .max(authentication.authenticated_at_ms);
    Ok(AuthenticationOutcome::Authenticated(identity))
}

fn candidates(
    transaction: &Transaction<'_>,
    normalized_email: &str,
    scope: &YardScope,
    now: i64,
) -> Result<Vec<String>, RepositoryError> {
    let mut statement = transaction
        .prepare(
            "SELECT subject.id
             FROM local_users user
             JOIN yard_subjects subject
               ON subject.id = user.id
              AND subject.kind = 'member'
              AND subject.workspace_id = user.workspace_id
              AND subject.local_user_id = user.id
              AND subject.invitation_id IS NULL
              AND subject.revoked_at_ms IS NULL
             WHERE user.workspace_id = ?1 AND user.email = ?2
               AND user.status = 'active' AND user.deactivated_at_ms IS NULL
             UNION ALL
             SELECT subject.id
             FROM yard_guest_invitations invitation
             JOIN yard_subjects subject
               ON subject.id = invitation.accepted_subject_id
              AND subject.workspace_id = invitation.workspace_id
              AND subject.invitation_id = invitation.id
              AND subject.kind = 'guest'
              AND subject.local_user_id IS NULL
              AND subject.revoked_at_ms IS NULL
             JOIN yard_access_grants grant
               ON grant.id = invitation.grant_id
              AND grant.yard_id = invitation.yard_id
              AND grant.principal_kind = 'guest-invite'
              AND grant.principal_id = invitation.id
             WHERE invitation.workspace_id = ?1 AND invitation.email = ?2
               AND invitation.yard_id = ?3 AND invitation.status = 'accepted'
               AND invitation.revoked_at_ms IS NULL AND invitation.expires_at_ms > ?5
               AND (invitation.environment_id IS NULL OR invitation.environment_id = ?4)
               AND grant.status = 'active' AND grant.revoked_at_ms IS NULL
               AND grant.expires_at_ms = invitation.expires_at_ms
               AND grant.expires_at_ms > ?5
               AND grant.environment_id IS invitation.environment_id
             LIMIT 2",
        )
        .map_err(map_error)?;
    statement
        .query_map(
            params![
                scope.workspace,
                normalized_email,
                scope.yard,
                scope.environment,
                now
            ],
            |row| row.get(0),
        )
        .map_err(map_error)
        .and_then(super::collect)
}

fn new_identity(
    authentication: &NewYardOidcAuthentication,
    normalized_email: &str,
    workspace_id: &str,
    subject_id: &str,
) -> YardOidcIdentityRecord {
    YardOidcIdentityRecord {
        issuer: authentication.issuer.clone(),
        provider_subject: authentication.provider_subject.clone(),
        workspace_id: workspace_id.to_owned(),
        yard_subject_id: subject_id.to_owned(),
        normalized_email: normalized_email.to_owned(),
        created_at_ms: authentication.authenticated_at_ms,
        last_authenticated_at_ms: authentication.authenticated_at_ms,
    }
}

fn insert_identity(
    transaction: &Transaction<'_>,
    identity: &YardOidcIdentityRecord,
    now: i64,
) -> Result<(), RepositoryError> {
    let changed = transaction
        .execute(
            "INSERT INTO yard_oidc_identities
             (issuer, provider_subject, workspace_id, yard_subject_id, normalized_email,
              created_at_ms, last_authenticated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                identity.issuer,
                identity.provider_subject,
                identity.workspace_id,
                identity.yard_subject_id,
                identity.normalized_email,
                now
            ],
        )
        .map_err(map_error)?;
    super::changed_once(changed)
}

fn insert_link_audit(
    transaction: &Transaction<'_>,
    audit: &YardOidcAuditContext,
    scope: &YardScope,
    subject_id: &str,
    now_ms: u64,
) -> Result<(), RepositoryError> {
    lifecycle_audit::insert(
        transaction,
        &NewAuditEvent {
            id: audit.id.clone(),
            workspace_id: scope.workspace.clone(),
            actor: subject_id.to_owned(),
            action: YARD_OIDC_IDENTITY_LINKED_ACTION.to_owned(),
            request_id: audit.request_id.clone(),
            target_type: YARD_OIDC_IDENTITY_AUDIT_TARGET.to_owned(),
            metadata: yard_oidc_identity_audit_metadata(&scope.yard, subject_id),
            created_at_ms: now_ms,
        },
    )
}

#[cfg(test)]
#[path = "identities_tests.rs"]
mod tests;
