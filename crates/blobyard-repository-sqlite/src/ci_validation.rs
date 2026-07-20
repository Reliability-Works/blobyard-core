use super::{auth_validation, rows};
use blobyard_contract::{
    AuditValue, CiAction, LocalCiTrustRecord, NewAuditEvent, NewMachineSession, RepositoryError,
    valid_github_ref_tail, valid_github_repository_part, valid_github_workflow_path,
};
use std::collections::BTreeSet;

pub(super) const MACHINE_SESSION_TTL_MS: u64 = 15 * 60 * 1_000;
pub(super) const EXCHANGE_RATE_LIMIT: u32 = 20;
pub(super) const EXCHANGE_RATE_WINDOW_MS: u64 = 60 * 1_000;

#[derive(Clone, Copy)]
pub(super) struct SqlMachineTimes {
    pub(super) now: i64,
    pub(super) expires: i64,
}

pub(super) fn validate_trust(trust: &LocalCiTrustRecord) -> Result<i64, RepositoryError> {
    for value in [
        &trust.id,
        &trust.workspace_id,
        &trust.repository,
        &trust.workflow_path,
        &trust.workflow_ref,
        &trust.allowed_ref_glob,
        &trust.audience,
    ] {
        rows::validate_text(value)?;
    }
    if let Some(value) = trust.project_id.as_deref() {
        rows::validate_text(value)?;
    }
    if let Some(value) = trust.environment.as_deref() {
        rows::validate_text(value)?;
    }
    if trust.revoked_at_ms.is_some()
        || !valid_repository(&trust.repository)
        || !valid_workflow_path(&trust.workflow_path)
        || !valid_git_ref(&trust.workflow_ref)
        || !valid_ref_glob(&trust.allowed_ref_glob)
    {
        return Err(RepositoryError::InvalidInput);
    }
    validate_actions(&trust.allowed_actions)?;
    auth_validation::sql_time(trust.created_at_ms)
}

pub(super) fn validate_exchange(
    session: &NewMachineSession,
) -> Result<SqlMachineTimes, RepositoryError> {
    auth_validation::validate_hash(&session.oidc_token_hash)?;
    auth_validation::validate_hash(&session.secret_hash)?;
    for value in [
        &session.project,
        &session.identity.audience,
        &session.identity.repository,
        &session.identity.git_ref,
        &session.identity.workflow_path,
        &session.identity.workflow_ref,
        &session.identity.run_id,
    ] {
        rows::validate_text(value)?;
    }
    for value in [
        session.workspace.as_deref(),
        session.identity.environment.as_deref(),
        session.identity.run_attempt.as_deref(),
        session.identity.sha.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        rows::validate_text(value)?;
    }
    validate_actions(&session.actions)?;
    let now = auth_validation::sql_time(session.now_ms)?;
    let expires = auth_validation::sql_time(session.identity.expires_at_ms)?;
    let expires_by = session.now_ms + MACHINE_SESSION_TTL_MS;
    let token_name = format!(
        "GitHub Actions {} run {}",
        session.identity.repository, session.identity.run_id
    );
    if !session.id.starts_with("machine_")
        || rows::validate_text(&session.id).is_err()
        || rows::validate_text(&session.token_prefix).is_err()
        || rows::validate_text(&token_name).is_err()
        || session.identity.expires_at_ms <= session.now_ms
        || session.identity.expires_at_ms > expires_by
        || !valid_repository(&session.identity.repository)
        || !valid_git_ref(&session.identity.git_ref)
        || !valid_workflow_path(&session.identity.workflow_path)
        || !valid_git_ref(&session.identity.workflow_ref)
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(SqlMachineTimes { now, expires })
}

pub(super) fn validate_event(
    event: &NewAuditEvent,
    action: &str,
    target_type: &str,
    target_id: &str,
    repository: &str,
    workspace_id: &str,
    created_at_ms: u64,
) -> Result<(), RepositoryError> {
    if event.action != action
        || event.target_type != target_type
        || event.workspace_id != workspace_id
        || event.created_at_ms != created_at_ms
        || event.metadata
            != [
                (
                    "repository".to_owned(),
                    AuditValue::String(repository.to_owned()),
                ),
                (
                    "targetId".to_owned(),
                    AuditValue::String(target_id.to_owned()),
                ),
            ]
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

pub(super) fn ref_matches(value: &str, glob: &str) -> bool {
    let mut remaining = value;
    let mut segments = glob.split('*').peekable();
    let first = segments.next().unwrap_or_default();
    if !remaining.starts_with(first) {
        return false;
    }
    remaining = &remaining[first.len()..];
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            return remaining.ends_with(segment);
        }
        let Some(index) = remaining.find(segment) else {
            return false;
        };
        remaining = &remaining[index + segment.len()..];
    }
    remaining.is_empty()
}

fn validate_actions(actions: &[CiAction]) -> Result<(), RepositoryError> {
    let unique = actions.iter().copied().collect::<BTreeSet<_>>();
    if actions.is_empty() || actions.len() > 4 || unique.len() != actions.len() {
        Err(RepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

fn valid_repository(value: &str) -> bool {
    let Some((owner, repository)) = value.split_once('/') else {
        return false;
    };
    value == value.to_ascii_lowercase()
        && valid_github_repository_part(owner, 39, false)
        && valid_github_repository_part(repository, 100, true)
}

fn valid_workflow_path(value: &str) -> bool {
    valid_github_workflow_path(value)
}

fn valid_git_ref(value: &str) -> bool {
    let named = ["refs/heads/", "refs/tags/", "refs/pull/"]
        .iter()
        .any(|prefix| {
            value
                .strip_prefix(prefix)
                .is_some_and(valid_github_ref_tail)
        });
    named || (value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn valid_ref_glob(value: &str) -> bool {
    ["refs/heads/", "refs/tags/", "refs/pull/"]
        .iter()
        .any(|prefix| value.strip_prefix(prefix).is_some_and(valid_glob_tail))
}

fn valid_glob_tail(value: &str) -> bool {
    !value.is_empty()
        && !value.contains("..")
        && !value.contains("**")
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b'-' | b'*')
        })
}
