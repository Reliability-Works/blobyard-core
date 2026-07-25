use super::YardConformanceRepository;
use super::session_fixtures::{
    issue_session, new_session, production_environment, session_revoked_event, set_visibility,
};
use super::session_groups::{assert_live_policy, create_group_access};
use crate::{hash, local_user, local_user_event, login_key};
use blobyard_contract::{
    NewYardContinuation, RepositoryError, YARD_EXCHANGE_CODE_LIFETIME_MS, YardSessionAuditContext,
    YardSessionStatus, YardStartRecord, YardVisibility,
};

pub(super) fn assert_session_controls(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
) -> Result<(), RepositoryError> {
    assert_visibility_admission(repository, first)?;
    create_group_access(repository, first)?;
    set_visibility(
        repository,
        &first.yard.id,
        "any-authenticated",
        YardVisibility::Selected,
        105,
    )?;
    let session = exchange_session(repository, first)?;
    set_visibility(
        repository,
        &first.yard.id,
        "selected",
        YardVisibility::AnyAuthenticated,
        107,
    )?;
    assert_host_binding_and_touch(repository, first, version_id, &session)?;
    assert_live_policy(repository, first, version_id, &session)?;
    assert_management_revocation(repository, first, &session)?;
    assert_logout_and_deactivation(repository, first)?;
    repository.purge_yard_session_history(4_000_000_000)
}

pub(super) fn create_session_user(
    repository: &dyn YardConformanceRepository,
) -> Result<(), RepositoryError> {
    let user = local_user("workspace_fixture", "user_fixture", None, 100);
    let key = login_key("userkey_yard", &user.id, '7', 100);
    repository.create_local_user(
        &user,
        &key,
        &local_user_event("audit_user_yard", &user, "user.created", 100),
    )
}

fn assert_visibility_admission(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<(), RepositoryError> {
    set_visibility(
        repository,
        &first.yard.id,
        "public",
        YardVisibility::Owner,
        101,
    )?;
    if repository.evaluate_yard_admission(&first.yard.host_label, "user_fixture", 102)
        != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    set_visibility(
        repository,
        &first.yard.id,
        "owner",
        YardVisibility::AnyAuthenticated,
        103,
    )?;
    let stable = repository.evaluate_yard_admission(&first.yard.host_label, "user_fixture", 104)?;
    let immutable = repository.evaluate_yard_admission(
        &first.deploy.deployment_host_label,
        "user_fixture",
        104,
    )?;
    let environment = production_environment(repository.list_yard_environments(&first.yard.id)?)?;
    if stable.yard_id != first.yard.id
        || stable.environment_id != environment.id
        || stable.workspace_id != first.yard.workspace_id
        || immutable != stable
        || repository.evaluate_yard_admission("unknown-host", "user_fixture", 104)
            != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn exchange_session(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<blobyard_contract::YardSessionRecord, RepositoryError> {
    let continuation = issue_selected_continuation(repository, first)?;
    assert_exchange_visibility_drift(repository, first, &continuation)?;
    let new_session_record = new_session("yardsession_fixture", 'c', 108);
    let exchange = repository.exchange_yard_session_code(
        &continuation.code_hash,
        &first.yard.host_label,
        &new_session_record,
        &YardSessionAuditContext {
            id: "audit_session_issued".to_owned(),
            request_id: "request_session_issued".to_owned(),
        },
        108,
    )?;
    if exchange.return_path != continuation.return_path
        || exchange.session.id != new_session_record.id
        || exchange.session.token_hash != new_session_record.token_hash
        || exchange.session.user_id != continuation.user_id
        || repository.exchange_yard_session_code(
            &continuation.code_hash,
            &first.yard.host_label,
            &new_session("yardsession_replay", 'd', 109),
            &YardSessionAuditContext {
                id: "audit_session_replay".to_owned(),
                request_id: "request_session_replay".to_owned(),
            },
            109,
        ) != Err(RepositoryError::NotFound)
        || audit_count(repository, "yard.session_issued", "audit_session_issued")? != 1
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(exchange.session)
}

fn issue_selected_continuation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<NewYardContinuation, RepositoryError> {
    let admission =
        repository.evaluate_yard_admission(&first.yard.host_label, "user_fixture", 104)?;
    let continuation = NewYardContinuation {
        id: "yardcont_fixture".to_owned(),
        continuation_hash: hash('a'),
        code_hash: hash('b'),
        yard_id: admission.yard_id,
        environment_id: admission.environment_id,
        host_label: first.yard.host_label.clone(),
        user_id: "user_fixture".to_owned(),
        return_path: "/reports?q=1".to_owned(),
        created_at_ms: 105,
        expires_at_ms: 105 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    };
    repository.issue_yard_exchange_code(&continuation)?;
    if repository.issue_yard_exchange_code(&continuation) != Err(RepositoryError::Conflict) {
        return Err(RepositoryError::Unavailable);
    }
    Ok(continuation)
}

fn assert_exchange_visibility_drift(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    continuation: &NewYardContinuation,
) -> Result<(), RepositoryError> {
    set_visibility(
        repository,
        &first.yard.id,
        "selected",
        YardVisibility::Owner,
        106,
    )?;
    if repository.exchange_yard_session_code(
        &continuation.code_hash,
        &first.yard.host_label,
        &new_session("yardsession_visibility_drift", 'd', 106),
        &YardSessionAuditContext {
            id: "audit_session_visibility_drift".to_owned(),
            request_id: "request_session_visibility_drift".to_owned(),
        },
        106,
    ) != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    set_visibility(
        repository,
        &first.yard.id,
        "owner",
        YardVisibility::Selected,
        107,
    )?;
    Ok(())
}

fn assert_host_binding_and_touch(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    session: &blobyard_contract::YardSessionRecord,
) -> Result<(), RepositoryError> {
    let target = repository.yard_file_by_host(
        &first.yard.host_label,
        "asset.js",
        Some(&session.token_hash),
        120,
    )?;
    if target.object.version.id != version_id
        || repository.yard_file_by_host(
            &first.deploy.deployment_host_label,
            "asset.js",
            Some(&session.token_hash),
            120,
        ) != Err(RepositoryError::NotFound)
    {
        return Err(RepositoryError::Unavailable);
    }
    repository.yard_file_by_host(
        &first.yard.host_label,
        "asset.js",
        Some(&session.token_hash),
        119,
    )?;
    let listed = repository.list_yard_sessions(&first.yard.id)?;
    if listed.len() != 1
        || listed[0].session.last_used_at_ms != Some(120)
        || listed[0].session.status_at(120) != YardSessionStatus::Active
        || listed[0].user_display_name != "Fixture user"
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn assert_management_revocation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    session: &blobyard_contract::YardSessionRecord,
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
        || repository.list_yard_sessions(&first.yard.id)?[0]
            .session
            .status_at(141)
            != YardSessionStatus::Revoked
        || audit_count(repository, "yard.session_revoked", &event.id)? != 1
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}

fn audit_count(
    repository: &dyn YardConformanceRepository,
    action: &str,
    event_id: &str,
) -> Result<usize, RepositoryError> {
    repository
        .list_audit("workspace_fixture", None, 100)
        .map(|page| {
            page.items
                .iter()
                .filter(|event| event.action == action && event.id == event_id)
                .count()
        })
}

fn assert_logout_and_deactivation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
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
    {
        return Err(RepositoryError::Unavailable);
    }
    Ok(())
}
