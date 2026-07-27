#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use crate::test_support::error_status;
use axum::http::StatusCode;

#[test]
fn guest_exchange_expiry_rejects_overflow() {
    assert_eq!(
        super::exchange_expiry(1).expect("expiry"),
        1 + blobyard_contract::YARD_EXCHANGE_CODE_LIFETIME_MS
    );
    assert_eq!(
        error_status(super::exchange_expiry(u64::MAX)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
