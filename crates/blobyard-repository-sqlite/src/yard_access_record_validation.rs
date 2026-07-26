use super::{rows, yard_access};
use blobyard_contract::{RevocableStatus, YardAccessGrantRecord};

pub(super) fn validate(record: &YardAccessGrantRecord) -> rusqlite::Result<()> {
    let valid_text = [
        record.id.as_str(),
        record.yard_id.as_str(),
        record.principal_id.as_str(),
        record.created_by_principal.as_str(),
    ]
    .into_iter()
    .all(|value| rows::validate_text(value).is_ok())
        && record
            .environment_id
            .as_deref()
            .is_none_or(|value| rows::validate_text(value).is_ok());
    let valid_lifecycle = match record.status {
        RevocableStatus::Active => record.revoked_at_ms.is_none(),
        RevocableStatus::Revoked => record
            .revoked_at_ms
            .is_some_and(|at_ms| at_ms >= record.created_at_ms),
    };
    let valid_expiry = record
        .expires_at_ms
        .is_none_or(|at_ms| at_ms >= record.created_at_ms);
    let valid_roles = yard_access::encode_roles(&record.app_roles).is_ok();
    (valid_text && valid_lifecycle && valid_expiry && valid_roles)
        .then_some(())
        .ok_or_else(|| rows::conversion_error("invalid persisted Yard grant"))
}
