use super::auth_validation::{sql_time, validate_hash};
use super::{SqliteRepository, lifecycle_audit, map_error, rows, validate_record, yard_rows};
use blobyard_contract::{
    AuditValue, LocalUserListing, LocalUserLoginKeyRecord, LocalUserRecord, LocalUserRepository,
    LocalUserStatus, NewAuditEvent, RepositoryError,
};
use rusqlite::{OptionalExtension, Row, Statement, Transaction, params};

const USER_COLUMNS: &str =
    "id, workspace_id, display_name, email, status, created_at_ms, deactivated_at_ms";

impl LocalUserRepository for SqliteRepository {
    fn create_local_user(
        &self,
        user: &LocalUserRecord,
        key: &LocalUserLoginKeyRecord,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        validate_new_user(user)?;
        validate_new_key(key)?;
        if key.user_id != user.id {
            return Err(RepositoryError::InvalidInput);
        }
        if key.created_at_ms != user.created_at_ms {
            return Err(RepositoryError::InvalidInput);
        }
        validate_user_event(event, "user.created", user, user.created_at_ms)?;
        self.write_transaction(|transaction| {
            require_workspace(transaction, &user.workspace_id)?;
            let created_at_ms = sql_time(key.created_at_ms)?;
            insert_user(transaction, user, created_at_ms)?;
            insert_key(transaction, key, created_at_ms)?;
            lifecycle_audit::insert(transaction, event)
        })
    }

    fn list_local_users(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<LocalUserListing>, RepositoryError> {
        rows::validate_text(workspace_id)?;
        let connection = self.connection()?;
        let result = {
            let mut statement = connection
                .prepare(&format!(
                    "SELECT {USER_COLUMNS}, (SELECT token_prefix FROM local_user_login_keys WHERE user_id = local_users.id AND revoked_at_ms IS NULL ORDER BY created_at_ms DESC, id DESC LIMIT 1) FROM local_users WHERE workspace_id = ?1 ORDER BY created_at_ms DESC, id DESC"
                ))
                .map_err(map_error)?;
            query_listings(&mut statement, workspace_id)
        };
        drop(connection);
        result
    }

    fn reset_local_user_login_key(
        &self,
        key: &LocalUserLoginKeyRecord,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        validate_new_key(key)?;
        if key.created_at_ms != now_ms {
            return Err(RepositoryError::InvalidInput);
        }
        let now = sql_time(now_ms)?;
        self.write_transaction(|transaction| {
            let user = active_user(transaction, &key.user_id)?;
            validate_user_event(event, "user.login_key_reset", &user, now_ms)?;
            revoke_active_keys(transaction, &user.id, now)?;
            insert_key(transaction, key, now)?;
            lifecycle_audit::insert(transaction, event)
        })
    }

    fn deactivate_local_user(
        &self,
        user_id: &str,
        now_ms: u64,
        event: &NewAuditEvent,
    ) -> Result<(), RepositoryError> {
        rows::validate_text(user_id)?;
        let now = sql_time(now_ms)?;
        self.write_transaction(|transaction| {
            let user = active_user(transaction, user_id)?;
            validate_user_event(event, "user.deactivated", &user, now_ms)?;
            transaction
                .execute(
                    "UPDATE local_users SET status = 'deactivated', deactivated_at_ms = ?2 WHERE id = ?1 AND status = 'active'",
                    params![user.id, now],
                )
                .map_err(map_error)?;
            revoke_active_keys(transaction, &user.id, now)?;
            lifecycle_audit::insert(transaction, event)
        })
    }

    fn authenticate_local_user_key(
        &self,
        secret_hash: &str,
        now_ms: u64,
    ) -> Result<LocalUserRecord, RepositoryError> {
        validate_hash(secret_hash)?;
        let now = sql_time(now_ms)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(map_error)?;
        let user_id: String = transaction
            .query_row(
                "UPDATE local_user_login_keys SET last_used_at_ms = CASE WHEN last_used_at_ms IS NULL OR last_used_at_ms < ?2 THEN ?2 ELSE last_used_at_ms END WHERE secret_hash = ?1 AND revoked_at_ms IS NULL AND created_at_ms <= ?2 AND expires_at_ms > ?2 AND EXISTS (SELECT 1 FROM local_users WHERE local_users.id = local_user_login_keys.user_id AND local_users.status = 'active') RETURNING user_id",
                params![secret_hash, now],
                |row| row.get(0),
            )
            .map_err(map_error)?;
        let user = transaction
            .query_row(
                &format!("SELECT {USER_COLUMNS} FROM local_users WHERE id = ?1"),
                [user_id],
                user_row,
            )
            .map_err(map_error)?;
        transaction.commit().map_err(map_error)?;
        drop(connection);
        Ok(user)
    }
}

fn validate_new_user(user: &LocalUserRecord) -> Result<(), RepositoryError> {
    validate_record(&user.id, &user.display_name)?;
    rows::validate_text(&user.workspace_id)?;
    if let Some(email) = user.email.as_deref() {
        rows::validate_text(email)?;
    }
    if user.status != LocalUserStatus::Active || user.deactivated_at_ms.is_some() {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_new_key(key: &LocalUserLoginKeyRecord) -> Result<(), RepositoryError> {
    validate_record(&key.id, &key.user_id)?;
    rows::validate_text(&key.token_prefix)?;
    validate_hash(&key.secret_hash)?;
    if key.created_at_ms >= key.expires_at_ms
        || key.last_used_at_ms.is_some()
        || key.revoked_at_ms.is_some()
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_user_event(
    event: &NewAuditEvent,
    action: &str,
    user: &LocalUserRecord,
    created_at_ms: u64,
) -> Result<(), RepositoryError> {
    if event.action != action
        || event.target_type != "local_user"
        || event.workspace_id != user.workspace_id
        || event.created_at_ms != created_at_ms
        || event.metadata != [("userId".to_owned(), AuditValue::String(user.id.clone()))]
    {
        return Err(RepositoryError::InvalidInput);
    }
    Ok(())
}

fn require_workspace(
    transaction: &Transaction<'_>,
    workspace_id: &str,
) -> Result<(), RepositoryError> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
            [workspace_id],
            |row| row.get(0),
        )
        .map_err(map_error)?;
    if exists {
        Ok(())
    } else {
        Err(RepositoryError::NotFound)
    }
}

fn active_user(
    transaction: &Transaction<'_>,
    user_id: &str,
) -> Result<LocalUserRecord, RepositoryError> {
    let user = transaction
        .query_row(
            &format!("SELECT {USER_COLUMNS} FROM local_users WHERE id = ?1"),
            [user_id],
            user_row,
        )
        .optional()
        .map_err(map_error)?
        .ok_or(RepositoryError::NotFound)?;
    if user.status == LocalUserStatus::Active {
        Ok(user)
    } else {
        Err(RepositoryError::Conflict)
    }
}

fn revoke_active_keys(
    transaction: &Transaction<'_>,
    user_id: &str,
    now: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "UPDATE local_user_login_keys SET revoked_at_ms = ?2 WHERE user_id = ?1 AND revoked_at_ms IS NULL",
            params![user_id, now],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn insert_user(
    transaction: &Transaction<'_>,
    user: &LocalUserRecord,
    created_at_ms: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO local_users (id, workspace_id, display_name, email, status, created_at_ms, deactivated_at_ms) VALUES (?1, ?2, ?3, ?4, 'active', ?5, NULL)",
            params![
                user.id,
                user.workspace_id,
                user.display_name,
                user.email,
                created_at_ms,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn insert_key(
    transaction: &Transaction<'_>,
    key: &LocalUserLoginKeyRecord,
    created_at_ms: i64,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            "INSERT INTO local_user_login_keys (id, user_id, token_prefix, secret_hash, created_at_ms, expires_at_ms, last_used_at_ms, revoked_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL)",
            params![
                key.id,
                key.user_id,
                key.token_prefix,
                key.secret_hash,
                created_at_ms,
                sql_time(key.expires_at_ms)?,
            ],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

fn query_listings(
    statement: &mut Statement<'_>,
    workspace_id: &str,
) -> Result<Vec<LocalUserListing>, RepositoryError> {
    statement
        .query_map([workspace_id], listing_row)
        .map_err(map_error)
        .and_then(super::collect)
}

fn listing_row(row: &Row<'_>) -> rusqlite::Result<LocalUserListing> {
    Ok(LocalUserListing {
        user: user_row(row)?,
        active_key_prefix: row.get(7)?,
    })
}

fn user_row(row: &Row<'_>) -> rusqlite::Result<LocalUserRecord> {
    let status: String = row.get(4)?;
    Ok(LocalUserRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        display_name: row.get(2)?,
        email: row.get(3)?,
        status: LocalUserStatus::parse(&status).ok_or_else(|| rows::conversion_error(status))?,
        created_at_ms: yard_rows::required_u64(row.get(5)?)?,
        deactivated_at_ms: yard_rows::optional_u64(row.get(6)?)?,
    })
}

#[cfg(test)]
#[path = "local_users_row_tests.rs"]
mod tests;
