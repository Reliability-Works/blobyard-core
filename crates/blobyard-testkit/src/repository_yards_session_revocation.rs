use super::YardConformanceRepository;
use super::session_fixtures::{audit_count, issue_session, session_revoked_event};
use crate::{FixtureExecutionTracker, local_user, local_user_event};
use blobyard_contract::{RepositoryError, YardSessionStatus, YardStartRecord};

pub(super) fn assert_management_revocation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    session: &blobyard_contract::YardSessionRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let event = session_revoked_event(&first.yard.id, &session.id, 140);
    if !repository.revoke_yard_session(&first.yard.id, &session.id, 140, &event)?
        || repository.revoke_yard_session(&first.yard.id, &session.id, 141, &event)?
        || repository.yard_file_by_host(
            &first.yard.host_label,
            "asset.js",
            Some(&session.token_hash),
            141,
        ) != Err(RepositoryError::NotFound)
        || audit_count(repository, "yard.session_revoked", &event.id)? != 1
    {
        return Err(RepositoryError::Unavailable);
    }
    let listed = repository.list_yard_sessions(&first.yard.id)?;
    if listed
        .iter()
        .find(|listing| listing.session.id == session.id)
        .is_none_or(|listing| listing.session.status_at(141) != YardSessionStatus::Revoked)
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "revocation-bound-is-zero-ms",
        &serde_json::json!({"surface": "session-revocation"}),
        &serde_json::json!({"propagationMilliseconds": 0}),
    );
    Ok(())
}

pub(super) fn assert_logout_and_deactivation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let fallback = issue_session(repository, first, "fallback", '7', 145)?;
    let logout = issue_session(repository, first, "logout", 'e', 150)?;
    if !repository.revoke_yard_session_by_token(
        &fallback.token_hash,
        &first.yard.host_label,
        146,
    )? || !repository.revoke_yard_session_by_token(
        &logout.token_hash,
        &first.yard.host_label,
        151,
    )? || repository.revoke_yard_session_by_token(
        &logout.token_hash,
        &first.yard.host_label,
        152,
    )? {
        return Err(RepositoryError::Unavailable);
    }
    let deactivated = issue_session(repository, first, "deactivated", 'f', 160)?;
    let user = local_user("workspace_fixture", "user_fixture", None, 100);
    repository.deactivate_local_user(
        "user_fixture",
        161,
        &local_user_event("audit_user_session_gone", &user, "user.deactivated", 161),
    )?;
    let records = repository.list_yard_sessions(&first.yard.id)?;
    if records
        .iter()
        .find(|listing| listing.session.id == deactivated.id)
        .is_none_or(|listing| listing.session.revoked_at_ms != Some(161))
        || repository.yard_file_by_host(
            &first.yard.host_label,
            "asset.js",
            Some(&deactivated.token_hash),
            161,
        ) != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "user-deactivation-denies-on-next-private-request",
        &serde_json::json!({
            "principalKind": "group",
            "change": "user-deactivated",
            "surface": "private-delivery"
        }),
        &serde_json::json!({"admitted": false, "propagationMilliseconds": 0}),
    );
    Ok(())
}
