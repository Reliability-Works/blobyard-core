#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::values;
use crate::{
    Repository, repository_fault_tests::FaultingRepository, test_support::error_status,
    transfers::test_seams,
};
use axum::http::StatusCode;
use blobyard_contract::YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS;
use std::sync::Arc;

#[test]
fn invitation_values_propagate_live_target_repository_failures() {
    let (fixture, raw_token, continuation, _expires_at_ms) = invitation_fixture('e');
    let inner: Arc<dyn Repository> = Arc::clone(&fixture.state.repository);
    let mut faulted = fixture.state.clone();
    faulted.repository = Arc::new(FaultingRepository::new(inner, 1));
    assert_eq!(
        error_status(values(&faulted, raw_token, continuation, 2)),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn invitation_continuation_remains_usable_after_ten_minutes_until_exact_expiry() {
    let (fixture, raw_token, continuation, expires_at_ms) = invitation_fixture('c');
    assert!(matches!(
        values(
            &fixture.state,
            raw_token.clone(),
            continuation.clone(),
            600_002,
        ),
        Ok(Some(_))
    ));
    assert!(matches!(
        values(&fixture.state, raw_token, continuation, expires_at_ms,),
        Ok(None)
    ));
}

fn invitation_fixture(marker: char) -> (test_seams::TransferFixture, String, String, u64) {
    let fixture = test_seams::fixture(&["yard:read"]);
    let started = super::super::test_support::start_yard(&fixture.state);
    let raw_token = format!("bygi_{}", marker.to_string().repeat(64));
    let expires_at_ms = 1 + YARD_GUEST_INVITE_DEFAULT_LIFETIME_MS;
    let _record = super::super::test_support::create_invitation(
        &fixture.state,
        &started.yard,
        &raw_token,
        expires_at_ms,
    );
    let continuation = crate::yard_session_contracts::issue_invitation(
        &fixture.state.yard_continuation_key,
        &started.yard.host_label,
        "/",
        1,
        expires_at_ms,
    )
    .expect("continuation");
    (
        fixture,
        raw_token,
        continuation.expose_secret().to_owned(),
        expires_at_ms,
    )
}
