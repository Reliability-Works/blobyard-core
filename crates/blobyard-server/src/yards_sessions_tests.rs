#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]

use super::super::sessions::{list, revoke, status, summary};
use super::{
    faulted_state,
    session_support::{SessionFixture, setup, sign_in},
};
use crate::{
    Repository,
    auth::Principal,
    repository_fault_tests::{Corruption, FaultingRepository},
    test_support::error_status,
};
use axum::http::StatusCode;
use blobyard_api_client::{
    ListYardSessionsQuery, RevokeYardSessionRequest, YardSessionStatus as ApiStatus,
};
use blobyard_contract::{YardSessionListing, YardSessionRecord, YardSessionStatus};
use std::sync::Arc;

fn listing() -> YardSessionListing {
    YardSessionListing {
        session: YardSessionRecord {
            id: "yardsession_fixture".to_owned(),
            token_hash: "a".repeat(64),
            yard_id: "yard_fixture".to_owned(),
            environment_id: "yardenv_fixture".to_owned(),
            host_label: "docs-fixture".to_owned(),
            user_id: "user_fixture".to_owned(),
            created_at_ms: 1,
            expires_at_ms: 2,
            last_used_at_ms: Some(1),
            revoked_at_ms: None,
        },
        user_display_name: "Fixture reader".to_owned(),
    }
}

struct OperationFixture {
    session: SessionFixture,
    principal: Principal,
    query: ListYardSessionsQuery,
    request: RevokeYardSessionRequest,
}

async fn operation_fixture() -> OperationFixture {
    let session = setup().await;
    let _signed_in = sign_in(&session, "/").await;
    let principal = Principal(session.fixture.principal.clone());
    let query = ListYardSessionsQuery {
        yard_id: session.yard_id.clone(),
    };
    let request = RevokeYardSessionRequest {
        yard_id: session.yard_id.clone(),
        session_id: "yardsession_missing".to_owned(),
    };
    OperationFixture {
        session,
        principal,
        query,
        request,
    }
}

#[test]
fn session_summaries_cover_statuses_and_invalid_timestamps() {
    assert_eq!(status(YardSessionStatus::Active), ApiStatus::Active);
    assert_eq!(status(YardSessionStatus::Expired), ApiStatus::Expired);
    assert_eq!(status(YardSessionStatus::Revoked), ApiStatus::Revoked);

    for mutate in [
        |value: &mut YardSessionListing| value.session.created_at_ms = u64::MAX,
        |value: &mut YardSessionListing| value.session.expires_at_ms = u64::MAX,
        |value: &mut YardSessionListing| value.session.last_used_at_ms = Some(u64::MAX),
    ] {
        let mut invalid = listing();
        mutate(&mut invalid);
        assert_eq!(
            error_status(summary(invalid, 1)),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[tokio::test]
async fn session_management_maps_lookup_and_clock_failures() {
    let OperationFixture {
        session,
        principal,
        query,
        request,
    } = operation_fixture().await;
    let mut foreign = principal.clone();
    foreign.0.workspace_id = "workspace_foreign".to_owned();
    assert_eq!(
        error_status(list(&session.fixture.state, &foreign, &query, Ok(1))),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        error_status(revoke(&session.fixture.state, &foreign, &request, Ok(1),)),
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        error_status(list(
            &session.fixture.state,
            &principal,
            &query,
            Err(crate::error::ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        error_status(revoke(
            &session.fixture.state,
            &principal,
            &request,
            Err(crate::error::ApiError::internal()),
        )),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn session_management_maps_repository_and_summary_failures() {
    let OperationFixture {
        session,
        principal,
        query,
        request,
    } = operation_fixture().await;
    for failure_index in [0, 1] {
        assert_eq!(
            error_status(list(
                &faulted_state(&session.fixture, failure_index),
                &principal,
                &query,
                Ok(1),
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            error_status(revoke(
                &faulted_state(&session.fixture, failure_index),
                &principal,
                &request,
                Ok(1),
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    let inner: Arc<dyn Repository> = Arc::clone(&session.fixture.state.repository);
    let mut corrupt = session.fixture.state.clone();
    corrupt.repository = Arc::new(FaultingRepository::corrupting(
        inner,
        Corruption::YardSessionCreatedAt,
    ));
    assert_eq!(
        error_status(list(&corrupt, &principal, &query, Ok(1))),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
