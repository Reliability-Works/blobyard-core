use super::Corruption;
use blobyard_contract::{
    RepositoryError, RevocableStatus, YardAccessGrantRecord, YardAccessPolicyRecord,
    YardAccessPrincipalKind, YardVisibility,
};

pub(super) fn corrupt_policy(
    corruption: Corruption,
    yard_id: &str,
    record: Option<YardAccessPolicyRecord>,
) -> Option<YardAccessPolicyRecord> {
    if matches!(corruption, Corruption::YardPhantomPolicy) && record.is_none() {
        return Some(phantom_policy(yard_id));
    }
    record
}

pub(super) const fn corrupt_visibility(
    corruption: Corruption,
    updated_at_ms: u64,
    mut record: YardAccessPolicyRecord,
) -> YardAccessPolicyRecord {
    match corruption {
        Corruption::YardVisibilityRecord if updated_at_ms == 6 => {
            record.visibility = YardVisibility::Public;
        }
        Corruption::YardRestoredVisibility if updated_at_ms == 17 => {
            record.visibility = YardVisibility::Owner;
        }
        _ => {}
    }
    record
}

pub(super) fn corrupt_inserted_grant(
    corruption: Corruption,
    created_at_ms: u64,
    result: Result<YardAccessGrantRecord, RepositoryError>,
) -> Result<YardAccessGrantRecord, RepositoryError> {
    if matches!(corruption, Corruption::YardGrantValidation)
        && result == Err(RepositoryError::InvalidInput)
    {
        return Err(RepositoryError::Unavailable);
    }
    result.map(|mut record| {
        match corruption {
            Corruption::YardGrantRecord if created_at_ms == 7 => record.app_roles.clear(),
            Corruption::YardScopedGrantRecord if created_at_ms == 8 => {
                record.environment_id = None;
            }
            _ => {}
        }
        record
    })
}

pub(super) fn corrupt_revocation(
    corruption: Corruption,
    grant_id: &str,
    revoked_at_ms: u64,
    result: Result<bool, RepositoryError>,
) -> Result<bool, RepositoryError> {
    match corruption {
        Corruption::YardMissingGrantRevoke
            if grant_id == "grant_missing" && result == Err(RepositoryError::NotFound) =>
        {
            Err(RepositoryError::Unavailable)
        }
        Corruption::YardFirstRevoke if revoked_at_ms == 14 => result.map(|_revoked| false),
        Corruption::YardSecondRevoke if revoked_at_ms == 15 => result.map(|_revoked| true),
        _ => result,
    }
}

pub(super) fn corrupt_grant_list(
    corruption: Corruption,
    yard_id: &str,
    now_ms: u64,
    mut records: Vec<YardAccessGrantRecord>,
) -> Vec<YardAccessGrantRecord> {
    match corruption {
        Corruption::YardPhantomGrantList if yard_id != "yard_unknown" && records.is_empty() => {
            records.push(phantom_grant(yard_id));
        }
        Corruption::YardUnknownGrantList if yard_id == "yard_unknown" => {
            records.push(phantom_grant(yard_id));
        }
        Corruption::YardExpiredGrantList if now_ms == 1_000 => records.push(phantom_grant(yard_id)),
        Corruption::YardRevokedGrantList if now_ms == 16 => records.push(phantom_grant(yard_id)),
        _ => {}
    }
    records
}

fn phantom_policy(yard_id: &str) -> YardAccessPolicyRecord {
    YardAccessPolicyRecord {
        yard_id: yard_id.to_owned(),
        visibility: YardVisibility::Owner,
        updated_at_ms: 0,
        updated_by_principal: "corrupt".to_owned(),
    }
}

fn phantom_grant(yard_id: &str) -> YardAccessGrantRecord {
    YardAccessGrantRecord {
        id: "grant_unexpected".to_owned(),
        yard_id: yard_id.to_owned(),
        environment_id: None,
        principal_kind: YardAccessPrincipalKind::User,
        principal_id: "user_unexpected".to_owned(),
        app_roles: Vec::new(),
        status: RevocableStatus::Active,
        created_at_ms: 0,
        created_by_principal: "corrupt".to_owned(),
        expires_at_ms: None,
        revoked_at_ms: None,
    }
}
