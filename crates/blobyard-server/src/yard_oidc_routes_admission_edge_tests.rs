#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{
    callback, exchange_expiry, issue_redirect,
    test_support::{NOW, begin, member_fixture, missing_fixture},
};
use axum::http::StatusCode;
use blobyard_contract::YardVisibility;

#[tokio::test]
async fn missing_binding_and_admission_are_concealed() {
    let missing = missing_fixture();
    let missing_state = begin(&missing).await;
    assert_eq!(
        callback(
            &missing.state,
            Some(&format!("code=provider-code&state={missing_state}")),
            Ok(NOW + 1),
        )
        .await
        .expect("missing binding")
        .status(),
        StatusCode::OK
    );

    let selected = member_fixture();
    selected
        .state
        .repository
        .set_yard_visibility(
            "yard_oidc_fixture",
            YardVisibility::Selected,
            99,
            &blobyard_testkit::visibility_event(
                "yard_oidc_fixture",
                "any-authenticated",
                "selected",
                99,
            ),
        )
        .expect("selected visibility");
    let selected_state = begin(&selected).await;
    assert_eq!(
        callback(
            &selected.state,
            Some(&format!("code=provider-code&state={selected_state}")),
            Ok(NOW + 1),
        )
        .await
        .expect("missing admission")
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn overflow_and_duplicate_continuation_are_concealed() {
    assert_eq!(exchange_expiry(u64::MAX), u64::MAX);
    let direct = member_fixture();
    let direct_state = begin(&direct).await;
    let attempt = direct
        .state
        .repository
        .claim_yard_oidc_attempt(&crate::auth::hash(&direct_state), NOW + 1)
        .expect("claimed attempt");
    assert_eq!(
        issue_redirect(
            &direct.state,
            &attempt.attempt,
            "user_oidc_fixture",
            NOW + 1,
        )
        .expect("first continuation")
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        issue_redirect(
            &direct.state,
            &attempt.attempt,
            "user_oidc_fixture",
            NOW + 1,
        )
        .expect("duplicate continuation")
        .status(),
        StatusCode::OK
    );
}
