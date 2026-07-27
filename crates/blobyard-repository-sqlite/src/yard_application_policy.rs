use super::{
    lifecycle_audit, map_error, rows, yard_access, yard_management_roles, yard_validation,
};
use blobyard_contract::{
    AuditValue, MAXIMUM_YARD_ACCESS_ROLES, NewAuditEvent, RepositoryError, YardAccessGrantRecord,
    YardApplicationPolicyRecord,
};
use blobyard_core::{
    ApplicationPolicyGraph, CanonicalApplicationPolicy, canonicalize_application_policy,
    valid_source_manifest_digest,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::BTreeSet;

use super::yard_application_policy_encoding::{encode_effective, encode_graph, role_json};
use super::yard_application_policy_row::policy_row;

const POLICY_COLUMNS: &str = "yard_id, workspace_id, revision, source_manifest_digest, policy_json, effective_json, approved_at_ms, approved_by_principal";

struct PolicyEventContext<'a> {
    workspace_id: &'a str,
    yard_id: &'a str,
    from_revision: Option<u64>,
    to_revision: u64,
    digest: &'a str,
    approved_at_ms: u64,
}

pub(super) fn get(
    connection: &Connection,
    yard_id: &str,
) -> Result<Option<YardApplicationPolicyRecord>, RepositoryError> {
    rows::validate_text(yard_id)?;
    let yard = super::yard_queries::yard_by_id(connection, yard_id)?;
    if yard.status != blobyard_contract::WebYardStatus::Active {
        return Err(RepositoryError::NotFound);
    }
    let policy = policy_by_yard(connection, yard_id)?;
    if policy
        .as_ref()
        .is_some_and(|record| record.workspace_id != yard.workspace_id)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(policy)
}

pub(super) fn set(
    transaction: &Transaction<'_>,
    yard_id: &str,
    source_manifest_digest: &str,
    policy: ApplicationPolicyGraph,
    approved_by_principal: &str,
    now_ms: i64,
    event: &NewAuditEvent,
) -> Result<YardApplicationPolicyRecord, RepositoryError> {
    let yard = yard_access::active_yard(transaction, yard_id)?;
    yard_management_roles::require_owner(transaction, yard_id)?;
    rows::validate_text(approved_by_principal)?;
    if !valid_source_manifest_digest(source_manifest_digest) {
        return Err(RepositoryError::InvalidInput);
    }
    let canonical =
        canonicalize_application_policy(policy).map_err(|_error| RepositoryError::InvalidInput)?;
    let previous = policy_by_yard(transaction, yard_id)?;
    let revision = previous
        .as_ref()
        .map_or(1_u64, |record| record.revision + 1);
    let approved_at = u64::try_from(now_ms).map_err(|_error| RepositoryError::InvalidInput)?;
    validate_policy_event(
        event,
        &PolicyEventContext {
            workspace_id: &yard.workspace_id,
            yard_id,
            from_revision: previous.as_ref().map(|record| record.revision),
            to_revision: revision,
            digest: source_manifest_digest,
            approved_at_ms: approved_at,
        },
        &canonical,
    )?;
    let policy_json = encode_graph(&canonical.graph);
    let effective_json = encode_effective(&canonical.effective);
    transaction
        .execute(
            "INSERT INTO yard_application_policies
               (yard_id, workspace_id, revision, source_manifest_digest, policy_json,
                effective_json, approved_at_ms, approved_by_principal)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(yard_id) DO UPDATE SET
               revision = excluded.revision,
               source_manifest_digest = excluded.source_manifest_digest,
               policy_json = excluded.policy_json,
               effective_json = excluded.effective_json,
               approved_at_ms = excluded.approved_at_ms,
               approved_by_principal = excluded.approved_by_principal",
            params![
                yard_id,
                yard.workspace_id,
                i64::try_from(revision).map_err(|_error| RepositoryError::Conflict)?,
                source_manifest_digest,
                policy_json,
                effective_json,
                now_ms,
                approved_by_principal,
            ],
        )
        .map_err(map_error)?;
    lifecycle_audit::insert(transaction, event)?;
    policy_by_yard(transaction, yard_id)?.ok_or(RepositoryError::Unavailable)
}

pub(super) fn set_grant_roles(
    transaction: &Transaction<'_>,
    yard_id: &str,
    grant_id: &str,
    app_roles: &[String],
    now_ms: i64,
    event: &NewAuditEvent,
) -> Result<YardAccessGrantRecord, RepositoryError> {
    let yard = yard_access::active_yard(transaction, yard_id)?;
    let grant =
        yard_access::grant_by_id(transaction, grant_id)?.ok_or(RepositoryError::NotFound)?;
    if grant.yard_id != yard.id || grant.status != blobyard_contract::RevocableStatus::Active {
        return Err(RepositoryError::NotFound);
    }
    let roles = validated_roles(transaction, yard_id, app_roles)?;
    let mut from = grant.app_roles;
    from.sort();
    let at = u64::try_from(now_ms).map_err(|_error| RepositoryError::InvalidInput)?;
    yard_validation::action_event(
        event,
        "yard.access_roles_set",
        "yard_access_grant",
        &yard.workspace_id,
        at,
        [
            ("from", AuditValue::String(role_json(&from))),
            ("grantId", AuditValue::String(grant_id.to_owned())),
            ("to", AuditValue::String(role_json(&roles))),
            ("yardId", AuditValue::String(yard_id.to_owned())),
        ],
    )?;
    let encoded = serde_json::Value::from(roles).to_string();
    let changed = transaction
        .execute(
            "UPDATE yard_access_grants SET app_roles = ?3
             WHERE id = ?1 AND yard_id = ?2 AND status = 'active'",
            params![grant_id, yard_id, encoded],
        )
        .map_err(map_error)?;
    super::changed_once(changed)?;
    lifecycle_audit::insert(transaction, event)?;
    yard_access::grant_by_id(transaction, grant_id)?.ok_or(RepositoryError::Unavailable)
}

pub(super) fn validated_roles(
    connection: &Connection,
    yard_id: &str,
    roles: &[String],
) -> Result<Vec<String>, RepositoryError> {
    if roles.len() > MAXIMUM_YARD_ACCESS_ROLES {
        return Err(RepositoryError::Conflict);
    }
    let canonical = canonical_roles(roles)?;
    if canonical.is_empty() {
        return Ok(canonical);
    }
    let policy = policy_by_yard(connection, yard_id)?.ok_or(RepositoryError::InvalidInput)?;
    if canonical
        .iter()
        .all(|role| policy.policy.roles.contains_key(role))
    {
        Ok(canonical)
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn policy_by_yard(
    connection: &Connection,
    yard_id: &str,
) -> Result<Option<YardApplicationPolicyRecord>, RepositoryError> {
    connection
        .query_row(
            &format!("SELECT {POLICY_COLUMNS} FROM yard_application_policies WHERE yard_id = ?1"),
            [yard_id],
            policy_row,
        )
        .optional()
        .map_err(map_error)
}

fn validate_policy_event(
    event: &NewAuditEvent,
    context: &PolicyEventContext<'_>,
    canonical: &CanonicalApplicationPolicy,
) -> Result<(), RepositoryError> {
    let mut permissions = BTreeSet::new();
    for role in canonical.graph.roles.values() {
        permissions.extend(role.permissions.iter());
    }
    let mut permission_count = 0_u64;
    for _permission in permissions {
        permission_count += 1;
    }
    let mut role_count = 0_u64;
    for _role in canonical.graph.roles.keys() {
        role_count += 1;
    }
    yard_validation::action_event(
        event,
        "yard.application_policy_set",
        "yard_application_policy",
        context.workspace_id,
        context.approved_at_ms,
        [
            (
                "fromRevision",
                context
                    .from_revision
                    .map_or(AuditValue::Null, AuditValue::Number),
            ),
            ("permissionCount", AuditValue::Number(permission_count)),
            ("roleCount", AuditValue::Number(role_count)),
            (
                "sourceManifestDigest",
                AuditValue::String(context.digest.to_owned()),
            ),
            ("toRevision", AuditValue::Number(context.to_revision)),
            ("yardId", AuditValue::String(context.yard_id.to_owned())),
        ],
    )
    .map(|_at| ())
}

fn canonical_roles(roles: &[String]) -> Result<Vec<String>, RepositoryError> {
    let mut canonical = roles.to_vec();
    canonical.sort();
    let unique = canonical.windows(2).all(|window| window[0] != window[1]);
    if unique {
        yard_access::encode_roles(&canonical)?;
        Ok(canonical)
    } else {
        Err(RepositoryError::InvalidInput)
    }
}
