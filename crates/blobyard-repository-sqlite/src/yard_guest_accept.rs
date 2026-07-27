use super::{
    lifecycle_audit, map_error, yard_guest_events, yard_guest_queries, yard_guest_rows,
    yard_session_store,
};
use blobyard_contract::{
    NewAuditEvent, NewYardContinuation, RepositoryError, YardGuestAcceptance,
    YardGuestInviteRecord, YardGuestLoginKeyRecord, YardSubjectKind, YardSubjectRecord,
};
use rusqlite::{Transaction, params};

pub(super) fn accept(
    transaction: &Transaction<'_>,
    token_hash: &str,
    subject: &YardSubjectRecord,
    key: &YardGuestLoginKeyRecord,
    continuation: &NewYardContinuation,
    event: &NewAuditEvent,
    now_ms: u64,
) -> Result<YardGuestAcceptance, RepositoryError> {
    let now = super::auth_validation::sql_time(now_ms)?;
    let mut invitation = yard_guest_queries::pending_by_hash(transaction, token_hash, now)?;
    validate(&invitation, subject, key, continuation, event, now_ms)?;
    insert_subject(transaction, subject, now)?;
    let changed = transaction
        .execute(
            "UPDATE yard_guest_invitations
             SET token_hash = NULL, status = 'accepted', accepted_subject_id = ?2,
                 accepted_at_ms = ?3
             WHERE id = ?1 AND status = 'pending' AND token_hash = ?4 AND expires_at_ms > ?3",
            params![invitation.id, subject.id, now, token_hash],
        )
        .map_err(map_error)?;
    super::changed_once(changed)?;
    insert_key(
        transaction,
        key,
        now,
        invitation.expires_at_ms.cast_signed(),
    )?;
    yard_session_store::issue(transaction, continuation)?;
    lifecycle_audit::insert(transaction, event)?;
    invitation.status = blobyard_contract::YardGuestInviteStatus::Accepted;
    invitation.accepted_subject_id = Some(subject.id.clone());
    invitation.accepted_at_ms = Some(now_ms);
    Ok(YardGuestAcceptance {
        invitation,
        subject: subject.clone(),
    })
}

fn validate(
    invitation: &YardGuestInviteRecord,
    subject: &YardSubjectRecord,
    key: &YardGuestLoginKeyRecord,
    continuation: &NewYardContinuation,
    event: &NewAuditEvent,
    now_ms: u64,
) -> Result<(), RepositoryError> {
    yard_guest_rows::validate_subject(subject)?;
    super::yard_guest_keys::validate_new(key)?;
    let subject_valid = subject.kind == YardSubjectKind::Guest
        && subject.workspace_id == invitation.workspace_id
        && subject.invitation_id.as_deref() == Some(invitation.id.as_str())
        && subject.created_at_ms == now_ms
        && subject.revoked_at_ms.is_none();
    let key_valid = key.subject_id == subject.id
        && key.invitation_id == invitation.id
        && key.workspace_id == invitation.workspace_id
        && key.created_at_ms == now_ms
        && key.expires_at_ms == invitation.expires_at_ms;
    let continuation_valid = continuation.user_id.eq(&subject.id)
        && continuation.yard_id == invitation.yard_id
        && invitation
            .environment_id
            .as_ref()
            .is_none_or(|id| id == &continuation.environment_id)
        && continuation.created_at_ms == now_ms;
    yard_guest_events::validate(
        event,
        "yard.guest_invite.accepted",
        invitation,
        Some(&subject.id),
        now_ms,
    )?;
    if subject_valid && key_valid && continuation_valid {
        Ok(())
    } else {
        Err(RepositoryError::InvalidInput)
    }
}

fn insert_subject(
    transaction: &Transaction<'_>,
    subject: &YardSubjectRecord,
    created_at: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO yard_subjects
             (id, kind, workspace_id, local_user_id, invitation_id, created_at_ms, revoked_at_ms)
             VALUES (?1, 'guest', ?2, NULL, ?3, ?4, NULL)",
            params![
                subject.id,
                subject.workspace_id,
                subject.invitation_id,
                created_at,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn insert_key(
    transaction: &Transaction<'_>,
    key: &YardGuestLoginKeyRecord,
    created_at: i64,
    expires_at: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO yard_guest_login_keys
             (id, subject_id, invitation_id, workspace_id, token_prefix, secret_hash,
              created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL)",
            params![
                key.id,
                key.subject_id,
                key.invitation_id,
                key.workspace_id,
                key.token_prefix,
                key.secret_hash,
                created_at,
                expires_at,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}
