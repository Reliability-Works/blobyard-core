use super::{rows, yard_access, yard_rows};
use blobyard_contract::{
    RepositoryError, RevocableStatus, YardAccessGrantRecord, YardAccessPrincipalKind,
    YardGuestAuditInvitation, YardGuestInviteCursor, YardGuestInviteRecord, YardGuestInviteStatus,
    YardSubjectKind, YardSubjectRecord,
};
use rusqlite::Row;

pub(super) const INVITATION_COLUMNS: &str = "i.id, i.workspace_id, i.project_id, i.yard_id, i.environment_id, i.email, i.status, i.accepted_subject_id, i.grant_id, g.app_roles, i.created_at_ms, i.expires_at_ms, i.accepted_at_ms, i.revoked_at_ms, g.id, g.yard_id, g.environment_id, g.principal_kind, g.principal_id, g.app_roles, g.status, g.created_at_ms, g.created_by_principal, g.expires_at_ms, g.revoked_at_ms";

pub(super) fn invitation(row: &Row<'_>) -> rusqlite::Result<YardGuestInviteRecord> {
    let status: String = row.get(6)?;
    let encoded_roles: String = row.get(9)?;
    let decoded_roles = yard_access::decode_roles(&encoded_roles);
    let app_roles = decoded(encoded_roles, decoded_roles)?;
    let record = YardGuestInviteRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        project_id: row.get(2)?,
        yard_id: row.get(3)?,
        environment_id: row.get(4)?,
        email: row.get(5)?,
        status: {
            let parsed = YardGuestInviteStatus::parse(&status);
            decoded(status, parsed)?
        },
        accepted_subject_id: row.get(7)?,
        grant_id: row.get(8)?,
        app_roles: app_roles.clone(),
        created_at_ms: yard_rows::required_u64(row.get(10)?)?,
        expires_at_ms: yard_rows::required_u64(row.get(11)?)?,
        accepted_at_ms: yard_rows::optional_u64(row.get(12)?)?,
        revoked_at_ms: yard_rows::optional_u64(row.get(13)?)?,
    };
    let grant = grant(row, app_roles)?;
    validate_invitation(&record, &grant).or(Err(rows::conversion_error(record.id.clone())))?;
    Ok(record)
}

fn grant(row: &Row<'_>, app_roles: Vec<String>) -> rusqlite::Result<YardAccessGrantRecord> {
    let principal_kind: String = row.get(17)?;
    let status: String = row.get(20)?;
    Ok(YardAccessGrantRecord {
        id: row.get(14)?,
        yard_id: row.get(15)?,
        environment_id: row.get(16)?,
        principal_kind: {
            let parsed = YardAccessPrincipalKind::parse(&principal_kind);
            decoded(principal_kind, parsed)?
        },
        principal_id: row.get(18)?,
        app_roles,
        status: {
            let parsed = RevocableStatus::parse(&status);
            decoded(status, parsed)?
        },
        created_at_ms: yard_rows::required_u64(row.get(21)?)?,
        created_by_principal: row.get(22)?,
        expires_at_ms: yard_rows::optional_u64(row.get(23)?)?,
        revoked_at_ms: yard_rows::optional_u64(row.get(24)?)?,
    })
}

fn validate_invitation(
    invitation: &YardGuestInviteRecord,
    grant: &YardAccessGrantRecord,
) -> Result<(), RepositoryError> {
    validate_invitation_texts(invitation, &invitation.email)?;
    let scope_matches = grant.id.eq(&invitation.grant_id)
        && grant.yard_id == invitation.yard_id
        && grant.environment_id == invitation.environment_id
        && grant.principal_kind == YardAccessPrincipalKind::GuestInvite
        && grant.principal_id == invitation.id
        && grant.app_roles == invitation.app_roles
        && grant.created_at_ms == invitation.created_at_ms
        && grant.expires_at_ms == Some(invitation.expires_at_ms);
    let lifecycle_matches = match invitation.status {
        YardGuestInviteStatus::Pending => {
            invitation.accepted_subject_id.is_none()
                && invitation.accepted_at_ms.is_none()
                && invitation.revoked_at_ms.is_none()
                && grant.status == RevocableStatus::Active
                && grant.revoked_at_ms.is_none()
        }
        YardGuestInviteStatus::Accepted => {
            invitation.accepted_subject_id.is_some()
                && invitation.accepted_at_ms.is_some()
                && invitation.revoked_at_ms.is_none()
                && grant.status == RevocableStatus::Active
                && grant.revoked_at_ms.is_none()
        }
        YardGuestInviteStatus::Revoked => {
            invitation.revoked_at_ms.is_some()
                && grant.status == RevocableStatus::Revoked
                && grant.revoked_at_ms == invitation.revoked_at_ms
        }
    };
    if scope_matches
        && lifecycle_matches
        && invitation.created_at_ms < invitation.expires_at_ms
        && normalized_email(&invitation.email)
    {
        Ok(())
    } else {
        Err(RepositoryError::Unavailable)
    }
}

pub(super) fn validate_invitation_texts(
    invitation: &(impl YardGuestAuditInvitation + ?Sized),
    email: &str,
) -> Result<(), RepositoryError> {
    rows::validate_texts([
        invitation.invitation_id(),
        invitation.workspace_id(),
        invitation.project_id(),
        invitation.yard_id(),
        email,
        invitation.grant_id(),
    ])
}

pub(super) fn subject(row: &Row<'_>) -> rusqlite::Result<YardSubjectRecord> {
    let kind: String = row.get(1)?;
    let record = YardSubjectRecord {
        id: row.get(0)?,
        kind: {
            let parsed = YardSubjectKind::parse(&kind);
            decoded(kind, parsed)?
        },
        workspace_id: row.get(2)?,
        local_user_id: row.get(3)?,
        invitation_id: row.get(4)?,
        created_at_ms: yard_rows::required_u64(row.get(5)?)?,
        revoked_at_ms: yard_rows::optional_u64(row.get(6)?)?,
    };
    validate_subject(&record).or(Err(rows::conversion_error(record.id.clone())))?;
    Ok(record)
}

pub(super) fn validate_subject(subject: &YardSubjectRecord) -> Result<(), RepositoryError> {
    rows::validate_text(&subject.id)?;
    rows::validate_text(&subject.workspace_id)?;
    let valid = match subject.kind {
        YardSubjectKind::Member => {
            subject.local_user_id.as_deref() == Some(subject.id.as_str())
                && subject.invitation_id.is_none()
        }
        YardSubjectKind::Guest => {
            subject.local_user_id.is_none()
                && subject.invitation_id.is_some()
                && rows::valid_prefixed_hex_id(&subject.id, "guest_")
        }
    };
    if valid
        && subject
            .revoked_at_ms
            .is_none_or(|at| at >= subject.created_at_ms)
    {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

pub(super) fn cursor(record: &YardGuestInviteRecord) -> YardGuestInviteCursor {
    YardGuestInviteCursor {
        created_at_ms: record.created_at_ms,
        id: record.id.clone(),
    }
}

pub(super) fn normalized_email(value: &str) -> bool {
    (3..=254).contains(&value.len())
        && value.trim() == value
        && value.to_lowercase() == value
        && !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && !domain.is_empty() && !domain.contains('@')
        })
}

#[expect(
    clippy::option_if_let_else,
    reason = "the named decoder keeps conversion failure coverage explicit"
)]
fn decoded<T>(value: String, parsed: Option<T>) -> rusqlite::Result<T> {
    match parsed {
        Some(decoded) => Ok(decoded),
        None => Err(rows::conversion_error(value)),
    }
}

#[cfg(test)]
#[path = "yard_guest_rows_tests.rs"]
mod tests;
