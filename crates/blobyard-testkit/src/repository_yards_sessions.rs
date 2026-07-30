use super::YardConformanceRepository;
use super::session_fixtures::{audit_count, new_session, production_environment, set_visibility};
use super::session_groups::{assert_live_policy, create_group_access};
use crate::{FixtureExecutionTracker, hash, local_user, local_user_event, login_key};
use blobyard_contract::{
    NewYardContinuation, RepositoryError, YARD_EXCHANGE_CODE_LIFETIME_MS, YardSessionAuditContext,
    YardSessionStatus, YardStartRecord, YardVisibility,
};

pub(super) fn assert_session_controls(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    assert_visibility_admission(repository, first, tracker)?;
    super::session_direct::assert_direct_grant_session(repository, first, version_id, tracker)?;
    create_group_access(repository, first, tracker)?;
    let session = exchange_session(repository, first, tracker)?;
    set_visibility(
        repository,
        &first.yard.id,
        "selected",
        YardVisibility::AnyAuthenticated,
        107,
    )?;
    assert_host_binding_and_touch(repository, first, version_id, &session, tracker)?;
    assert_live_policy(repository, first, version_id, &session, tracker)?;
    super::session_revocation::assert_management_revocation(repository, first, &session, tracker)?;
    super::session_revocation::assert_logout_and_deactivation(repository, first, tracker)?;
    repository.purge_yard_session_history(4_000_000_000)
}

pub(super) fn create_session_user(
    repository: &dyn YardConformanceRepository,
) -> Result<(), RepositoryError> {
    let user = local_user(
        "workspace_fixture",
        "user_fixture",
        Some("member@example.test".to_owned()),
        100,
    );
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
    tracker: &mut FixtureExecutionTracker,
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
    tracker.record_case(
        "non-public-non-navigation-matches-unknown-host",
        &serde_json::json!({"surface": "yard-delivery"}),
        &serde_json::json!({
            "responseClass": "concealed-not-found"
        }),
    );
    Ok(())
}

fn exchange_session(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
) -> Result<blobyard_contract::YardSessionRecord, RepositoryError> {
    let continuation = issue_selected_continuation(repository, first, tracker)?;
    assert_exchange_visibility_drift(repository, first, &continuation, tracker)?;
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
    tracker.record_case(
        "replay-is-indistinguishable-from-expiry",
        &serde_json::json!({"surface": "code-exchange"}),
        &serde_json::json!({"repositoryError": "NOT_FOUND"}),
    );
    tracker.record_case(
        "session-lifetime-is-twelve-hours",
        &serde_json::json!({"surface": "session"}),
        &serde_json::json!({
            "lifetimeMilliseconds":
                exchange.session.expires_at_ms - exchange.session.created_at_ms
        }),
    );
    Ok(exchange.session)
}

fn issue_selected_continuation(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    tracker: &mut FixtureExecutionTracker,
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
    tracker.record_case(
        "exchange-code-lifetime-is-sixty-seconds",
        &serde_json::json!({"surface": "exchange-code"}),
        &serde_json::json!({"lifetimeMilliseconds": YARD_EXCHANGE_CODE_LIFETIME_MS}),
    );
    Ok(continuation)
}

fn assert_exchange_visibility_drift(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    continuation: &NewYardContinuation,
    tracker: &mut FixtureExecutionTracker,
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
    tracker.record_case(
        "visibility-drift-between-issue-and-exchange-denies",
        &serde_json::json!({
            "principalKind": "group",
            "change": "selected-to-owner",
            "driftPoint": "after-code-issue"
        }),
        &serde_json::json!({"admitted": false, "repositoryError": "NOT_FOUND"}),
    );
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
    tracker: &mut FixtureExecutionTracker,
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
    let current = listed
        .iter()
        .find(|listing| listing.session.id == session.id)
        .ok_or(RepositoryError::Unavailable)?;
    if current.session.last_used_at_ms != Some(120)
        || current.session.status_at(120) != YardSessionStatus::Active
        || current.user_display_name != "Fixture user"
    {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "active-group-grant-admits-current-member",
        &serde_json::json!({
            "grantStatus": "active",
            "groupStatus": "active",
            "membershipStatus": "active",
            "principalKind": "group",
            "surface": "selected-yard"
        }),
        &serde_json::json!({
            "admitted": true,
            "reevaluatedAt": ["code-issue", "code-exchange", "private-delivery"]
        }),
    );
    Ok(())
}
