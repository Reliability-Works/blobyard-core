use super::{map_error, rows as yard_oidc_rows, validation as yard_oidc_validation};
use blobyard_contract::{
    NewYardOidcAttempt, RepositoryError, YARD_OIDC_ATTEMPT_CLEANUP_LIMIT, YardOidcAttemptRecord,
};
use rusqlite::{Transaction, params};

pub(super) fn create(
    transaction: &Transaction<'_>,
    attempt: &NewYardOidcAttempt,
) -> Result<(), RepositoryError> {
    yard_oidc_validation::attempt(attempt)?;
    let now = super::auth_validation::sql_time(attempt.created_at_ms)?;
    cleanup(transaction, now)?;
    let changed = transaction
        .execute(
            "INSERT INTO yard_oidc_attempts
             (state_hash, continuation_hash, host_label, return_path,
              created_at_ms, expires_at_ms, claimed_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                attempt.state_hash,
                attempt.continuation_hash,
                attempt.host_label,
                attempt.return_path,
                now,
                super::auth_validation::sql_time(attempt.expires_at_ms)?,
            ],
        )
        .map_err(map_error)?;
    super::changed_once(changed)
}

pub(super) fn claim(
    transaction: &Transaction<'_>,
    state_hash: &str,
    now_ms: u64,
) -> Result<YardOidcAttemptRecord, RepositoryError> {
    super::auth_validation::validate_hash(state_hash)?;
    let now = super::auth_validation::sql_time(now_ms)?;
    cleanup(transaction, now)?;
    transaction
        .query_row(
            &format!(
                "UPDATE yard_oidc_attempts
                 SET claimed_at_ms = ?2
                 WHERE state_hash = ?1
                   AND claimed_at_ms IS NULL
                   AND created_at_ms <= ?2
                   AND expires_at_ms > ?2
                 RETURNING {}",
                yard_oidc_rows::ATTEMPT_COLUMNS
            ),
            params![state_hash, now],
            yard_oidc_rows::attempt,
        )
        .map_err(map_error)
}

fn cleanup(transaction: &Transaction<'_>, now: i64) -> Result<(), RepositoryError> {
    #[expect(
        clippy::cast_possible_wrap,
        reason = "the fixed cleanup limit is far below the signed SQLite range"
    )]
    let limit = YARD_OIDC_ATTEMPT_CLEANUP_LIMIT as i64;
    transaction
        .execute(
            "DELETE FROM yard_oidc_attempts
             WHERE state_hash IN (
               SELECT state_hash
               FROM yard_oidc_attempts
               WHERE claimed_at_ms IS NOT NULL OR expires_at_ms <= ?1
               ORDER BY expires_at_ms, created_at_ms, state_hash
               LIMIT ?2
             )",
            params![now, limit],
        )
        .map(|_changed| ())
        .map_err(map_error)
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
mod tests {
    use super::{claim, cleanup, create};
    use blobyard_contract::{
        NewYardOidcAttempt, RepositoryError, YARD_OIDC_ATTEMPT_CLEANUP_LIMIT,
        YARD_OIDC_ATTEMPT_LIFETIME_MS,
    };
    use rusqlite::Connection;

    fn valid_attempt() -> NewYardOidcAttempt {
        NewYardOidcAttempt {
            state_hash: "a".repeat(64),
            continuation_hash: "b".repeat(64),
            host_label: "yard-123456789-fixture".to_owned(),
            return_path: "/reports".to_owned(),
            created_at_ms: 1,
            expires_at_ms: 1 + YARD_OIDC_ATTEMPT_LIFETIME_MS,
        }
    }

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().expect("connection");
        connection
            .execute_batch(
                "CREATE TABLE yard_oidc_attempts (
                   state_hash TEXT PRIMARY KEY,
                   continuation_hash TEXT,
                   host_label TEXT,
                   return_path TEXT,
                   created_at_ms INTEGER,
                   expires_at_ms INTEGER,
                   claimed_at_ms INTEGER
                 );",
            )
            .expect("schema");
        connection
    }

    #[test]
    fn cleanup_is_bounded_and_maps_provider_failures() {
        let mut connection = connection();
        for index in 0..=YARD_OIDC_ATTEMPT_CLEANUP_LIMIT {
            connection
                .execute(
                    "INSERT INTO yard_oidc_attempts VALUES (?1, ?1, 'host', '/', 0, 1, NULL)",
                    [format!("{index:064x}")],
                )
                .expect("attempt");
        }
        let transaction = connection.transaction().expect("transaction");
        cleanup(&transaction, 2).expect("cleanup");
        let retained: i64 = transaction
            .query_row("SELECT COUNT(*) FROM yard_oidc_attempts", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(retained, 1);
        transaction.commit().expect("commit");

        let transaction = connection.transaction().expect("transaction");
        transaction
            .execute_batch("DROP TABLE yard_oidc_attempts")
            .expect("drop");
        assert_eq!(
            cleanup(&transaction, 2),
            Err(blobyard_contract::RepositoryError::Unavailable)
        );
    }

    #[test]
    fn attempt_operations_propagate_validation_and_time_failures() {
        let mut connection = connection();
        let transaction = connection.transaction().expect("transaction");
        assert_eq!(
            create(
                &transaction,
                &NewYardOidcAttempt {
                    state_hash: "short".to_owned(),
                    ..valid_attempt()
                },
            ),
            Err(RepositoryError::InvalidInput)
        );

        let created_at_ms = (i64::MAX as u64) + 1;
        assert_eq!(
            create(
                &transaction,
                &NewYardOidcAttempt {
                    created_at_ms,
                    expires_at_ms: created_at_ms + YARD_OIDC_ATTEMPT_LIFETIME_MS,
                    ..valid_attempt()
                },
            ),
            Err(RepositoryError::InvalidInput)
        );

        let created_at_ms = (i64::MAX as u64) - 100;
        assert_eq!(
            create(
                &transaction,
                &NewYardOidcAttempt {
                    created_at_ms,
                    expires_at_ms: created_at_ms + YARD_OIDC_ATTEMPT_LIFETIME_MS,
                    ..valid_attempt()
                },
            ),
            Err(RepositoryError::InvalidInput)
        );

        assert_eq!(
            claim(&transaction, "short", 1),
            Err(RepositoryError::InvalidInput)
        );
        assert_eq!(
            claim(&transaction, &"a".repeat(64), (i64::MAX as u64) + 1),
            Err(RepositoryError::InvalidInput)
        );
    }
}
