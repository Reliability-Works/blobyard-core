use super::{
    lifecycle_audit, map_error, rows, yard_access, yard_guest_events, yard_guest_queries,
    yard_guest_rows,
};
use blobyard_contract::{
    MAXIMUM_ACTIVE_YARD_GUEST_INVITES, NewAuditEvent, NewYardAccessGrant, NewYardGuestInvite,
    RepositoryError, YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS, YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS,
    YardAccessPrincipalKind, YardGuestInviteRecord,
};
use rusqlite::{Transaction, params};

pub(super) fn create(
    transaction: &Transaction<'_>,
    invitation: &NewYardGuestInvite,
    grant: &NewYardAccessGrant,
    event: &NewAuditEvent,
) -> Result<YardGuestInviteRecord, RepositoryError> {
    validate_create(transaction, invitation, grant)?;
    let created_at = yard_guest_events::validate(
        event,
        "yard.guest_invite.created",
        invitation,
        None,
        invitation.created_at_ms,
    )?;
    let expires_at = super::auth_validation::sql_time(invitation.expires_at_ms)?;
    require_capacity(transaction, invitation, created_at)?;
    let roles = super::yard_application_policy::validated_roles(
        transaction,
        &invitation.yard_id,
        &grant.app_roles,
    )?;
    insert_grant(transaction, grant, &roles, created_at, expires_at)?;
    insert_invitation(transaction, invitation, created_at, expires_at)?;
    lifecycle_audit::insert(transaction, event)?;
    yard_guest_queries::by_id(transaction, &invitation.id)?.ok_or(RepositoryError::Unavailable)
}

fn validate_create(
    transaction: &Transaction<'_>,
    invitation: &NewYardGuestInvite,
    grant: &NewYardAccessGrant,
) -> Result<(), RepositoryError> {
    yard_guest_rows::validate_invitation_texts(invitation, &invitation.email)?;
    super::auth_validation::validate_hash(&invitation.token_hash)?;
    let yard = yard_access::active_yard(transaction, &invitation.yard_id)?;
    let lifetime = invitation
        .expires_at_ms
        .checked_sub(invitation.created_at_ms);
    let invitation_valid = rows::valid_prefixed_hex_id(&invitation.id, "ygi_")
        && yard_guest_rows::normalized_email(&invitation.email)
        && yard.workspace_id == invitation.workspace_id
        && yard.project_id == invitation.project_id
        && lifetime.is_some_and(|value| {
            (YARD_GUEST_INVITE_MINIMUM_LIFETIME_MS..=YARD_GUEST_INVITE_MAXIMUM_LIFETIME_MS)
                .contains(&value)
        });
    let grant_scope_valid = [
        grant.id.eq(&invitation.grant_id),
        grant.yard_id == invitation.yard_id,
        grant.environment_id == invitation.environment_id,
        grant.principal_kind == YardAccessPrincipalKind::GuestInvite,
        grant.principal_id == invitation.id,
    ]
    .into_iter()
    .all(|valid| valid);
    let grant_valid = grant_scope_valid
        && grant.created_at_ms == invitation.created_at_ms
        && grant.created_by_principal.trim() == grant.created_by_principal
        && grant.expires_at_ms == Some(invitation.expires_at_ms);
    if !invitation_valid || !grant_valid {
        return Err(RepositoryError::InvalidInput);
    }
    if let Some(environment_id) = invitation.environment_id.as_deref() {
        require_environment(transaction, &invitation.yard_id, environment_id)?;
    }
    Ok(())
}

fn require_environment(
    transaction: &Transaction<'_>,
    yard_id: &str,
    environment_id: &str,
) -> Result<(), RepositoryError> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM yard_environments
             WHERE id = ?1 AND yard_id = ?2 AND kind = 'production' AND status = 'active')",
            params![environment_id, yard_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    exists.then_some(()).ok_or(RepositoryError::NotFound)
}

fn require_capacity(
    transaction: &Transaction<'_>,
    invitation: &NewYardGuestInvite,
    now: i64,
) -> Result<(), RepositoryError> {
    let count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM (
               SELECT 1 FROM yard_guest_invitations
               WHERE yard_id = ?1 AND status IN ('pending', 'accepted')
                 AND expires_at_ms > ?2 LIMIT 101
             )",
            params![invitation.yard_id, now],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    #[expect(
        clippy::cast_possible_wrap,
        reason = "the fixed 100-invitation capacity is safely within i64"
    )]
    if count >= MAXIMUM_ACTIVE_YARD_GUEST_INVITES as i64 {
        return Err(RepositoryError::Conflict);
    }
    let duplicate: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM yard_guest_invitations
               WHERE yard_id = ?1 AND environment_id IS ?2 AND email = ?3
                 AND status IN ('pending', 'accepted') AND expires_at_ms > ?4
             )",
            params![
                invitation.yard_id,
                invitation.environment_id,
                invitation.email,
                now
            ],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    (!duplicate).then_some(()).ok_or(RepositoryError::Conflict)
}

fn insert_grant(
    transaction: &Transaction<'_>,
    grant: &NewYardAccessGrant,
    roles: &[String],
    created_at: i64,
    expires_at: i64,
) -> Result<(), RepositoryError> {
    let encoded = serde_json::Value::from(roles).to_string();
    transaction
        .execute(
            "INSERT INTO yard_access_grants
             (id, yard_id, environment_id, principal_kind, principal_id, app_roles, status,
              created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms)
             VALUES (?1, ?2, ?3, 'guest-invite', ?4, ?5, 'active', ?6, ?7, ?8, NULL)",
            params![
                grant.id,
                grant.yard_id,
                grant.environment_id,
                grant.principal_id,
                encoded,
                created_at,
                grant.created_by_principal,
                expires_at,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn insert_invitation(
    transaction: &Transaction<'_>,
    invitation: &NewYardGuestInvite,
    created_at: i64,
    expires_at: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO yard_guest_invitations
             (id, workspace_id, project_id, yard_id, environment_id, email, token_hash, status,
              accepted_subject_id, grant_id, created_at_ms, expires_at_ms, accepted_at_ms,
              revoked_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', NULL, ?8, ?9, ?10, NULL, NULL)",
            params![
                invitation.id,
                invitation.workspace_id,
                invitation.project_id,
                invitation.yard_id,
                invitation.environment_id,
                invitation.email,
                invitation.token_hash,
                invitation.grant_id,
                created_at,
                expires_at,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}
