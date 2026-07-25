use super::YardConformanceRepository;
use super::session_fixtures::{new_session, set_visibility};
use crate::{FixtureExecutionTracker, hash};
use blobyard_contract::{
    NewYardContinuation, RepositoryError, YARD_EXCHANGE_CODE_LIFETIME_MS, YardSessionAuditContext,
    YardStartRecord, YardVisibility,
};

pub(super) fn assert_direct_grant_session(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
    version_id: &str,
    tracker: &mut FixtureExecutionTracker,
) -> Result<(), RepositoryError> {
    let mut direct = super::session_grants::direct_grant(&first.yard.id, 110);
    "grant_yard_direct_initial".clone_into(&mut direct.id);
    super::session_grants::insert_grant(repository, &direct)?;
    set_visibility(
        repository,
        &first.yard.id,
        "any-authenticated",
        YardVisibility::Selected,
        110,
    )?;
    let session = issue_direct_session(repository, first)?;
    let delivered = repository.yard_file_by_host(
        &first.yard.host_label,
        "asset.js",
        Some(&session.token_hash),
        111,
    )?;
    if delivered.object.version.id != version_id {
        return Err(RepositoryError::Unavailable);
    }
    tracker.record_case(
        "direct-user-grant-admits",
        &serde_json::json!({"principalKind": "user", "grantStatus": "active"}),
        &serde_json::json!({
            "admitted": true,
            "reevaluatedAt": ["code-issue", "code-exchange", "private-delivery"]
        }),
    );
    super::session_grants::revoke_grant(repository, &first.yard.id, &direct.id, 112)?;
    if !repository.revoke_yard_session_by_token(&session.token_hash, &first.yard.host_label, 112)? {
        return Err(RepositoryError::Unavailable);
    }
    set_visibility(
        repository,
        &first.yard.id,
        "selected",
        YardVisibility::AnyAuthenticated,
        112,
    )
}

fn issue_direct_session(
    repository: &dyn YardConformanceRepository,
    first: &YardStartRecord,
) -> Result<blobyard_contract::YardSessionRecord, RepositoryError> {
    let admission =
        repository.evaluate_yard_admission(&first.yard.host_label, "user_fixture", 110)?;
    let continuation = NewYardContinuation {
        id: "yardcont_direct_fixture".to_owned(),
        continuation_hash: hash('8'),
        code_hash: hash('9'),
        yard_id: admission.yard_id,
        environment_id: admission.environment_id,
        host_label: first.yard.host_label.clone(),
        user_id: "user_fixture".to_owned(),
        return_path: "/direct".to_owned(),
        created_at_ms: 110,
        expires_at_ms: 110 + YARD_EXCHANGE_CODE_LIFETIME_MS,
    };
    repository.issue_yard_exchange_code(&continuation)?;
    repository
        .exchange_yard_session_code(
            &continuation.code_hash,
            &first.yard.host_label,
            &new_session("yardsession_direct_fixture", '8', 111),
            &YardSessionAuditContext {
                id: "audit_session_direct_fixture".to_owned(),
                request_id: "request_session_direct_fixture".to_owned(),
            },
            111,
        )
        .map(|exchange| exchange.session)
}
