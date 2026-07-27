use super::{lifecycle_audit, map_error, rows, yard_guest_events, yard_guest_queries};
use blobyard_contract::{
    NewAuditEvent, RepositoryError, YardGuestInviteRecord, YardGuestInviteStatus,
};
use rusqlite::{Transaction, params};

pub(super) fn revoke(
    transaction: &Transaction<'_>,
    yard_id: &str,
    invitation_id: &str,
    now_ms: u64,
    event: &NewAuditEvent,
) -> Result<YardGuestInviteRecord, RepositoryError> {
    rows::validate_text(yard_id)?;
    rows::validate_text(invitation_id)?;
    let mut invitation =
        yard_guest_queries::by_id(transaction, invitation_id)?.ok_or(RepositoryError::NotFound)?;
    if invitation.yard_id != yard_id {
        return Err(RepositoryError::NotFound);
    }
    if invitation.status == YardGuestInviteStatus::Revoked {
        return Err(RepositoryError::Conflict);
    }
    let now = yard_guest_events::validate(
        event,
        "yard.guest_invite.revoked",
        &invitation,
        invitation.accepted_subject_id.as_deref(),
        now_ms,
    )?;
    let changed = transaction
        .execute(
            "UPDATE yard_guest_invitations SET token_hash = NULL, status = 'revoked',
                 revoked_at_ms = ?2 WHERE id = ?1 AND status IN ('pending', 'accepted')",
            params![invitation.id, now],
        )
        .map_err(map_error)?;
    super::changed_once(changed)?;
    revoke_authority(transaction, &invitation, now)?;
    lifecycle_audit::insert(transaction, event)?;
    invitation.status = YardGuestInviteStatus::Revoked;
    invitation.revoked_at_ms = Some(now_ms);
    Ok(invitation)
}

fn revoke_authority(
    transaction: &Transaction<'_>,
    invitation: &YardGuestInviteRecord,
    now: i64,
) -> Result<(), RepositoryError> {
    let grant_changed = transaction
        .execute(
            "UPDATE yard_access_grants SET status = 'revoked', revoked_at_ms = ?2
             WHERE id = ?1 AND status = 'active' AND revoked_at_ms IS NULL",
            params![invitation.grant_id, now],
        )
        .map_err(map_error)?;
    super::changed_once(grant_changed)?;
    transaction
        .execute(
            "UPDATE yard_guest_login_keys SET revoked_at_ms = ?2
             WHERE invitation_id = ?1 AND revoked_at_ms IS NULL",
            params![invitation.id, now],
        )
        .map(|_changed| ())
        .map_err(map_error)
}
