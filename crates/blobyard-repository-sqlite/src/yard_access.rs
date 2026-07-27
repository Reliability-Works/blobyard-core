use super::{
    lifecycle_audit, map_error, rows, transfer_validation, yard_access_principals, yard_queries,
    yard_rows, yard_validation,
};
use blobyard_contract::{
    AuditValue, NewAuditEvent, NewYardAccessGrant, RepositoryError, RevocableStatus, WebYardRecord,
    WebYardStatus, YardAccessGrantRecord, YardAccessPolicyRecord, YardAccessPrincipalKind,
    YardVisibility,
};
use rusqlite::{Connection, OptionalExtension, Row, Statement, Transaction, params};
use std::collections::HashSet;

pub(super) const GRANT_COLUMNS: &str = "id, yard_id, environment_id, principal_kind, principal_id, app_roles, status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms";
const MAXIMUM_ROLES: usize = 16;
const MAXIMUM_ROLE_LENGTH: usize = 64;

pub(super) fn policy(
    connection: &Connection,
    yard_id: &str,
) -> Result<Option<YardAccessPolicyRecord>, RepositoryError> {
    connection
        .query_row(
            "SELECT yard_id, visibility, updated_at_ms, updated_by_principal FROM yard_access_policies WHERE yard_id = ?1",
            [yard_id],
            policy_row,
        )
        .optional()
        .map_err(map_error)
}

pub(super) fn set_visibility(
    transaction: &Transaction<'_>,
    yard_id: &str,
    visibility: YardVisibility,
    updated_at_ms: u64,
    event: &NewAuditEvent,
) -> Result<YardAccessPolicyRecord, RepositoryError> {
    let yard = active_yard(transaction, yard_id)?;
    let previous =
        policy(transaction, yard_id)?.map_or(YardVisibility::Public, |record| record.visibility);
    let updated_at = yard_validation::action_event(
        event,
        "yard.visibility_changed",
        "yard_access_policy",
        &yard.workspace_id,
        updated_at_ms,
        [
            ("from", AuditValue::String(previous.as_str().to_owned())),
            ("to", AuditValue::String(visibility.as_str().to_owned())),
            ("yardId", AuditValue::String(yard.id.clone())),
        ],
    )?;
    transaction
        .execute(
            "INSERT INTO yard_access_policies (yard_id, visibility, updated_at_ms, updated_by_principal) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(yard_id) DO UPDATE SET visibility = excluded.visibility, updated_at_ms = excluded.updated_at_ms, updated_by_principal = excluded.updated_by_principal",
            params![yard.id, visibility.as_str(), updated_at, event.actor],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)?;
    policy(transaction, yard_id)?.ok_or(RepositoryError::Unavailable)
}

pub(super) fn insert_grant(
    transaction: &Transaction<'_>,
    grant: &NewYardAccessGrant,
    event: &NewAuditEvent,
) -> Result<YardAccessGrantRecord, RepositoryError> {
    let yard = validated_grant_yard(transaction, grant)?;
    let roles =
        super::yard_application_policy::validated_roles(transaction, &yard.id, &grant.app_roles)?;
    let app_roles = serde_json::Value::from(roles).to_string();
    let created_at = yard_validation::action_event(
        event,
        "yard.access_granted",
        "yard_access_grant",
        &yard.workspace_id,
        grant.created_at_ms,
        [
            (
                "environmentId",
                grant
                    .environment_id
                    .clone()
                    .map_or(AuditValue::Null, AuditValue::String),
            ),
            ("grantId", AuditValue::String(grant.id.clone())),
            (
                "principalKind",
                AuditValue::String(grant.principal_kind.as_str().to_owned()),
            ),
            ("yardId", AuditValue::String(yard.id.clone())),
        ],
    )?;
    let expires_at = grant
        .expires_at_ms
        .map(transfer_validation::to_i64)
        .transpose()?;
    transaction
        .execute(
            "INSERT INTO yard_access_grants (id, yard_id, environment_id, principal_kind, principal_id, app_roles, status, created_at_ms, created_by_principal, expires_at_ms, revoked_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?8, ?9, NULL)",
            params![
                grant.id,
                yard.id,
                grant.environment_id,
                grant.principal_kind.as_str(),
                grant.principal_id,
                app_roles,
                created_at,
                grant.created_by_principal,
                expires_at,
            ],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)?;
    grant_by_id(transaction, &grant.id)?.ok_or(RepositoryError::Unavailable)
}

pub(super) fn revoke_grant(
    transaction: &Transaction<'_>,
    yard_id: &str,
    grant_id: &str,
    revoked_at_ms: u64,
    event: &NewAuditEvent,
) -> Result<bool, RepositoryError> {
    let yard = active_yard(transaction, yard_id)?;
    let grant = grant_by_id(transaction, grant_id)?.ok_or(RepositoryError::NotFound)?;
    if grant.yard_id != yard.id {
        return Err(RepositoryError::NotFound);
    }
    if grant.status == RevocableStatus::Revoked {
        return Ok(false);
    }
    let revoked_at = yard_validation::action_event(
        event,
        "yard.access_revoked",
        "yard_access_grant",
        &yard.workspace_id,
        revoked_at_ms,
        [
            ("grantId", AuditValue::String(grant.id.clone())),
            ("yardId", AuditValue::String(yard.id.clone())),
        ],
    )?;
    transaction
        .execute(
            "UPDATE yard_access_grants SET status = 'revoked', revoked_at_ms = ?2 WHERE id = ?1 AND status = 'active'",
            params![grant.id, revoked_at],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)?;
    Ok(true)
}

pub(super) fn list_grants(
    statement: &mut Statement<'_>,
    yard_id: &str,
    now_ms: i64,
) -> Result<Vec<YardAccessGrantRecord>, RepositoryError> {
    super::collect(
        statement
            .query_map(params![yard_id, now_ms], grant)
            .map_err(map_error)?,
    )
}

pub(super) fn grant_by_id(
    connection: &Connection,
    grant_id: &str,
) -> Result<Option<YardAccessGrantRecord>, RepositoryError> {
    connection
        .query_row(
            &format!("SELECT {GRANT_COLUMNS} FROM yard_access_grants WHERE id = ?1"),
            [grant_id],
            grant,
        )
        .optional()
        .map_err(map_error)
}

fn validated_grant_yard(
    transaction: &Transaction<'_>,
    grant: &NewYardAccessGrant,
) -> Result<WebYardRecord, RepositoryError> {
    for value in [
        &grant.id,
        &grant.yard_id,
        &grant.principal_id,
        &grant.created_by_principal,
    ] {
        rows::validate_text(value)?;
    }
    let yard = active_yard(transaction, &grant.yard_id)?;
    if let Some(environment_id) = &grant.environment_id {
        rows::validate_text(environment_id)?;
        require_active_environment(transaction, &yard.id, environment_id)?;
    }
    if grant
        .expires_at_ms
        .is_some_and(|expires| expires < grant.created_at_ms)
    {
        return Err(RepositoryError::InvalidInput);
    }
    yard_access_principals::validate(transaction, &yard, grant)?;
    Ok(yard)
}

pub(super) fn active_yard(
    transaction: &Transaction<'_>,
    yard_id: &str,
) -> Result<WebYardRecord, RepositoryError> {
    let yard = yard_queries::yard_by_id(transaction, yard_id)?;
    if yard.status == WebYardStatus::Active {
        Ok(yard)
    } else {
        Err(RepositoryError::NotFound)
    }
}

fn require_active_environment(
    transaction: &Transaction<'_>,
    yard_id: &str,
    environment_id: &str,
) -> Result<(), RepositoryError> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM yard_environments WHERE id = ?1 AND yard_id = ?2 AND status = 'active')",
            params![environment_id, yard_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    if exists {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn encode_roles(roles: &[String]) -> Result<String, RepositoryError> {
    let mut seen = HashSet::with_capacity(roles.len());
    let valid = roles.len() <= MAXIMUM_ROLES
        && roles.iter().all(|role| {
            !role.is_empty()
                && role.len() <= MAXIMUM_ROLE_LENGTH
                && role.trim() == role
                && !role.chars().any(char::is_control)
                && seen.insert(role.as_str())
        });
    if !valid {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(serde_json::Value::from(roles.to_vec()).to_string())
}

pub(super) fn decode_roles(encoded: &str) -> Option<Vec<String>> {
    let roles: Vec<String> = serde_json::from_str(encoded).ok()?;
    let canonical = serde_json::Value::from(roles.clone()).to_string();
    (canonical == encoded).then_some(roles)
}

fn policy_row(row: &Row<'_>) -> rusqlite::Result<YardAccessPolicyRecord> {
    let visibility: String = row.get(1)?;
    Ok(YardAccessPolicyRecord {
        yard_id: row.get(0)?,
        visibility: YardVisibility::parse(&visibility)
            .ok_or_else(|| rows::conversion_error(visibility))?,
        updated_at_ms: yard_rows::required_u64(row.get(2)?)?,
        updated_by_principal: row.get(3)?,
    })
}

pub(super) fn grant(row: &Row<'_>) -> rusqlite::Result<YardAccessGrantRecord> {
    let principal_kind: String = row.get(3)?;
    let app_roles: String = row.get(5)?;
    let status: String = row.get(6)?;
    let record = YardAccessGrantRecord {
        id: row.get(0)?,
        yard_id: row.get(1)?,
        environment_id: row.get(2)?,
        principal_kind: YardAccessPrincipalKind::parse(&principal_kind)
            .ok_or_else(|| rows::conversion_error(principal_kind))?,
        principal_id: row.get(4)?,
        app_roles: decode_roles(&app_roles).ok_or_else(|| rows::conversion_error(app_roles))?,
        status: RevocableStatus::parse(&status).ok_or_else(|| rows::conversion_error(status))?,
        created_at_ms: yard_rows::required_u64(row.get(7)?)?,
        created_by_principal: row.get(8)?,
        expires_at_ms: yard_rows::optional_u64(row.get(9)?)?,
        revoked_at_ms: yard_rows::optional_u64(row.get(10)?)?,
    };
    super::yard_access_record_validation::validate(&record)?;
    Ok(record)
}

#[cfg(test)]
#[path = "yard_access_row_tests.rs"]
mod tests;
