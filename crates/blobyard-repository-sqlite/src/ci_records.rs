use super::{ci_validation, map_error, rows};
use blobyard_contract::{
    LocalApiTokenRecord, LocalCiTrustRecord, LocalMachineSessionRecord, NewMachineSession,
    RepositoryError,
};
use rusqlite::{Connection, Statement, ToSql, params};

pub(super) fn query_trusts(
    statement: &mut Statement<'_>,
    parameters: &[&dyn ToSql],
) -> Result<Vec<LocalCiTrustRecord>, RepositoryError> {
    statement
        .query_map(parameters, rows::ci_trust)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

pub(super) fn insert_trust(
    connection: &Connection,
    trust: &LocalCiTrustRecord,
    created_at: i64,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "INSERT INTO ci_trusts (id, workspace_id, project_id, repository, workflow_path, workflow_ref, allowed_ref_glob, environment, allowed_actions, audience, created_at_ms, revoked_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                trust.id,
                trust.workspace_id,
                trust.project_id,
                trust.repository,
                trust.workflow_path,
                trust.workflow_ref,
                trust.allowed_ref_glob,
                trust.environment,
                actions(&trust.allowed_actions),
                trust.audience,
                created_at,
                Option::<i64>::None,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

pub(super) fn trust_by_id(
    connection: &Connection,
    id: &str,
    workspace_id: &str,
) -> Result<LocalCiTrustRecord, RepositoryError> {
    connection
        .query_row(
            &format!(
                "SELECT {} FROM ci_trusts WHERE id = ?1 AND workspace_id = ?2",
                rows::CI_TRUST_COLUMNS
            ),
            params![id, workspace_id],
            rows::ci_trust,
        )
        .map_err(map_error)
}

pub(super) fn require_project_scope(
    connection: &Connection,
    project_id: Option<&str>,
    workspace_id: &str,
) -> Result<(), RepositoryError> {
    let Some(project_id) = project_id else {
        return Ok(());
    };
    connection
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1 AND workspace_id = ?2",
            params![project_id, workspace_id],
            |_row| Ok(()),
        )
        .map_err(map_error)
}

pub(super) fn machine_record(
    session: &NewMachineSession,
    trust: &LocalCiTrustRecord,
    project_id: String,
) -> LocalMachineSessionRecord {
    LocalMachineSessionRecord {
        id: session.id.clone(),
        trust_id: trust.id.clone(),
        workspace_id: trust.workspace_id.clone(),
        project_id,
        repository: session.identity.repository.clone(),
        git_ref: session.identity.git_ref.clone(),
        run_id: session.identity.run_id.clone(),
        run_attempt: session.identity.run_attempt.clone(),
        actions: session.actions.clone(),
        created_at_ms: session.now_ms,
        expires_at_ms: session.identity.expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

pub(super) fn machine_token(
    session: &NewMachineSession,
    record: &LocalMachineSessionRecord,
) -> LocalApiTokenRecord {
    LocalApiTokenRecord {
        id: session.id.clone(),
        name: format!("GitHub Actions {} run {}", record.repository, record.run_id),
        token_prefix: session.token_prefix.clone(),
        secret_hash: session.secret_hash.clone(),
        scopes: record
            .actions
            .iter()
            .map(|action| action.as_str().to_owned())
            .collect(),
        workspace_id: record.workspace_id.clone(),
        project_id: Some(record.project_id.clone()),
        created_at_ms: record.created_at_ms,
        expires_at_ms: record.expires_at_ms,
        last_used_at_ms: None,
        revoked_at_ms: None,
    }
}

pub(super) fn insert_machine(
    connection: &Connection,
    session: &NewMachineSession,
    record: &LocalMachineSessionRecord,
    times: ci_validation::SqlMachineTimes,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "INSERT INTO machine_sessions (id, token_id, trust_id, workspace_id, project_id, repository, git_ref, run_id, run_attempt, actions, oidc_token_hash, created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                record.id,
                record.trust_id,
                record.workspace_id,
                record.project_id,
                record.repository,
                record.git_ref,
                record.run_id,
                record.run_attempt,
                actions(&record.actions),
                session.oidc_token_hash,
                times.now,
                times.expires,
                Option::<i64>::None,
                Option::<i64>::None,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn actions(actions: &[blobyard_contract::CiAction]) -> String {
    actions
        .iter()
        .map(|action| action.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
