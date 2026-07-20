use crate::error::ApiError;

pub(crate) fn bounded_expiry(
    now: u64,
    duration: Option<&str>,
    default_lifetime_ms: u64,
    maximum_lifetime_ms: u64,
) -> Result<u64, ApiError> {
    let lifetime = match duration {
        None => default_lifetime_ms,
        Some(value) => parse_duration(value)?,
    };
    if lifetime > maximum_lifetime_ms {
        return Err(ApiError::invalid_request());
    }
    now.checked_add(lifetime)
        .ok_or_else(ApiError::invalid_request)
}

fn parse_duration(value: &str) -> Result<u64, ApiError> {
    let split = value
        .len()
        .checked_sub(1)
        .ok_or_else(ApiError::invalid_request)?;
    let (amount, unit) = value.split_at(split);
    if amount.is_empty()
        || !amount.bytes().all(|byte| byte.is_ascii_digit())
        || amount.starts_with('0')
    {
        return Err(ApiError::invalid_request());
    }
    let amount = amount
        .parse::<u64>()
        .map_err(|_error| ApiError::invalid_request())?;
    let multiplier = match unit {
        "d" => 24 * 60 * 60 * 1_000,
        "h" => 60 * 60 * 1_000,
        "m" => 60 * 1_000,
        "s" => 1_000,
        _ => return Err(ApiError::invalid_request()),
    };
    amount
        .checked_mul(multiplier)
        .ok_or_else(ApiError::invalid_request)
}
