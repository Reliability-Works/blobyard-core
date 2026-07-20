#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::*;
use rusqlite::Connection;

fn rate_table(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE inbox_rate_limits (rate_key, window_started_at_ms, request_count);",
        )
        .expect("rate table");
}

fn consume_once(connection: &mut Connection) -> Result<InboxRateResult, RepositoryError> {
    let transaction = connection.transaction().expect("transaction");
    consume(&transaction, &"a".repeat(64), 1_000, 2, 1_000)
}

#[test]
fn rate_rows_and_missing_tables_fail_closed() {
    let mut missing = Connection::open_in_memory().expect("database");
    assert_eq!(
        consume_once(&mut missing),
        Err(RepositoryError::Unavailable)
    );

    for values in [("x'00'", "1"), ("1000", "x'00'")] {
        let mut connection = Connection::open_in_memory().expect("database");
        rate_table(&connection);
        connection
            .execute_batch(&format!(
                "INSERT INTO inbox_rate_limits VALUES ('{}', {}, {});",
                "a".repeat(64),
                values.0,
                values.1,
            ))
            .expect("corrupt rate row");
        assert_eq!(
            consume_once(&mut connection),
            Err(RepositoryError::Unavailable)
        );
    }
}

#[test]
fn rate_mutation_failures_and_clock_rollback_are_explicit() {
    let mut update = Connection::open_in_memory().expect("database");
    rate_table(&update);
    update
        .execute_batch(&format!(
            "INSERT INTO inbox_rate_limits VALUES ('{}', 1000, 1);
             CREATE TRIGGER reject_rate_update BEFORE UPDATE ON inbox_rate_limits BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
            "a".repeat(64),
        ))
        .expect("update fixture");
    assert_eq!(consume_once(&mut update), Err(RepositoryError::Conflict));

    let mut reset = Connection::open_in_memory().expect("database");
    rate_table(&reset);
    reset
        .execute_batch(
            "CREATE TRIGGER reject_rate_insert BEFORE INSERT ON inbox_rate_limits BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
        )
        .expect("reset fixture");
    assert_eq!(consume_once(&mut reset), Err(RepositoryError::Unavailable));

    let mut rollback = Connection::open_in_memory().expect("database");
    rate_table(&rollback);
    rollback
        .execute_batch(&format!(
            "INSERT INTO inbox_rate_limits VALUES ('{}', 2000, 2);",
            "a".repeat(64),
        ))
        .expect("rollback fixture");
    assert_eq!(
        consume_once(&mut rollback),
        Ok(InboxRateResult::Limited {
            retry_after_seconds: 2,
        })
    );
}
