use super::auth_validation::{
    optional_sql_time, sql_time, validate_hash, validate_session, validate_session_event,
    validate_token, validate_token_event,
};
use super::{SqliteRepository, auth_machine, lifecycle_audit, map_error, rows};
use blobyard_contract::{
    CredentialRepository, LocalApiTokenRecord, LocalCliSessionRecord, NewAuditEvent,
    RepositoryError,
};
use rusqlite::{Connection, Statement, params};

impl CredentialRepository for SqliteRepository {
    fn install_bootstrap(&self, secret_hash: &str) -> Result<bool, RepositoryError> {
        validate_hash(secret_hash)?;
        self.connection()?
            .execute(
                "INSERT INTO bootstrap_authority (id, secret_hash, consumed) VALUES (1, ?1, 0) ON CONFLICT(id) DO NOTHING",
                [secret_hash],
            )
            .map(|count| count == 1)
            .map_err(map_error)
    }

    fn exchange_bootstrap(
        &self,
        bootstrap_hash: &str,
        token: &LocalApiTokenRecord,
        session: &LocalCliSessionRecord,
    ) -> Result<(), RepositoryError> {
        validate_hash(bootstrap_hash)?;
        validate_token(token)?;
        validate_session(session, token)?;
        let mut connection = self.connection()?;
        let result = exchange_transaction(&mut connection, bootstrap_hash, token, session);
        drop(connection);
        result
    }

    fn list_cli_sessions(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalCliSessionRecord>, RepositoryError> {
        rows::validate_text(workspace_id)?;
        let connection = self.connection()?;
        let result = {
            let mut statement = connection
                .prepare(&format!(
                    "SELECT {} FROM cli_sessions WHERE workspace_id = ?1 AND revoked_at_ms IS NULL ORDER BY created_at_ms DESC, id DESC",
                    rows::CLI_SESSION_COLUMNS
                ))
                .map_err(map_error)?;
            query_cli_sessions(&mut statement, workspace_id)
        };
        drop(connection);
        result
    }

    fn revoke_cli_session(
        &self,
        id: &str,
        workspace_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        rows::validate_text(id)?;
        rows::validate_text(workspace_id)?;
        let now = sql_time(now_ms)?;
        self.write_transaction(|transaction| {
            let (token_id, revoked_at_ms) = transaction
                .query_row(
                    "SELECT token_id, revoked_at_ms FROM cli_sessions WHERE id = ?1 AND workspace_id = ?2",
                    params![id, workspace_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .map_err(map_error)?;
            if revoked_at_ms.is_some() {
                return Err(RepositoryError::Conflict);
            }
            validate_session_event(event, id, workspace_id, now_ms)?;
            let session_changed = transaction
                .execute(
                    "UPDATE cli_sessions SET revoked_at_ms = ?3 WHERE id = ?1 AND workspace_id = ?2 AND revoked_at_ms IS NULL",
                    params![id, workspace_id, now],
                )
                .map_err(map_error)?;
            let token_changed = transaction
                .execute(
                    "UPDATE api_tokens SET revoked = 1, revoked_at_ms = ?2 WHERE id = ?1 AND revoked = 0",
                    params![token_id, now],
                )
                .map_err(map_error)?;
            if session_changed != 1 || token_changed != 1 {
                return Err(RepositoryError::Conflict);
            }
            lifecycle_audit::insert(transaction, event)
        })
    }

    fn create_api_token(
        &self,
        token: &LocalApiTokenRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        validate_token(token)?;
        validate_token_event(
            event,
            "api_token.created",
            &token.id,
            &token.workspace_id,
            token.created_at_ms,
        )?;
        self.write_transaction(|transaction| {
            insert_token(transaction, token)?;
            lifecycle_audit::insert(transaction, event)
        })
    }

    fn list_api_tokens(&self) -> Result<Vec<LocalApiTokenRecord>, RepositoryError> {
        let connection = self.connection()?;
        let result = {
            let mut statement = connection
                .prepare(&format!(
                    "SELECT {} FROM api_tokens WHERE NOT EXISTS (SELECT 1 FROM cli_sessions WHERE cli_sessions.token_id = api_tokens.id) AND NOT EXISTS (SELECT 1 FROM machine_sessions WHERE machine_sessions.token_id = api_tokens.id) ORDER BY created_at_ms DESC, id DESC",
                    rows::API_TOKEN_COLUMNS
                ))
                .map_err(map_error)?;
            query_api_tokens(&mut statement)
        };
        drop(connection);
        result
    }

    fn authenticate_api_token(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<LocalApiTokenRecord, RepositoryError> {
        validate_hash(secret_hash)?;
        let now = sql_time(now_ms)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(map_error)?;
        let token = transaction
            .query_row(
                &format!(
                    "UPDATE api_tokens SET last_used_at_ms = CASE WHEN last_used_at_ms IS NULL OR last_used_at_ms < ?2 THEN ?2 ELSE last_used_at_ms END WHERE secret_hash = ?1 AND revoked = 0 AND created_at_ms <= ?2 AND expires_at_ms > ?2 RETURNING {}",
                    rows::API_TOKEN_COLUMNS
                ),
                params![secret_hash, now],
                rows::api_token,
            )
            .map_err(map_error)?;
        auth_machine::validate_and_touch(&transaction, &token.id, now_ms, now)?;
        transaction
            .execute(
                "UPDATE cli_sessions SET last_used_at_ms = CASE WHEN last_used_at_ms IS NULL OR last_used_at_ms < ?2 THEN ?2 ELSE last_used_at_ms END WHERE token_id = ?1 AND revoked_at_ms IS NULL",
                params![token.id, now],
            )
            .map_err(map_error)?;
        transaction.commit().map_err(map_error)?;
        drop(connection);
        Ok(token)
    }

    fn revoke_api_token(
        &self,
        id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        rows::validate_text(id)?;
        let now = sql_time(now_ms)?;
        self.write_transaction(|transaction| {
            let mut statement = transaction
                .prepare("SELECT workspace_id, revoked FROM api_tokens WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM cli_sessions WHERE cli_sessions.token_id = api_tokens.id) AND NOT EXISTS (SELECT 1 FROM machine_sessions WHERE machine_sessions.token_id = api_tokens.id)")
                .map_err(map_error)?;
            let (workspace_id, revoked) = query_token_revocation(&mut statement, id)?;
            drop(statement);
            if revoked {
                return Err(RepositoryError::Conflict);
            }
            validate_token_event(
                event,
                "api_token.revoked",
                id,
                &workspace_id,
                now_ms,
            )?;
            transaction
                .execute(
                    "UPDATE api_tokens SET revoked = 1, revoked_at_ms = ?2 WHERE id = ?1 AND revoked = 0",
                    params![id, now],
                )
                .map_err(map_error)?;
            lifecycle_audit::insert(transaction, event)
        })
    }
}

fn query_cli_sessions(
    statement: &mut Statement<'_>,
    workspace_id: &str,
) -> Result<Vec<LocalCliSessionRecord>, RepositoryError> {
    statement
        .query_map([workspace_id], rows::cli_session)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

pub(super) fn query_api_tokens(
    statement: &mut Statement<'_>,
) -> Result<Vec<LocalApiTokenRecord>, RepositoryError> {
    statement
        .query_map([], rows::api_token)
        .map_err(map_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)
}

pub(super) fn query_token_revocation(
    statement: &mut Statement<'_>,
    id: &str,
) -> Result<(String, bool), RepositoryError> {
    statement
        .query_row([id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })
        .map_err(map_error)
}

fn exchange_transaction(
    connection: &mut Connection,
    bootstrap_hash: &str,
    token: &LocalApiTokenRecord,
    session: &LocalCliSessionRecord,
) -> Result<(), RepositoryError> {
    let transaction = connection.transaction().map_err(map_error)?;
    let changed = transaction
        .execute(
            "UPDATE bootstrap_authority SET secret_hash = NULL, consumed = 1 WHERE id = 1 AND consumed = 0 AND secret_hash = ?1",
            [bootstrap_hash],
        )
        .map_err(map_error)?;
    if changed != 1 {
        return Err(RepositoryError::NotFound);
    }
    insert_token(&transaction, token)?;
    insert_session(&transaction, session)?;
    transaction.commit().map_err(map_error)
}

fn insert_session(
    connection: &Connection,
    session: &LocalCliSessionRecord,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "INSERT INTO cli_sessions (id, token_id, workspace_id, name, platform, version, created_at_ms, last_used_at_ms, revoked_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session.id,
                session.token_id,
                session.workspace_id,
                session.name,
                session.platform,
                session.version,
                sql_time(session.created_at_ms)?,
                optional_sql_time(session.last_used_at_ms)?,
                optional_sql_time(session.revoked_at_ms)?,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

pub(super) fn insert_token(
    connection: &Connection,
    token: &LocalApiTokenRecord,
) -> Result<(), RepositoryError> {
    connection
        .execute(
            "INSERT INTO api_tokens (id, name, secret_hash, scopes, workspace_id, token_prefix, project_id, created_at_ms, expires_at_ms, last_used_at_ms, revoked, revoked_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                token.id,
                token.name,
                token.secret_hash,
                token.scopes.join("\n"),
                token.workspace_id,
                token.token_prefix,
                token.project_id,
                sql_time(token.created_at_ms)?,
                sql_time(token.expires_at_ms)?,
                optional_sql_time(token.last_used_at_ms)?,
                i64::from(token.revoked_at_ms.is_some()),
                optional_sql_time(token.revoked_at_ms)?,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
