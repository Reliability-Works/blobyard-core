use super::{map_error, transfer_validation};
use blobyard_contract::{InboxRateResult, RepositoryError};
use rusqlite::{Transaction, params};

pub(super) fn consume(
    transaction: &Transaction<'_>,
    rate_key: &str,
    window_ms: u64,
    limit: u32,
    now_ms: u64,
) -> Result<InboxRateResult, RepositoryError> {
    if window_ms == 0 || limit == 0 {
        return Err(RepositoryError::InvalidInput);
    }
    super::inbox_queries::validate_capability(rate_key)?;
    let now = transfer_validation::to_i64(now_ms)?;
    let window = transfer_validation::to_i64(window_ms)?;
    let current = transaction.query_row(
        "SELECT window_started_at_ms, request_count FROM inbox_rate_limits WHERE rate_key = ?1",
        [rate_key],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    );
    match current {
        Ok((started, count)) if now.saturating_sub(started) < window => {
            consume_current(transaction, rate_key, started, count, now, window, limit)
        }
        Ok(_) | Err(rusqlite::Error::QueryReturnedNoRows) => reset(transaction, rate_key, now),
        Err(error) => Err(map_error(error)),
    }
}

fn consume_current(
    transaction: &Transaction<'_>,
    rate_key: &str,
    started: i64,
    count: i64,
    now: i64,
    window: i64,
    limit: u32,
) -> Result<InboxRateResult, RepositoryError> {
    if count >= i64::from(limit) {
        let remaining_ms = started.saturating_add(window).saturating_sub(now);
        let seconds = remaining_ms.unsigned_abs().div_ceil(1_000);
        return Ok(InboxRateResult::Limited {
            retry_after_seconds: seconds,
        });
    }
    transaction
        .execute(
            "UPDATE inbox_rate_limits SET request_count = request_count + 1 WHERE rate_key = ?1",
            [rate_key],
        )
        .map_err(map_error)?;
    Ok(InboxRateResult::Allowed)
}

fn reset(
    transaction: &Transaction<'_>,
    rate_key: &str,
    now: i64,
) -> Result<InboxRateResult, RepositoryError> {
    transaction
        .execute(
            "INSERT INTO inbox_rate_limits (rate_key, window_started_at_ms, request_count) VALUES (?1, ?2, 1) ON CONFLICT(rate_key) DO UPDATE SET window_started_at_ms = excluded.window_started_at_ms, request_count = 1",
            params![rate_key, now],
        )
        .map_err(map_error)?;
    Ok(InboxRateResult::Allowed)
}

#[cfg(test)]
#[path = "inbox_rates_tests.rs"]
mod tests;
