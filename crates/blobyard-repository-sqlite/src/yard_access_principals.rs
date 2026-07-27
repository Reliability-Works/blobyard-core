use super::map_error;
use blobyard_contract::{
    MAXIMUM_ACTIVE_GROUP_GRANTS, NewYardAccessGrant, RepositoryError, WebYardRecord,
    YardAccessPrincipalKind,
};
use rusqlite::{Transaction, params};

pub(super) fn validate(
    transaction: &Transaction<'_>,
    yard: &WebYardRecord,
    grant: &NewYardAccessGrant,
) -> Result<(), RepositoryError> {
    if matches!(
        grant.principal_kind,
        YardAccessPrincipalKind::User | YardAccessPrincipalKind::Group
    ) && is_guest_subject(transaction, &grant.principal_id)?
    {
        return Err(RepositoryError::InvalidInput);
    }
    match grant.principal_kind {
        YardAccessPrincipalKind::User => {
            require_active_user(transaction, &yard.workspace_id, &grant.principal_id)
        }
        YardAccessPrincipalKind::Group => {
            require_active_group(transaction, &yard.workspace_id, &grant.principal_id)?;
            require_group_grant_capacity(transaction, &grant.principal_id)
        }
        YardAccessPrincipalKind::GuestInvite | YardAccessPrincipalKind::Link => Ok(()),
    }
}

fn is_guest_subject(
    transaction: &Transaction<'_>,
    principal_id: &str,
) -> Result<bool, RepositoryError> {
    transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM yard_subjects WHERE id = ?1 AND kind = 'guest'
             )",
            [principal_id],
            |row| row.get(0),
        )
        .map_err(map_error)
}

fn require_active_user(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    user_id: &str,
) -> Result<(), RepositoryError> {
    require_principal(
        transaction,
        "SELECT EXISTS(SELECT 1 FROM local_users WHERE id = ?1 AND workspace_id = ?2 AND status = 'active')",
        user_id,
        workspace_id,
    )
}

fn require_active_group(
    transaction: &Transaction<'_>,
    workspace_id: &str,
    group_id: &str,
) -> Result<(), RepositoryError> {
    require_principal(
        transaction,
        "SELECT EXISTS(SELECT 1 FROM workspace_groups WHERE id = ?1 AND workspace_id = ?2 AND status = 'active')",
        group_id,
        workspace_id,
    )
}

fn require_principal(
    transaction: &Transaction<'_>,
    sql: &str,
    principal_id: &str,
    workspace_id: &str,
) -> Result<(), RepositoryError> {
    let exists: bool = transaction
        .query_row(sql, params![principal_id, workspace_id], |row| row.get(0))
        .map_err(map_error)?;
    exists.then_some(()).ok_or(RepositoryError::NotFound)
}

fn require_group_grant_capacity(
    transaction: &Transaction<'_>,
    group_id: &str,
) -> Result<(), RepositoryError> {
    let count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM (
               SELECT 1 FROM yard_access_grants
               WHERE principal_kind = 'group' AND principal_id = ?1 AND status = 'active'
               LIMIT 501
             )",
            [group_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    (count < i64::from(MAXIMUM_ACTIVE_GROUP_GRANTS))
        .then_some(())
        .ok_or(RepositoryError::Conflict)
}
