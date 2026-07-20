use crate::{api::AppState, error::ApiError};
use blobyard_contract::InboxRateResult;

pub(crate) fn consume(
    state: &AppState,
    key: &str,
    window_ms: u64,
    limit: u32,
    now_ms: u64,
) -> Result<(), ApiError> {
    match state
        .repository
        .consume_inbox_rate(key, window_ms, limit, now_ms)
    {
        Ok(InboxRateResult::Allowed) => Ok(()),
        Ok(InboxRateResult::Limited {
            retry_after_seconds,
        }) => Err(ApiError::rate_limited(retry_after_seconds)),
        Err(_error) => Err(ApiError::internal()),
    }
}
