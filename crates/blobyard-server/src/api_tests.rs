#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::{not_found, reject_cursor, who_am_i};
use crate::{
    auth::Principal, repository_fault_tests::FaultingRepository, slug, transfers::test_seams,
};
use axum::{extract::State, http::StatusCode, response::IntoResponse};
use std::sync::Arc;

#[tokio::test]
async fn fallback_and_input_helpers_return_stable_statuses() {
    assert_eq!(
        not_found().await.into_response().status(),
        StatusCode::NOT_FOUND
    );
    assert!(slug::validate_name("Fixture").is_ok());
    for value in ["", &"x".repeat(129), "line\nbreak"] {
        assert_eq!(
            slug::validate_name(value)
                .expect_err("invalid name")
                .into_response()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert!(reject_cursor(None).is_ok());
    assert!(reject_cursor(Some("next")).is_err());
    assert_eq!(
        slug::parse("fixture".to_owned())
            .expect("valid slug")
            .as_str(),
        "fixture"
    );
    assert!(slug::parse("Invalid Slug".to_owned()).is_err());
}

#[tokio::test]
async fn identity_maps_workspace_repository_failures() {
    let fixture = test_seams::fixture(&["workspace:read"]);
    let mut state = fixture.state.clone();
    state.repository = Arc::new(FaultingRepository::new(state.repository.clone(), 0));
    assert!(
        who_am_i(State(state), Principal(fixture.principal.clone()),)
            .await
            .is_err()
    );
}
